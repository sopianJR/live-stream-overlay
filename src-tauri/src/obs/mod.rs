//! Koneksi ke OBS lewat obs-websocket v5 (crate `obws`).
//!
//! Satu task di latar menjaga koneksi:
//! - menghubungkan, lalu mencoba ulang dengan backoff (1, 2, 4, 8, kemudian tiap 10 detik);
//! - berhenti mencoba saat password salah, sampai pengaturan atau password diubah;
//! - polling statistik tiap detik dan mendengarkan event (stream, rekaman, scene, mic);
//! - menyiarkan [`ObsStatus`] ke semua window lewat event `obs://status`.
//!
//! Command kontrol (start/stop stream, rekaman, mic, scene) memakai klien yang sama.

mod stats;
mod status;

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use obws::client::{ConnectConfig, HandshakeError};
use obws::error::Error as ObsError;
use obws::requests::inputs::InputId;
use obws::requests::EventSubscription;
use obws::responses::StatusCode;
use obws::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::secrets;
use crate::settings::{ObsSettings, SettingsState};
use stats::StatsTracker;
pub use status::ObsStatus;
use status::{Connection, EventEffect, MicInfo, PollSample};

pub const STATUS_EVENT: &str = "obs://status";
pub const NOTICE_EVENT: &str = "obs://notice";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const POLL_TIMEOUT: Duration = Duration::from_secs(5);
/// Request obws yang dikirim setelah koneksi putus bisa menunggu selamanya; semua
/// panggilan diberi batas waktu.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(10);
/// Kode close obs-websocket untuk autentikasi gagal (password salah / tidak dikirim).
const CLOSE_AUTH_FAILED: u16 = 4009;
const CLOSE_UNSUPPORTED_RPC: u16 = 4010;

enum Control {
    /// Pengaturan atau password berubah, atau user menekan "Hubungkan ulang".
    Reconnect,
    /// Input mic yang dipilih berubah; cukup dibaca ulang tanpa memutus koneksi.
    RefreshMic,
}

struct Shared {
    status: ObsStatus,
    client: Option<Arc<Client>>,
}

pub struct ObsService {
    shared: Mutex<Shared>,
    control: mpsc::UnboundedSender<Control>,
    epoch: Instant,
}

impl ObsService {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> ObsStatus {
        self.lock().status.clone()
    }

    pub fn reconnect(&self) {
        let _ = self.control.send(Control::Reconnect);
    }

    pub fn refresh_mic(&self) {
        let _ = self.control.send(Control::RefreshMic);
    }

