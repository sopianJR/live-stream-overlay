//! Live chat YouTube tanpa login (InnerTube) + jumlah penonton.
//!
//! Satu task di latar:
//! 1. membaca sumber dari pengaturan (URL video, URL channel, atau @handle);
//! 2. untuk channel, mencari live yang sedang berjalan lewat `/<channel>/live`
//!    (dicek ulang tiap 30 detik saat belum live);
//! 3. mengambil chat mode "Live chat" (semua pesan) tiap ±1,5 detik dan jumlah penonton
//!    tiap 30 detik;
//! 4. menyiarkan `chat://status` dan `chat://update` ke semua window.

pub mod account;
pub mod api;
pub mod buffer;
pub mod innertube;
pub mod oauth;
pub mod parse;
pub mod source;

/// Server HTTP tiruan untuk test OAuth & Data API (dipakai bersama oleh beberapa modul).
#[cfg(test)]
#[path = "../../tests/support/mock_http.rs"]
mod mock_http;

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::settings::SettingsState;
use buffer::{ChatBuffer, ChatUpdate};
use innertube::{FetchError, Innertube};
use parse::{ChatAction, ChatMessage, ChatPage, Continuation};
use source::Source;

pub const STATUS_EVENT: &str = "chat://status";
pub const UPDATE_EVENT: &str = "chat://update";

/// Pesan lama yang ditampilkan saat baru tersambung ke sebuah live.
const HISTORY_ON_CONNECT: usize = 20;
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// Batas atas jeda saat chat sepi (halaman terakhir kosong) dan YouTube menyarankan lebih lama.
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(3);
const VIEWERS_INTERVAL: Duration = Duration::from_secs(30);
const OFFLINE_RECHECK: Duration = Duration::from_secs(30);
const NO_CHAT_RECHECK: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Gagal polling berturut-turut sebelum sesi dianggap putus dan dimulai ulang.
const MAX_POLL_ERRORS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatState {
    /// Sumber chat belum diisi.
    Idle,
    Resolving,
    /// Belum live / live selesai; dicek ulang otomatis.
    Waiting,
    Connected,
    /// Gangguan jaringan; dicoba ulang otomatis.
    Retrying,
    /// Butuh tindakan user (sumber salah, live selesai untuk sumber berupa video).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStatus {
    pub state: ChatState,
    pub message: Option<String>,
    pub source: String,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub channel: Option<String>,
    pub viewers: Option<u64>,
}

impl ChatStatus {
    fn new(state: ChatState, source: &str, message: Option<String>) -> Self {
        Self {
            state,
            message,
            source: source.to_string(),
            video_id: None,
            title: None,
            channel: None,
            viewers: None,
        }
    }
}

/// Isi untuk window yang baru dibuka.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshot {
    pub status: ChatStatus,
    pub messages: Vec<ChatMessage>,
}

struct Shared {
    status: ChatStatus,
    buffer: ChatBuffer,
}

pub struct ChatService {
    shared: Mutex<Shared>,
    control: mpsc::UnboundedSender<()>,
}

impl ChatService {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> ChatStatus {
        self.lock().status.clone()
    }

    pub fn snapshot(&self) -> ChatSnapshot {
        let shared = self.lock();
        ChatSnapshot { status: shared.status.clone(), messages: shared.buffer.messages() }
    }

    /// Sumber chat berubah atau user meminta sambung ulang.
    pub fn reconnect(&self) {
        let _ = self.control.send(());
    }
}

pub fn init(app: &AppHandle) {
    let source = app.state::<SettingsState>().get().youtube.source;
    let (control, rx) = mpsc::unbounded_channel();
    app.manage(ChatService {
        shared: Mutex::new(Shared {
            status: ChatStatus::new(ChatState::Resolving, &source, None),
            buffer: ChatBuffer::default(),
        }),
        control,
    });
    tauri::async_runtime::spawn(run(app.clone(), rx));
}

