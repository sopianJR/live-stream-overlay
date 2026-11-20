//! Koneksi realtime ke Trakteer untuk menampilkan alert donasi di overlay.
//!
//! Satu task di latar menjaga koneksi WebSocket (V1 Pusher atau V2), mencoba ulang dengan
//! backoff 1,5 detik ×1,6 (maks. 10 detik), lalu menyiarkan:
//! - `trakteer://status`: keadaan koneksi;
//! - `trakteer://donation`: donasi yang lolos filter, untuk widget overlay;
//! - `trakteer://recent`: 20 donasi terakhir, untuk dashboard.
//!
//! Kunci (Channel ID / Stream Key) disimpan di Credential Manager dan tidak pernah dikirim
//! ke frontend. Log hanya mencatat nominal, bukan nama atau pesan donatur.

pub mod protocol;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_websockets::{ClientBuilder, Message};

use crate::secrets;
use crate::settings::SettingsState;
use protocol::{Credential, Donation, Incoming, Protocol};

pub const STATUS_EVENT: &str = "trakteer://status";
pub const DONATION_EVENT: &str = "trakteer://donation";
pub const RECENT_EVENT: &str = "trakteer://recent";
pub const KEY_TARGET: &str = "LiveStreamOverlay/trakteer-key";

const RECENT_CAPACITY: usize = 20;
/// ID alert V2 yang sudah diproses (mencegah alert ganda dari beberapa event yang sama).
const SEEN_CAPACITY: usize = 100;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Batas menunggu `connection_established` (V1) / `authenticated` (V2).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const V2_PING_EVERY: Duration = Duration::from_secs(20);
const BACKOFF_START: Duration = Duration::from_millis(1500);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrakteerState {
    /// Kunci belum diisi.
    Idle,
    Connecting,
    Connected,
    /// Koneksi putus / gagal; dicoba ulang otomatis.
    Retrying,
    /// Kunci tidak valid atau ditolak; menunggu kunci baru.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrakteerStatus {
    pub state: TrakteerState,
    pub message: Option<String>,
    pub protocol: Option<Protocol>,
    pub attempt: u32,
}