    fn client(&self) -> Result<Arc<Client>, String> {
        self.lock().client.clone().ok_or_else(|| "OBS belum terhubung.".to_string())
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Notifikasi singkat untuk overlay, mis. hasil hotkey yang ditekan saat bermain.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

fn endpoint(s: &ObsSettings) -> String {
    format!("{}:{}", s.host, s.port)
}

/// Mendaftarkan servis dan menjalankan task koneksi. Butuh `SettingsState` sudah terdaftar.
pub fn init(app: &AppHandle) {
    let settings = app.state::<SettingsState>().get().obs;
    let (control, rx) = mpsc::unbounded_channel();
    app.manage(ObsService {
        shared: Mutex::new(Shared { status: ObsStatus::new(endpoint(&settings)), client: None }),
        control,
        epoch: Instant::now(),
    });
    tauri::async_runtime::spawn(run(app.clone(), rx));
}

/// Mengubah status; event hanya dikirim jika `f` melaporkan ada perubahan.
fn update(app: &AppHandle, f: impl FnOnce(&mut ObsStatus) -> bool) {
    let svc = app.state::<ObsService>();
    let snapshot = {
        let mut shared = svc.lock();
        if !f(&mut shared.status) {
            return;
        }
        shared.status.clone()
    };
    if let Err(e) = app.emit(STATUS_EVENT, &snapshot) {
        log::warn!("gagal emit status OBS: {e}");
    }
}

fn notify(app: &AppHandle, text: impl Into<String>, error: bool) {
    let notice = Notice { text: text.into(), error };
    if let Err(e) = app.emit(NOTICE_EVENT, &notice) {
        log::warn!("gagal emit notifikasi OBS: {e}");
    }
}

// ---------- Koneksi ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    Retry,
    Auth,
    Fatal,
}

#[derive(Debug)]
struct Failure {
    kind: FailureKind,
    message: String,
}

fn classify(e: &ObsError, had_password: bool, target: &str) -> Failure {
    let (kind, message) = match e {
        ObsError::Connect(_) | ObsError::Timeout => (
            FailureKind::Retry,
            format!(
                "OBS tidak ditemukan di {target}. Pastikan OBS sudah dibuka dan WebSocket server \
                 aktif (Tools → WebSocket Server Settings)."
            ),
        ),
        ObsError::Handshake(HandshakeError::ConnectionClosed(Some(close))) => {
            match u16::from(close.code) {
                CLOSE_AUTH_FAILED if had_password => (
                    FailureKind::Auth,
                    "Password OBS salah. Cek di OBS → Tools → WebSocket Server Settings → \
                     Show Connect Info."
                        .to_string(),
                ),
                CLOSE_AUTH_FAILED => (
                    FailureKind::Auth,
                    "OBS meminta password. Isi password WebSocket di dashboard → Koneksi.".to_string(),
                ),
                CLOSE_UNSUPPORTED_RPC => (
                    FailureKind::Fatal,
                    "Versi protokol obs-websocket tidak didukung. Perbarui OBS ke versi terbaru."
                        .to_string(),
                ),
                code => (
                    FailureKind::Retry,
                    format!("OBS menutup koneksi (kode {code}): {}", close.reason),
                ),
            }
        }
        ObsError::ObsStudioVersion(version, _) => (
            FailureKind::Fatal,
            format!("OBS {version} terlalu lama. Perbarui ke OBS 30.2 atau yang lebih baru."),
        ),
        ObsError::ObsWebsocketVersion(version, _) => (
            FailureKind::Fatal,
            format!("obs-websocket {version} tidak didukung. Perbarui OBS ke versi terbaru."),
        ),
        ObsError::InvalidUri(_) => {
            (FailureKind::Fatal, format!("Alamat OBS \"{target}\" tidak valid."))
        }
        other => (FailureKind::Retry, format!("Gagal terhubung ke OBS di {target}: {other}")),
    };
    Failure { kind, message }
}

fn event_subscriptions() -> EventSubscription {
    EventSubscription::GENERAL
        | EventSubscription::SCENES
        | EventSubscription::INPUTS
        | EventSubscription::OUTPUTS
}

async fn connect(
    settings: &ObsSettings,
    password: Option<&str>,
    target: &str,
) -> Result<Client, Failure> {
    let config = ConnectConfig {
        host: settings.host.as_str(),
        port: settings.port,
        dangerous: None,
        password,
        event_subscriptions: Some(event_subscriptions()),
        broadcast_capacity: 256,
        connect_timeout: CONNECT_TIMEOUT,
    };
    // Batas total, karena obws tidak membatasi waktu menunggu balasan identifikasi.
    match tokio::time::timeout(CONNECT_TIMEOUT * 3, Client::connect_with_config(config)).await {
        Ok(Ok(client)) => Ok(client),
        Ok(Err(e)) => Err(classify(&e, password.is_some(), target)),
        Err(_) => Err(Failure {
            kind: FailureKind::Retry,
            message: format!("OBS di {target} tidak menjawab."),
        }),
    }
}

fn backoff(attempt: u32) -> Duration {
    let secs = 1u64 << attempt.saturating_sub(1).min(4);
    Duration::from_secs(secs).min(MAX_BACKOFF)
}

enum Wake {
    Timer,
    Control,
    Closed,
}

async fn wait(rx: &mut mpsc::UnboundedReceiver<Control>, delay: Option<Duration>) -> Wake {
    let control = |c: Option<Control>| if c.is_some() { Wake::Control } else { Wake::Closed };
    match delay {
        Some(d) => tokio::select! {
            _ = tokio::time::sleep(d) => Wake::Timer,
            c = rx.recv() => control(c),
        },
        None => control(rx.recv().await),
    }
}

async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<Control>) {
    let mut attempt = 0u32;
    let mut last_logged: Option<String> = None;

    loop {
        let settings = app.state::<SettingsState>().get().obs;
        let target = endpoint(&settings);
        let password = secrets::obs_password().unwrap_or_else(|e| {
            log::warn!("gagal membaca password OBS: {e}");
            None
        });

        update(&app, |s| {
            // Saat mencoba ulang, alasan kegagalan sebelumnya tetap terlihat.
            let message = if attempt > 0 { s.message.take() } else { None };
            s.set_disconnected(Connection::Connecting, message, attempt, &target);
            true
        });

        let delay = match connect(&settings, password.as_deref(), &target).await {
            Ok(client) => {
                attempt = 0;
                last_logged = None;
                log::info!("terhubung ke OBS di {target}");
                match session(&app, Arc::new(client), &settings, &mut rx).await {
                    SessionEnd::Reconnect => continue,
                    SessionEnd::Shutdown => return,
                    SessionEnd::Lost(reason) => {
                        log::info!("koneksi OBS berakhir: {reason}");
                        update(&app, |s| {
                            s.set_disconnected(Connection::Retrying, Some(reason), 0, &target);
                            true
                        });
                        Some(Duration::from_secs(1))
                    }
                }
            }
            Err(failure) => {
                if last_logged.as_deref() != Some(failure.message.as_str()) {
                    log::info!("OBS: {}", failure.message);
                    last_logged = Some(failure.message.clone());
                }
                let (connection, delay) = match failure.kind {
                    FailureKind::Retry => {
                        attempt += 1;
                        (Connection::Retrying, Some(backoff(attempt)))
                    }
                    FailureKind::Auth => (Connection::AuthFailed, None),
                    FailureKind::Fatal => (Connection::Failed, None),
                };
                update(&app, |s| {
                    s.set_disconnected(connection, Some(failure.message), attempt, &target);
                    true
                });
                delay
            }
        };

        match wait(&mut rx, delay).await {
            Wake::Timer => {}
            Wake::Control => {
                attempt = 0;
                last_logged = None;
            }
            Wake::Closed => return,
        }
    }
}