fn set_status(app: &AppHandle, f: impl FnOnce(&mut ChatStatus)) {
    let svc = app.state::<ChatService>();
    let snapshot = {
        let mut shared = svc.lock();
        let before = shared.status.clone();
        f(&mut shared.status);
        if shared.status == before {
            return;
        }
        shared.status.clone()
    };
    if let Err(e) = app.emit(STATUS_EVENT, &snapshot) {
        log::warn!("gagal emit status chat: {e}");
    }
}

fn replace_status(app: &AppHandle, state: ChatState, source: &str, message: Option<String>) {
    set_status(app, |s| *s = ChatStatus::new(state, source, message));
}

fn publish(app: &AppHandle, update: ChatUpdate) {
    if update.is_empty() {
        return;
    }
    if let Err(e) = app.emit(UPDATE_EVENT, &update) {
        log::warn!("gagal emit pesan chat: {e}");
    }
}

fn apply(app: &AppHandle, actions: Vec<ChatAction>) {
    let update = app.state::<ChatService>().lock().buffer.apply(actions);
    crate::session::on_chat(app, &update.upserts);
    publish(app, update);
}

fn clear_messages(app: &AppHandle) {
    let update = app.state::<ChatService>().lock().buffer.clear();
    publish(app, update);
}

/// Hanya `limit` pesan terakhir dari riwayat yang ikut ditampilkan saat tersambung.
fn recent_history(actions: Vec<ChatAction>, limit: usize) -> Vec<ChatAction> {
    let upserts = actions.iter().filter(|a| matches!(a, ChatAction::Upsert { .. })).count();
    let mut skip = upserts.saturating_sub(limit);
    actions
        .into_iter()
        .filter(|a| {
            if skip > 0 && matches!(a, ChatAction::Upsert { .. }) {
                skip -= 1;
                false
            } else {
                true
            }
        })
        .collect()
}

/// Jeda polling: ikuti saran YouTube untuk continuation berwaktu, tapi tetap responsif.
/// Saat chat sepi (`idle`), jeda boleh lebih panjang (maks. 3 detik) agar request berkurang.
fn poll_delay(next: &Continuation, idle: bool) -> Duration {
    let max = if idle { IDLE_POLL_INTERVAL } else { POLL_INTERVAL };
    next.timeout_ms.map(Duration::from_millis).unwrap_or(POLL_INTERVAL).clamp(Duration::from_secs(1), max)
}

fn backoff(failures: u32) -> Duration {
    let secs = 2u64 << failures.saturating_sub(1).min(4);
    Duration::from_secs(secs).min(MAX_BACKOFF)
}

// ---------- Loop utama ----------

enum End {
    Reconnect,
    Shutdown,
    /// Belum/tidak live; dicek ulang setelah `after`.
    Wait { message: String, after: Duration },
    /// Gangguan sementara; dicoba ulang dengan backoff.
    Retry(String),
    /// Butuh tindakan user.
    Fail(String),
}

fn retry_or_fail(e: FetchError, not_found: &str) -> End {
    match e {
        FetchError::NotFound => End::Fail(not_found.to_string()),
        other => End::Retry(format!("Chat YouTube belum tersambung: {other}.")),
    }
}

async fn wait(rx: &mut mpsc::UnboundedReceiver<()>, delay: Option<Duration>) -> Option<bool> {
    // Some(true) = dibangunkan user, Some(false) = waktu habis, None = aplikasi ditutup.
    match delay {
        Some(d) => tokio::select! {
            _ = tokio::time::sleep(d) => Some(false),
            c = rx.recv() => c.map(|_| true),
        },
        None => rx.recv().await.map(|_| true),
    }
}