impl TrakteerStatus {
    fn new(state: TrakteerState, protocol: Option<Protocol>, message: Option<String>, attempt: u32) -> Self {
        Self { state, message, protocol, attempt }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DonationKind {
    Live,
    /// Dari tombol "Test alert" di dashboard Trakteer.
    Test,
    /// Contoh lokal dari tombol "Tes donasi" aplikasi ini.
    Sample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonationEvent {
    pub id: String,
    pub kind: DonationKind,
    pub donation: Donation,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrakteerSnapshot {
    pub status: TrakteerStatus,
    pub recent: Vec<DonationEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub has_key: bool,
    pub protocol: Option<Protocol>,
}

pub enum Control {
    /// Kunci berubah atau user menekan "Hubungkan ulang".
    Reconnect,
}

struct Shared {
    status: TrakteerStatus,
    recent: VecDeque<DonationEvent>,
    seen: VecDeque<String>,
}

pub struct TrakteerService {
    shared: Mutex<Shared>,
    control: mpsc::UnboundedSender<Control>,
    seq: AtomicU64,
}

impl TrakteerService {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> TrakteerStatus {
        self.lock().status.clone()
    }

    pub fn snapshot(&self) -> TrakteerSnapshot {
        let shared = self.lock();
        TrakteerSnapshot { status: shared.status.clone(), recent: shared.recent.iter().cloned().collect() }
    }

    pub fn reconnect(&self) {
        let _ = self.control.send(Control::Reconnect);
    }

    fn next_id(&self) -> String {
        format!("d{}", self.seq.fetch_add(1, Ordering::Relaxed))
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Server tiruan untuk pengembangan: `LSO_TRAKTEER_URL=ws://127.0.0.1:4470`. Hanya build debug.
fn base_override() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("LSO_TRAKTEER_URL").ok().filter(|v| !v.trim().is_empty())
    } else {
        None
    }
}

fn stored_key() -> Result<Option<String>, String> {
    secrets::read(KEY_TARGET)
}

pub fn key_status() -> KeyStatus {
    match stored_key() {
        Ok(Some(key)) => KeyStatus { has_key: true, protocol: protocol::parse_key(&key).ok().map(|c| c.protocol()) },
        _ => KeyStatus { has_key: false, protocol: None },
    }
}

/// Menyimpan kunci baru (setelah divalidasi) lalu menyambung ulang.
pub fn set_key(app: &AppHandle, key: &str) -> Result<KeyStatus, String> {
    let credential = protocol::parse_key(key)?;
    secrets::write(KEY_TARGET, key.trim())?;
    log::info!("kunci Trakteer disimpan ({})", credential.protocol().label());
    app.state::<TrakteerService>().reconnect();
    Ok(key_status())
}

pub fn clear_key(app: &AppHandle) -> Result<KeyStatus, String> {
    secrets::delete(KEY_TARGET)?;
    log::info!("kunci Trakteer dihapus");
    app.state::<TrakteerService>().reconnect();
    Ok(key_status())
}

pub fn init(app: &AppHandle) {
    let (control, rx) = mpsc::unbounded_channel();
    let initial = if key_status().has_key { TrakteerState::Connecting } else { TrakteerState::Idle };
    app.manage(TrakteerService {
        shared: Mutex::new(Shared {
            status: TrakteerStatus::new(initial, None, None, 0),
            recent: VecDeque::new(),
            seen: VecDeque::new(),
        }),
        control,
        seq: AtomicU64::new(1),
    });
    tauri::async_runtime::spawn(run(app.clone(), rx));
}

fn set_status(app: &AppHandle, status: TrakteerStatus) {
    let svc = app.state::<TrakteerService>();
    {
        let mut shared = svc.lock();
        if shared.status == status {
            return;
        }
        shared.status = status.clone();
    }
    if let Err(e) = app.emit(STATUS_EVENT, &status) {
        log::warn!("gagal emit status Trakteer: {e}");
    }
}

fn emit_donation(app: &AppHandle, event: &DonationEvent) {
    if let Err(e) = app.emit(DONATION_EVENT, event) {
        log::warn!("gagal emit donasi: {e}");
    }
}

/// Apakah donasi ditampilkan di overlay menurut pengaturan.
pub fn passes_filter(kind: DonationKind, amount_rp: u64, show_test: bool, min_amount_rp: u64) -> bool {
    match kind {
        DonationKind::Sample => true,
        DonationKind::Test if !show_test => false,
        _ => amount_rp >= min_amount_rp,
    }
}

fn on_donation(app: &AppHandle, donation: Donation, test: bool) {
    let svc = app.state::<TrakteerService>();
    let kind = if test { DonationKind::Test } else { DonationKind::Live };
    let event = DonationEvent { id: svc.next_id(), kind, donation, received_at_ms: now_ms() };
    let recent = {
        let mut shared = svc.lock();
        if let Some(id) = &event.donation.source_id {
            if shared.seen.contains(id) {
                return;
            }
            shared.seen.push_back(id.clone());
            if shared.seen.len() > SEEN_CAPACITY {
                shared.seen.pop_front();
            }
        }
        shared.recent.push_front(event.clone());
        shared.recent.truncate(RECENT_CAPACITY);
        shared.recent.iter().cloned().collect::<Vec<_>>()
    };
    if kind == DonationKind::Live {
        crate::session::on_donation(app, &event.donation.supporter_name, event.donation.amount_rp);
    }
    log::info!(
        "donasi Trakteer diterima {}{}",
        event.donation.price_text,
        if test { " (test)" } else { "" }
    );
    if let Err(e) = app.emit(RECENT_EVENT, &recent) {
        log::warn!("gagal emit riwayat donasi: {e}");
    }
    let cfg = app.state::<SettingsState>().get().trakteer;
    if passes_filter(kind, event.donation.amount_rp, cfg.show_test, cfg.min_amount_rp) {
        emit_donation(app, &event);
    }
}

/// Donasi contoh untuk mengatur posisi widget (tidak masuk riwayat).
pub fn emit_sample(app: &AppHandle) {
    let svc = app.state::<TrakteerService>();
    let donation = Donation {
        source_id: None,
        supporter_name: "Penonton Contoh".into(),
        unit: "Cendol".into(),
        quantity: 2,
        amount_rp: 10_000,
        price_text: protocol::format_price(10_000),
        message: "Semangat streamingnya! Ini contoh alert dari LiveStream Overlay.".into(),
        avatar_url: "https://edge-cdn.trakteer.id/images/mix/default-avatar.png".into(),
        unit_icon_url: None,
    };
    let event = DonationEvent { id: svc.next_id(), kind: DonationKind::Sample, donation, received_at_ms: now_ms() };
    emit_donation(app, &event);
}

// ---------- Sesi koneksi (tanpa AppHandle, bisa diuji) ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Connected,
    Donation { donation: Donation, test: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEnd {
    Reconnect,
    Shutdown,
    /// Kunci ditolak server; jangan dicoba ulang otomatis.
    Rejected(String),
    Lost { message: String, was_connected: bool },
}

fn control_end(c: Option<Control>) -> SessionEnd {
    match c {
        Some(Control::Reconnect) => SessionEnd::Reconnect,
        None => SessionEnd::Shutdown,
    }
}

/// Satu sesi WebSocket sampai putus, ditolak, atau dihentikan lewat `rx`.
pub async fn run_session(
    url: &str,
    protocol: Protocol,
    v1_channel: Option<&str>,
    mut on_event: impl FnMut(SessionEvent),
    rx: &mut mpsc::UnboundedReceiver<Control>,
) -> SessionEnd {
    let lost = |message: &str, was_connected: bool| SessionEnd::Lost { message: message.to_string(), was_connected };
    let builder = match ClientBuilder::new().uri(url) {
        Ok(b) => b,
        Err(_) => return SessionEnd::Rejected("Alamat server Trakteer tidak valid.".into()),
    };
    let (mut ws, _) = tokio::select! {
        result = tokio::time::timeout(CONNECT_TIMEOUT, builder.connect()) => match result {
            Ok(Ok(pair)) => pair,
            Ok(Err(_)) => return lost("Tidak bisa terhubung ke Trakteer. Periksa koneksi internet.", false),
            Err(_) => return lost("Trakteer tidak menjawab (waktu habis).", false),
        },
        c = rx.recv() => return control_end(c),
    };

    let mut connected = false;
    let handshake_deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut ping_every = match protocol {
        Protocol::V1 => Duration::from_secs(protocol::DEFAULT_ACTIVITY_TIMEOUT_S * 9 / 10),
        Protocol::V2 => V2_PING_EVERY,
    };
    let mut next_ping = Instant::now() + ping_every;

    loop {
        tokio::select! {
            msg = ws.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(_)) | None => return lost("Koneksi ke Trakteer terputus.", connected),
                };
                if msg.is_close() {
                    return lost("Server Trakteer menutup koneksi.", connected);
                }
                let Some(text) = msg.as_text() else { continue };
                match protocol::parse(protocol, text) {
                    Incoming::Established { activity_timeout_s } => {
                        // Heartbeat ±90% dari batas diam server.
                        ping_every = Duration::from_secs((activity_timeout_s * 9 / 10).max(5));
                        next_ping = Instant::now() + ping_every;
                        if let Some(channel) = v1_channel {
                            for sub in protocol::subscribe_messages(channel) {
                                if ws.send(Message::text(sub)).await.is_err() {
                                    return lost("Gagal subscribe ke channel Trakteer.", false);
                                }
                            }
                        }
                        connected = true;
                        on_event(SessionEvent::Connected);
                    }
                    Incoming::Authenticated => {
                        connected = true;
                        on_event(SessionEvent::Connected);
                    }
                    Incoming::Ping => {
                        if ws.send(Message::text(protocol::pong_message())).await.is_err() {
                            return lost("Koneksi ke Trakteer terputus.", connected);
                        }
                    }
                    Incoming::Error(message) if !connected => return SessionEnd::Rejected(message),
                    Incoming::Error(message) => log::warn!("Trakteer mengirim error: {message}"),
                    Incoming::Donation { donation, test } => on_event(SessionEvent::Donation { donation, test }),
                    Incoming::Ignore => {}
                }
            }
            _ = tokio::time::sleep_until(next_ping) => {
                let ping = match protocol {
                    Protocol::V1 => Message::text(protocol::ping_message()),
                    Protocol::V2 => Message::ping(Vec::<u8>::new()),
                };
                if ws.send(ping).await.is_err() {
                    return lost("Koneksi ke Trakteer terputus.", connected);
                }
                next_ping = Instant::now() + ping_every;
            }
            _ = tokio::time::sleep_until(handshake_deadline), if !connected => {
                return lost("Trakteer tidak membalas. Periksa Channel ID / Stream Key.", false);
            }
            c = rx.recv() => {
                let _ = ws.send(Message::close(None, "")).await;
                return control_end(c);
            }
        }
    }
}

fn next_backoff(current: Duration) -> Duration {
    (current * 16 / 10).min(BACKOFF_MAX)
}

/// `Some(true)` = dibangunkan, `Some(false)` = waktu habis, `None` = aplikasi ditutup.
async fn wait(rx: &mut mpsc::UnboundedReceiver<Control>, delay: Option<Duration>) -> Option<bool> {
    match delay {
        Some(d) => tokio::select! {
            _ = tokio::time::sleep(d) => Some(false),
            c = rx.recv() => c.map(|_| true),
        },
        None => rx.recv().await.map(|_| true),
    }
}

async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<Control>) {
    let mut attempt = 0u32;
    let mut delay = BACKOFF_START;
    let mut last_logged = String::new();

    loop {
        let credential = match stored_key() {
            Ok(Some(key)) => protocol::parse_key(&key),
            Ok(None) => {
                set_status(&app, TrakteerStatus::new(TrakteerState::Idle, None, None, 0));
                match wait(&mut rx, None).await {
                    Some(_) => continue,
                    None => return,
                }
            }
            Err(e) => Err(format!("Gagal membaca kunci Trakteer: {e}")),
        };
        let credential = match credential {
            Ok(c) => c,
            Err(e) => {
                set_status(&app, TrakteerStatus::new(TrakteerState::Failed, None, Some(e), 0));
                match wait(&mut rx, None).await {
                    Some(_) => continue,
                    None => return,
                }
            }
        };

        let proto = credential.protocol();
        let previous = app.state::<TrakteerService>().status().message;
        let message = if attempt > 0 { previous } else { None };
        set_status(&app, TrakteerStatus::new(TrakteerState::Connecting, Some(proto), message, attempt));

        let url = credential.url_with_base(base_override().as_deref());
        let channel = match &credential {
            Credential::V1Channel(c) => Some(c.clone()),
            Credential::V2Token(_) => None,
        };
        let session_app = app.clone();
        let end = run_session(&url, proto, channel.as_deref(), |event| match event {
            SessionEvent::Connected => {
                log::info!("Trakteer tersambung ({})", proto.label());
                set_status(&session_app, TrakteerStatus::new(TrakteerState::Connected, Some(proto), None, 0));
            }
            SessionEvent::Donation { donation, test } => on_donation(&session_app, donation, test),
        }, &mut rx)
        .await;

        let pause = match end {
            SessionEnd::Reconnect => {
                attempt = 0;
                delay = BACKOFF_START;
                last_logged.clear();
                continue;
            }
            SessionEnd::Shutdown => return,
            SessionEnd::Rejected(message) => {
                log::info!("Trakteer menolak kunci: {message}");
                set_status(&app, TrakteerStatus::new(TrakteerState::Failed, Some(proto), Some(message), 0));
                None
            }
            SessionEnd::Lost { message, was_connected } => {
                if was_connected {
                    attempt = 0;
                    delay = BACKOFF_START;
                }
                attempt += 1;
                let shown = if attempt >= 3 {
                    format!("{message} Jika terus gagal, periksa Channel ID / Stream Key.")
                } else {
                    message
                };
                if shown != last_logged {
                    log::info!("Trakteer: {shown}");
                    last_logged.clone_from(&shown);
                }
                set_status(&app, TrakteerStatus::new(TrakteerState::Retrying, Some(proto), Some(shown), attempt));
                let pause = delay;
                delay = next_backoff(delay);
                Some(pause)
            }
        };

        match wait(&mut rx, pause).await {
            Some(true) => {
                attempt = 0;
                delay = BACKOFF_START;
                last_logged.clear();
            }
            Some(false) => {}
            None => return,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/mock_trakteer.rs"]
mod mock_trakteer;

#[cfg(test)]
mod tests {
    use super::mock_trakteer::{MockDonation, MockTrakteer};
    use super::*;

    #[test]
    fn backoff_sequence() {
        let mut d = BACKOFF_START;
        let mut seen = vec![d.as_millis()];
        for _ in 0..6 {
            d = next_backoff(d);
            seen.push(d.as_millis());
        }
        assert_eq!(seen, vec![1500, 2400, 3840, 6144, 9830, 10000, 10000]);
    }

    #[test]
    fn filters() {
        assert!(passes_filter(DonationKind::Live, 5_000, true, 0));
        assert!(!passes_filter(DonationKind::Live, 5_000, true, 10_000));
        assert!(passes_filter(DonationKind::Test, 50_000, true, 10_000));
        assert!(!passes_filter(DonationKind::Test, 50_000, false, 0));
        assert!(passes_filter(DonationKind::Sample, 0, false, 1_000_000), "contoh selalu tampil");
    }

    /// Menjalankan sesi sampai `until` terpenuhi, lalu menghentikannya.
    async fn collect(
        mock: &MockTrakteer,
        credential: Credential,
        action: impl FnOnce(&MockTrakteer),
        want: usize,
    ) -> (Vec<SessionEvent>, SessionEnd) {
        let url = credential.url_with_base(Some(&mock.base_url()));
        let channel = match &credential {
            Credential::V1Channel(c) => Some(c.clone()),
            Credential::V2Token(_) => None,
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let events = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&events);
        let session = run_session(&url, credential.protocol(), channel.as_deref(), move |e| sink.lock().unwrap().push(e), &mut rx);
        tokio::pin!(session);

        // Tunggu tersambung, picu aksi, lalu tunggu event yang diharapkan.
        let mut acted = Some(action);
        let deadline = Instant::now() + Duration::from_secs(5);
        let end = loop {
            tokio::select! {
                end = &mut session => break end,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    let n = events.lock().unwrap().len();
                    if n >= 1 {
                        if let Some(act) = acted.take() {
                            act(mock);
                        }
                    }
                    if n >= want || Instant::now() > deadline {
                        let _ = tx.send(Control::Reconnect);
                    }
                }
            }
        };
        let events = events.lock().unwrap().clone();
        (events, end)
    }

    fn donations(events: &[SessionEvent]) -> Vec<(String, u64, bool)> {
        events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Donation { donation, test } => Some((donation.supporter_name.clone(), donation.amount_rp, *test)),
                SessionEvent::Connected => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn v1_pusher_flow() {
        let mock = MockTrakteer::start(0).await.unwrap();
        let (events, end) = collect(
            &mock,
            Credential::V1Channel("kanal-uji".into()),
            |m| {
                m.send_ping();
                m.send_donation(MockDonation::new("Penonton Satu", 5_000, false));
                m.send_donation(MockDonation::new("Penonton Uji", 2_000, true));
            },
            3,
        )
        .await;
        assert_eq!(events[0], SessionEvent::Connected);
        assert_eq!(
            donations(&events),
            vec![("Penonton Satu".into(), 5_000, false), ("Penonton Uji".into(), 2_000, true)]
        );
        assert_eq!(end, SessionEnd::Reconnect);
        // Pong diproses server secara async; beri waktu sebentar.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            mock.subscriptions(),
            vec!["creator-stream.kanal-uji".to_string(), "creator-stream-test.kanal-uji".to_string()]
        );
        assert_eq!(mock.pongs_received(), 1, "pusher:ping dibalas pong");
    }

    #[tokio::test]
    async fn v2_flow_and_rejection() {
        let mock = MockTrakteer::start(0).await.unwrap();
        let (events, _) = collect(
            &mock,
            Credential::V2Token("trv2-benar".into()),
            |m| m.send_donation(MockDonation::new("Uji V2", 20_000, false)),
            2,
        )
        .await;
        assert_eq!(events[0], SessionEvent::Connected);
        assert_eq!(donations(&events), vec![("Uji V2".into(), 20_000, false)]);

        let (events, end) = collect(&mock, Credential::V2Token("trv2-salah".into()), |_| {}, 1).await;
        assert!(events.is_empty());
        assert_eq!(end, SessionEnd::Rejected("token tidak valid".into()));
    }

    #[tokio::test]
    async fn server_shutdown_is_lost_after_connect() {
        let mock = MockTrakteer::start(0).await.unwrap();
        let url = Credential::V2Token("trv2-benar".into()).url_with_base(Some(&mock.base_url()));
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&connected);
        let session = run_session(&url, Protocol::V2, None, move |e| {
            if e == SessionEvent::Connected {
                flag.store(true, Ordering::SeqCst);
            }
        }, &mut rx);
        tokio::pin!(session);
        let end = loop {
            tokio::select! {
                end = &mut session => break end,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if connected.load(Ordering::SeqCst) {
                        mock.disconnect_all();
                    }
                }
            }
        };
        assert!(matches!(end, SessionEnd::Lost { was_connected: true, .. }), "{end:?}");
    }

    /// Uji jaringan sungguhan (manual): `cargo test real_v1_handshake -- --ignored`.
    /// Memakai nama channel rekaan, jadi tidak membaca donasi siapa pun.
    #[tokio::test]
    #[ignore]
    async fn real_v1_handshake() {
        let credential = Credential::V1Channel("lso-uji-koneksi-00000".into());
        let url = credential.url_with_base(None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let end = run_session(&url, Protocol::V1, Some("lso-uji-koneksi-00000"), move |e| {
            if e == SessionEvent::Connected {
                let _ = tx.send(Control::Reconnect);
            }
        }, &mut rx)
        .await;
        assert_eq!(end, SessionEnd::Reconnect, "handshake TLS + Pusher ke socket.trakteer.id");
    }

    #[tokio::test]
    async fn unreachable_server_is_lost() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = Credential::V2Token("trv2-x".into()).url_with_base(Some(&format!("ws://127.0.0.1:{port}")));
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let end = run_session(&url, Protocol::V2, None, |_| {}, &mut rx).await;
        assert!(matches!(end, SessionEnd::Lost { was_connected: false, .. }), "{end:?}");
    }
}