// ---------- Sesi ----------

enum SessionEnd {
    Reconnect,
    Shutdown,
    Lost(String),
}

fn is_disconnect(e: &ObsError) -> bool {
    matches!(e, ObsError::Send(_) | ObsError::ReceiveMessage(_) | ObsError::Disconnected)
}

async fn poll(client: &Client) -> Result<PollSample, ObsError> {
    let (streaming, recording, general) = (client.streaming(), client.recording(), client.general());
    let (stream, record, stats) =
        tokio::join!(streaming.status(), recording.status(), general.stats());
    Ok(PollSample { stream: stream?, record: record?, stats: stats? })
}

/// Input yang dipantau sebagai mic: pilihan user, atau "Mic/Aux" global OBS.
async fn resolve_mic(client: &Client, configured: Option<&str>) -> (Option<MicInfo>, Option<String>) {
    let name = match configured {
        Some(name) => name.to_string(),
        None => match client.inputs().specials().await {
            Ok(specials) => match specials.mic1 {
                Some(name) => name,
                None => {
                    return (
                        None,
                        Some(
                            "OBS tidak punya input Mic/Aux global. Pilih input mic di dashboard → \
                             Koneksi."
                                .into(),
                        ),
                    )
                }
            },
            Err(e) => return (None, Some(format!("Gagal membaca input mic: {}", command_error(&e)))),
        },
    };
    match client.inputs().muted(InputId::Name(&name)).await {
        Ok(muted) => (Some(MicInfo { name, muted }), None),
        Err(ObsError::Api { code: StatusCode::ResourceNotFound, .. }) => {
            (None, Some(format!("Input mic \"{name}\" tidak ditemukan di OBS.")))
        }
        Err(e) => (None, Some(format!("Input \"{name}\" tidak bisa dipakai: {}", command_error(&e)))),
    }
}