async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<()>) {
    let yt = match Innertube::new() {
        Ok(yt) => yt,
        Err(e) => {
            log::warn!("{e}");
            replace_status(&app, ChatState::Failed, "", Some(e));
            return;
        }
    };
    let mut failures = 0u32;
    let mut last_logged = String::new();

    loop {
        let raw = app.state::<SettingsState>().get().youtube.source;
        if raw.is_empty() {
            clear_messages(&app);
            replace_status(&app, ChatState::Idle, "", None);
            match wait(&mut rx, None).await {
                Some(_) => continue,
                None => return,
            }
        }

        let end = match source::parse(&raw) {
            Ok(source) => session(&app, &yt, &source, &raw, &mut rx).await,
            Err(e) => End::Fail(e),
        };

        let (state, message, delay) = match end {
            End::Reconnect => {
                failures = 0;
                clear_messages(&app);
                continue;
            }
            End::Shutdown => return,
            End::Wait { message, after } => {
                failures = 0;
                (ChatState::Waiting, message, Some(after))
            }
            End::Retry(message) => {
                failures += 1;
                (ChatState::Retrying, message, Some(backoff(failures)))
            }
            End::Fail(message) => (ChatState::Failed, message, None),
        };
        if message != last_logged {
            log::info!("chat YouTube ({raw}): {message}");
            last_logged.clone_from(&message);
        }
        replace_status(&app, state, &raw, Some(message));

        match wait(&mut rx, delay).await {
            Some(true) => {
                failures = 0;
                last_logged.clear();
            }
            Some(false) => {}
            None => return,
        }
    }
}

/// Menentukan video live dari sumber.
async fn resolve(yt: &Innertube, source: &Source) -> Result<String, End> {
    match source {
        Source::Video(id) => Ok(id.clone()),
        Source::Channel(path) => {
            let page = yt
                .live_page(path)
                .await
                .map_err(|e| retry_or_fail(e, "Channel tidak ditemukan. Periksa @handle atau URL channel."))?;
            match page.video_id {
                Some(_) if page.upcoming && !page.live_now => Err(End::Wait {
                    message: "Live sudah dijadwalkan. Menunggu live dimulai…".into(),
                    after: OFFLINE_RECHECK,
                }),
                Some(id) => Ok(id),
                None => Err(End::Wait {
                    message: "Channel belum live. Dicek ulang tiap 30 detik.".into(),
                    after: OFFLINE_RECHECK,
                }),
            }
        }
    }
}

fn ended(source: &Source) -> End {
    match source {
        Source::Channel(_) => End::Wait {
            message: "Live selesai. Menunggu live berikutnya…".into(),
            after: OFFLINE_RECHECK,
        },
        Source::Video(_) => End::Fail(
            "Live ini sudah selesai. Ganti sumber chat dengan live berikutnya, atau pakai @handle \
             agar live baru terdeteksi otomatis."
                .into(),
        ),
    }
}

/// Halaman chat pertama, beralih ke mode "Live chat" (semua pesan) jika tersedia.
async fn open_chat(yt: &Innertube, top_token: &str) -> Result<Option<ChatPage>, FetchError> {
    let Some(first) = yt.chat_page(top_token).await? else { return Ok(None) };
    let Some(live_token) = &first.live_chat_token else { return Ok(Some(first)) };
    match yt.chat_page(live_token).await {
        Ok(Some(live)) => Ok(Some(live)),
        // Mode Live chat gagal dibuka: tetap pakai Top chat daripada tanpa chat.
        Ok(None) | Err(_) => {
            log::warn!("mode Live chat tidak tersedia, memakai Top chat");
            Ok(Some(first))
        }
    }
}