async fn session(
    app: &AppHandle,
    client: Arc<Client>,
    settings: &ObsSettings,
    rx: &mut mpsc::UnboundedReceiver<Control>,
) -> SessionEnd {
    let svc = app.state::<ObsService>();
    let mut events = match client.events() {
        Ok(events) => events,
        Err(e) => return SessionEnd::Lost(format!("Gagal mendengarkan event OBS: {e}")),
    };

    let initial = tokio::time::timeout(POLL_TIMEOUT, async {
        let version = client.general().version().await.ok().map(|v| v.obs_studio_version.to_string());
        let scenes = client.scenes().list().await.ok();
        let mic = resolve_mic(&client, settings.mic_input.as_deref()).await;
        (version, scenes, mic)
    })
    .await;
    let Ok((version, scenes, (mic, mic_note))) = initial else {
        return SessionEnd::Lost("OBS tidak merespons.".into());
    };

    svc.lock().client = Some(Arc::clone(&client));
    update(app, |s| {
        s.connection = Connection::Connected;
        s.message = None;
        s.attempt = 0;
        s.obs_version = version;
        if let Some(list) = &scenes {
            s.scene = list.current_program_scene.as_ref().map(|c| c.name.clone());
            s.scenes = status::scene_names(&list.scenes);
        }
        s.mic = mic;
        s.mic_note = mic_note;
        true
    });

    let mut tracker = StatsTracker::default();
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let end = loop {
        tokio::select! {
            _ = tick.tick() => match tokio::time::timeout(POLL_TIMEOUT, poll(&client)).await {
                Err(_) => break SessionEnd::Lost("OBS tidak merespons.".into()),
                Ok(Err(e)) if is_disconnect(&e) => break SessionEnd::Lost("Koneksi ke OBS terputus.".into()),
                // Mis. `NotReady` saat OBS mengganti scene collection; coba lagi detik berikutnya.
                Ok(Err(e)) => log::debug!("polling OBS gagal: {e}"),
                Ok(Ok(sample)) => {
                    let t = svc.now_ms();
                    update(app, |s| {
                        status::apply_poll(s, &mut tracker, t, &sample);
                        true
                    });
                }
            },
            event = events.next() => {
                let Some(event) = event else {
                    break SessionEnd::Lost("Koneksi ke OBS terputus.".into());
                };
                let t = svc.now_ms();
                let mut effect = EventEffect::Unchanged;
                update(app, |s| {
                    effect = status::apply_event(s, &mut tracker, t, &event);
                    effect == EventEffect::Changed
                });
                if let EventEffect::Closing(reason) = effect {
                    break SessionEnd::Lost(reason.into());
                }
            },
            control = rx.recv() => match control {
                Some(Control::Reconnect) => break SessionEnd::Reconnect,
                Some(Control::RefreshMic) => {
                    let configured = app.state::<SettingsState>().get().obs.mic_input;
                    let Ok((mic, mic_note)) =
                        tokio::time::timeout(POLL_TIMEOUT, resolve_mic(&client, configured.as_deref())).await
                    else {
                        break SessionEnd::Lost("OBS tidak merespons.".into());
                    };
                    update(app, |s| {
                        s.mic = mic;
                        s.mic_note = mic_note;
                        true
                    });
                }
                None => break SessionEnd::Shutdown,
            },
        }
    };

    svc.lock().client = None;
    end
}

// ---------- Kontrol ----------

fn command_error(e: &ObsError) -> String {
    match e {
        ObsError::Api { code, message } => {
            let detail = message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default();
            match code {
                StatusCode::OutputRunning => "Sudah berjalan.".to_string(),
                StatusCode::OutputNotRunning => "Sedang tidak berjalan.".to_string(),
                StatusCode::ResourceNotFound => format!("Tidak ditemukan di OBS{detail}."),
                StatusCode::NotReady => "OBS sedang sibuk, coba lagi sebentar.".to_string(),
                other => format!("OBS menolak perintah: {other:?}{detail}."),
            }
        }
        e if is_disconnect(e) => "Koneksi ke OBS terputus.".to_string(),
        other => format!("Perintah ke OBS gagal: {other}"),
    }
}

fn client(app: &AppHandle) -> Result<Arc<Client>, String> {
    app.state::<ObsService>().client()
}

async fn call<T>(request: impl Future<Output = Result<T, ObsError>>) -> Result<T, String> {
    match tokio::time::timeout(COMMAND_TIMEOUT, request).await {
        Ok(result) => result.map_err(|e| command_error(&e)),
        Err(_) => Err("OBS tidak merespons.".into()),
    }
}

pub async fn start_stream(app: &AppHandle) -> Result<(), String> {
    let client = client(app)?;
    call(client.streaming().start()).await?;
    log::info!("stream dimulai dari aplikasi");
    Ok(())
}

pub async fn stop_stream(app: &AppHandle) -> Result<(), String> {
    let client = client(app)?;
    call(client.streaming().stop()).await?;
    log::info!("stream dihentikan dari aplikasi");
    Ok(())
}

/// Mengembalikan `true` jika rekaman kini berjalan.
pub async fn toggle_record(app: &AppHandle) -> Result<bool, String> {
    let client = client(app)?;
    call(client.recording().toggle()).await
}

/// Mengembalikan `true` jika mic kini di-mute.
pub async fn toggle_mic(app: &AppHandle) -> Result<bool, String> {
    let client = client(app)?;
    let mic_name = {
        let svc = app.state::<ObsService>();
        let shared = svc.lock();
        match (&shared.status.mic, &shared.status.mic_note) {
            (Some(mic), _) => mic.name.clone(),
            (None, Some(note)) => return Err(note.clone()),
            (None, None) => return Err("Input mic belum tersedia.".into()),
        }
    };
    let muted = call(client.inputs().toggle_mute(InputId::Name(&mic_name))).await?;
    update(app, |s| match s.mic.as_mut() {
        Some(mic) if mic.name == mic_name && mic.muted != muted => {
            mic.muted = muted;
            true
        }
        _ => false,
    });
    Ok(muted)
}

pub async fn set_scene(app: &AppHandle, name: &str) -> Result<(), String> {
    let client = client(app)?;
    call(client.scenes().set_current_program_scene(name)).await
}

fn is_audio_kind(kind: &str) -> bool {
    kind.contains("input_capture") || kind.contains("output_capture") || kind.contains("audio_capture")
}