async fn session(
    app: &AppHandle,
    yt: &Innertube,
    source: &Source,
    raw: &str,
    rx: &mut mpsc::UnboundedReceiver<()>,
) -> End {
    // Saat cek ulang berkala, biarkan status "menunggu" tetap tampil (tidak berkedip).
    set_status(app, |s| {
        if s.state != ChatState::Waiting || s.source != raw {
            *s = ChatStatus::new(ChatState::Resolving, raw, Some("Mencari live…".into()));
        }
    });

    let video_id = match resolve(yt, source).await {
        Ok(id) => id,
        Err(end) => return end,
    };
    let info = match yt.video(&video_id).await {
        Ok(info) => info,
        Err(e) => return retry_or_fail(e, "Video tidak ditemukan. Periksa URL video."),
    };
    if info.is_replay {
        return ended(source);
    }
    let Some(top_token) = info.chat_token.clone() else {
        return if info.is_live {
            End::Wait { message: "Live chat dimatikan untuk live ini.".into(), after: NO_CHAT_RECHECK }
        } else {
            End::Wait {
                message: "Video belum live atau tidak punya live chat. Menunggu…".into(),
                after: OFFLINE_RECHECK,
            }
        };
    };
    let page = match open_chat(yt, &top_token).await {
        Ok(Some(page)) => page,
        Ok(None) => return End::Wait { message: "Live chat belum tersedia.".into(), after: OFFLINE_RECHECK },
        Err(e) => return retry_or_fail(e, "Live chat tidak ditemukan."),
    };

    log::info!("chat YouTube tersambung ke video {video_id}");
    clear_messages(app);
    set_status(app, |s| {
        *s = ChatStatus {
            state: ChatState::Connected,
            message: None,
            source: raw.to_string(),
            video_id: Some(video_id.clone()),
            title: info.title.clone(),
            channel: info.channel.clone(),
            viewers: info.viewers,
        };
    });
    apply(app, recent_history(page.actions, HISTORY_ON_CONNECT));

    let Some(mut next) = page.next else { return ended(source) };
    let mut poll_at = Instant::now() + poll_delay(&next, false);
    let mut idle = false;
    let mut viewers_at = Instant::now() + VIEWERS_INTERVAL;
    let mut errors = 0u32;

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(poll_at) => {
                match yt.chat_page(&next.token).await {
                    Ok(Some(page)) => {
                        if errors > 0 {
                            errors = 0;
                            set_status(app, |s| s.message = None);
                        }
                        idle = page.actions.is_empty();
                        apply(app, page.actions);
                        match page.next {
                            Some(n) => next = n,
                            None => return ended(source),
                        }
                    }
                    Ok(None) => return ended(source),
                    Err(e) => {
                        errors += 1;
                        if errors >= MAX_POLL_ERRORS {
                            return End::Retry(format!("Chat YouTube terputus: {e}."));
                        }
                        set_status(app, |s| s.message = Some(format!("Gangguan sementara: {e}.")));
                    }
                }
                poll_at = Instant::now() + poll_delay(&next, idle) * (errors + 1);
            }
            _ = tokio::time::sleep_until(viewers_at) => {
                if let Ok(Some(v)) = yt.viewership(&video_id).await {
                    set_status(app, |s| s.viewers = v.viewers);
                }
                viewers_at = Instant::now() + VIEWERS_INTERVAL;
            }
            control = rx.recv() => return if control.is_some() { End::Reconnect } else { End::Shutdown },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse::{Author, MessageKind, Run};
    use super::*;

    fn add(id: &str) -> ChatAction {
        ChatAction::Upsert {
            replaces: None,
            message: ChatMessage {
                id: id.into(),
                kind: MessageKind::Text,
                author: Author::default(),
                runs: vec![Run::Text { text: id.into() }],
                timestamp_ms: 0,
                amount: None,
                header: None,
                sticker: None,
                colors: None,
            },
        }
    }

    #[test]
    fn history_keeps_latest_messages_and_all_removals() {
        let actions = vec![add("1"), ChatAction::Remove("x".into()), add("2"), add("3"), add("4")];
        let kept = recent_history(actions, 2);
        assert_eq!(kept, vec![ChatAction::Remove("x".into()), add("3"), add("4")]);
    }

    #[test]
    fn poll_delay_is_clamped() {
        let c = |t: Option<u64>| Continuation { token: String::new(), timeout_ms: t };
        assert_eq!(poll_delay(&c(None), false), POLL_INTERVAL);
        assert_eq!(poll_delay(&c(Some(10_000)), false), POLL_INTERVAL);
        assert_eq!(poll_delay(&c(Some(200)), false), Duration::from_secs(1));
        // Chat sepi: saran YouTube yang lebih panjang diikuti sampai 3 detik.
        assert_eq!(poll_delay(&c(Some(10_000)), true), IDLE_POLL_INTERVAL);
        assert_eq!(poll_delay(&c(Some(2_000)), true), Duration::from_secs(2));
        assert_eq!(poll_delay(&c(None), true), POLL_INTERVAL);
    }

    #[test]
    fn backoff_caps() {
        let secs: Vec<u64> = (1..=7).map(|f| backoff(f).as_secs()).collect();
        assert_eq!(secs, vec![2, 4, 8, 16, 30, 30, 30]);
    }
}