async fn audio_inputs(client: &Client) -> Result<Vec<String>, ObsError> {
    let mut names: Vec<String> = client
        .inputs()
        .list(None)
        .await?
        .into_iter()
        .filter(|i| is_audio_kind(&i.unversioned_kind))
        .map(|i| i.id.name)
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

/// Input audio di OBS yang bisa dipilih sebagai mic.
pub async fn list_audio_inputs(app: &AppHandle) -> Result<Vec<String>, String> {
    let client = client(app)?;
    call(audio_inputs(&client)).await
}

#[derive(Debug, Clone, Copy)]
pub enum QuickAction {
    ToggleMic,
    ToggleRecord,
}

/// Untuk hotkey: dijalankan di latar, hasilnya tampil sebagai notifikasi di overlay.
pub fn spawn_quick_action(app: &AppHandle, action: QuickAction) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match action {
            QuickAction::ToggleMic => toggle_mic(&app)
                .await
                .map(|muted| if muted { "Mic dimatikan" } else { "Mic aktif" }),
            QuickAction::ToggleRecord => toggle_record(&app)
                .await
                .map(|active| if active { "Rekaman dimulai" } else { "Rekaman dihentikan" }),
        };
        match result {
            Ok(text) => notify(&app, text, false),
            Err(e) => {
                log::info!("hotkey {action:?} gagal: {e}");
                notify(&app, e, true);
            }
        }
    });
}

#[cfg(test)]
#[path = "../../tests/support/mock_obs.rs"]
mod mock_obs;

#[cfg(test)]
mod tests {
    use super::mock_obs::{MockConfig, MockObs};
    use super::*;

    fn settings(port: u16) -> ObsSettings {
        ObsSettings { port, ..ObsSettings::default() }
    }

    async fn start_mock(config: MockConfig) -> MockObs {
        MockObs::start(0, config).await.expect("mock OBS jalan")
    }

    async fn connect_to(mock: &MockObs, password: Option<&str>) -> Result<Client, Failure> {
        connect(&settings(mock.port), password, "uji").await
    }

    #[test]
    fn backoff_grows_then_caps() {
        let secs: Vec<u64> = (1..=7).map(|a| backoff(a).as_secs()).collect();
        assert_eq!(secs, vec![1, 2, 4, 8, 10, 10, 10]);
    }

    #[tokio::test]
    async fn password_handling() {
        let mock = start_mock(MockConfig { password: Some("rahasia".into()), ..Default::default() }).await;

        assert!(connect_to(&mock, Some("rahasia")).await.is_ok());

        let wrong = connect_to(&mock, Some("salah")).await.err().expect("harus gagal");
        assert_eq!(wrong.kind, FailureKind::Auth);
        assert!(wrong.message.contains("salah"), "{}", wrong.message);

        let missing = connect_to(&mock, None).await.err().expect("harus gagal");
        assert_eq!(missing.kind, FailureKind::Auth);
        assert!(missing.message.contains("meminta password"), "{}", missing.message);
    }

    #[tokio::test]
    async fn password_is_ignored_when_obs_has_none() {
        let mock = start_mock(MockConfig::default()).await;
        assert!(connect_to(&mock, Some("apa saja")).await.is_ok());
    }

    #[tokio::test]
    async fn closed_port_is_retried() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let failure = connect(&settings(port), None, "127.0.0.1:x").await.err().expect("harus gagal");
        assert_eq!(failure.kind, FailureKind::Retry);
        assert!(failure.message.contains("tidak ditemukan"), "{}", failure.message);
    }

    #[tokio::test]
    async fn polls_live_stream() {
        let mock = start_mock(MockConfig { live: true, bitrate_kbps: 6000, ..Default::default() }).await;
        let client = connect_to(&mock, None).await.expect("terhubung");
        let mut status = ObsStatus::new("uji".into());
        let mut tracker = StatsTracker::default();
        let epoch = Instant::now();

        let first = poll(&client).await.expect("poll pertama");
        status::apply_poll(&mut status, &mut tracker, epoch.elapsed().as_millis() as u64, &first);
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        let second = poll(&client).await.expect("poll kedua");
        status::apply_poll(&mut status, &mut tracker, epoch.elapsed().as_millis() as u64, &second);

        assert!(status.stream.active);
        let kbps = status.stream.bitrate_kbps.expect("ada bitrate");
        assert!((kbps - 6000.0).abs() < 300.0, "bitrate {kbps}");
        assert_eq!(status.stream.dropped_pct, Some(0.0));
        assert_eq!(status.perf.as_ref().map(|p| p.fps), Some(60.0));
    }

    #[tokio::test]
    async fn mic_resolution_and_audio_inputs() {
        let mock = start_mock(MockConfig::default()).await;
        let client = connect_to(&mock, None).await.expect("terhubung");

        let (mic, note) = resolve_mic(&client, None).await;
        assert_eq!(mic, Some(MicInfo { name: "Mic/Aux".into(), muted: false }));
        assert_eq!(note, None);

        let (mic, _) = resolve_mic(&client, Some("Mic Podcast")).await;
        assert_eq!(mic, Some(MicInfo { name: "Mic Podcast".into(), muted: true }));

        let (mic, note) = resolve_mic(&client, Some("Tidak Ada")).await;
        assert_eq!(mic, None);
        assert!(note.unwrap().contains("tidak ditemukan"));

        let inputs = audio_inputs(&client).await.expect("daftar input");
        assert_eq!(inputs, vec!["Desktop Audio", "Mic Podcast", "Mic/Aux"]);
    }

    #[tokio::test]
    async fn controls_and_events() {
        let mock = start_mock(MockConfig::default()).await;
        let client = connect_to(&mock, None).await.expect("terhubung");
        let mut events = client.events().expect("event");
        let mut status = ObsStatus::new("uji".into());
        status.mic = Some(MicInfo { name: "Mic/Aux".into(), muted: false });
        let mut tracker = StatsTracker::default();

        // Ganti scene → event CurrentProgramSceneChanged.
        client.scenes().set_current_program_scene("BRB").await.expect("ganti scene");
        let event = tokio::time::timeout(Duration::from_secs(2), events.next()).await.unwrap().unwrap();
        assert_eq!(status::apply_event(&mut status, &mut tracker, 0, &event), EventEffect::Changed);
        assert_eq!(status.scene.as_deref(), Some("BRB"));
        assert_eq!(mock.current_scene(), "BRB");

        let missing = client.scenes().set_current_program_scene("Tidak Ada").await.unwrap_err();
        assert!(command_error(&missing).contains("Tidak ditemukan"));

        // Mute mic → event InputMuteStateChanged.
        assert!(client.inputs().toggle_mute(InputId::Name("Mic/Aux")).await.unwrap());
        let event = tokio::time::timeout(Duration::from_secs(2), events.next()).await.unwrap().unwrap();
        status::apply_event(&mut status, &mut tracker, 0, &event);
        assert_eq!(status.mic.as_ref().map(|m| m.muted), Some(true));
        assert_eq!(mock.is_muted("Mic/Aux"), Some(true));

        // Start stream dua kali → percobaan kedua ditolak.
        client.streaming().start().await.expect("start");
        assert!(mock.is_live());
        let again = client.streaming().start().await.unwrap_err();
        assert_eq!(command_error(&again), "Sudah berjalan.");
        client.streaming().stop().await.expect("stop");
        assert!(!mock.is_live());

        assert!(client.recording().toggle().await.unwrap());
        assert!(mock.is_recording());
    }

    #[tokio::test]
    async fn obs_shutdown_is_detected() {
        let mock = start_mock(MockConfig::default()).await;
        let client = connect_to(&mock, None).await.expect("terhubung");
        let mut events = client.events().expect("event");
        mock.shutdown();

        let mut status = ObsStatus::new("uji".into());
        let mut tracker = StatsTracker::default();
        let detected = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match events.next().await {
                    None => return true,
                    Some(e) => {
                        if let EventEffect::Closing(_) = status::apply_event(&mut status, &mut tracker, 0, &e) {
                            return true;
                        }
                    }
                }
            }
        })
        .await;
        assert_eq!(detected, Ok(true));
        // Request setelah putus gagal atau menggantung; `call` memberi batas waktu.
        let after = call(poll(&client)).await;
        assert!(after.is_err());
    }
}
