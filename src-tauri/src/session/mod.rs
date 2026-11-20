//! Servis ringkasan sesi stream.
//!
//! - Tiap detik membaca status OBS dan jumlah penonton YouTube, lalu meneruskannya ke
//!   perekam murni (`recorder.rs`).
//! - Chat, donasi Trakteer (live saja), dan alert dimasukkan lewat hook `on_*`.
//! - Riwayat 30 sesi disimpan di `sessions.json` (`store.rs`); sesi yang berjalan disimpan
//!   tiap menit agar bisa dipulihkan setelah crash.
//! - Saat sesi selesai: event `session://summary`, info singkat di overlay, dan dashboard
//!   dibuka di tab Riwayat (bisa dimatikan di pengaturan).

pub mod recorder;
pub mod store;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::MissedTickBehavior;

use crate::alerts::engine::Alert;
use crate::obs::ObsService;
use crate::settings::SettingsState;
use crate::youtube::parse::ChatMessage;
use crate::youtube::ChatService;
use recorder::{EndReason, Recorder, SessionSummary, Tick};
use store::StoreFile;

pub const SUMMARY_EVENT: &str = "session://summary";
const INTERVAL: Duration = Duration::from_secs(1);
const SAVE_EVERY_MS: u64 = 60_000;
/// Sesi sependek ini (mis. tes stream) tetap disimpan, tapi dashboard tidak dibuka otomatis.
const OPEN_MIN_MS: u64 = 120_000;

struct Inner {
    recorder: Recorder,
    history: Vec<SessionSummary>,
    /// Sesi yang harus langsung ditampilkan saat dashboard berikutnya meminta state.
    focus: Option<String>,
    path: Option<PathBuf>,
    last_save_t: u64,
}

pub struct SessionService {
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub current: Option<SessionSummary>,
    pub history: Vec<SessionSummary>,
    pub focus_id: Option<String>,
}

impl SessionService {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `take_focus`: penanda "buka sesi ini" hanya diberikan sekali.
    pub fn state(&self, take_focus: bool) -> SessionState {
        let mut inner = self.lock();
        let focus_id = if take_focus { inner.focus.take() } else { inner.focus.clone() };
        SessionState { current: inner.recorder.current(), history: inner.history.clone(), focus_id }
    }

    fn persist(&self) {
        let (path, file) = {
            let inner = self.lock();
            (inner.path.clone(), StoreFile::new(inner.history.clone(), inner.recorder.current()))
        };
        if let Some(path) = path {
            if let Err(e) = store::save(&path, &file) {
                log::warn!("gagal menyimpan riwayat sesi: {e}");
            }
        }
    }

    pub fn delete(&self, id: &str) -> SessionState {
        self.lock().history.retain(|s| s.id != id);
        self.persist();
        self.state(false)
    }

    pub fn clear(&self) -> SessionState {
        self.lock().history.clear();
        self.persist();
        self.state(false)
    }
}

fn wall_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// "45 menit", "1 jam 32 menit".
pub fn duration_text(ms: u64) -> String {
    let minutes = ms / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => format!("{} detik", ms / 1000),
        (0, m) => format!("{m} menit"),
        (h, 0) => format!("{h} jam"),
        (h, m) => format!("{h} jam {m} menit"),
    }
}

fn read_tick(app: &AppHandle) -> Tick {
    let obs_service = app.state::<ObsService>();
    let obs = obs_service.status();
    Tick {
        t_ms: obs_service.now_ms(),
        wall_ms: wall_ms(),
        obs_connected: obs.connection == crate::obs::Connection::Connected,
        live: obs.stream.active,
        reconnecting: obs.stream.reconnecting,
        duration_ms: obs.stream.duration_ms,
        bitrate_kbps: obs.stream.bitrate_kbps,
        dropped_frames: obs.stream.dropped_frames,
        total_frames: obs.stream.total_frames,
        last_stop: obs.last_stop,
        viewers: app.state::<ChatService>().status().viewers,
    }
}

/// Butuh `ObsService` dan `ChatService` sudah terdaftar.
pub fn init(app: &AppHandle) {
    let path = match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join(store::FILE_NAME)),
        Err(e) => {
            log::warn!("folder data aplikasi tidak ditemukan, riwayat sesi tidak disimpan: {e}");
            None
        }
    };
    let mut file = path.as_deref().map(store::load).unwrap_or_default();
    let recovered = file.recover();
    if recovered {
        log::info!("sesi stream yang tertinggal (aplikasi ditutup saat live) dipindah ke riwayat");
    }
    app.manage(SessionService {
        inner: Mutex::new(Inner {
            recorder: Recorder::default(),
            history: file.history,
            focus: None,
            path,
            last_save_t: 0,
        }),
    });
    if recovered {
        app.state::<SessionService>().persist();
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let tick = read_tick(&app);
            let svc = app.state::<SessionService>();
            let (ended, save) = {
                let mut inner = svc.lock();
                let ended = inner.recorder.on_tick(&tick);
                if let Some(s) = &ended {
                    store::push(&mut inner.history, s.clone());
                }
                let due = inner.recorder.is_active() && tick.t_ms.saturating_sub(inner.last_save_t) >= SAVE_EVERY_MS;
                let save = ended.is_some() || due;
                if save {
                    inner.last_save_t = tick.t_ms;
                }
                (ended, save)
            };
            if save {
                svc.persist();
            }
            if let Some(s) = ended {
                on_session_end(&app, &s);
            }
        }
    });
}

fn on_session_end(app: &AppHandle, s: &SessionSummary) {
    let reason = match s.end_reason {
        Some(EndReason::Lost) => "stream putus",
        _ => "dihentikan",
    };
    log::info!("sesi stream selesai ({reason}), durasi {}", duration_text(s.duration_ms));
    if let Err(e) = app.emit(SUMMARY_EVENT, s) {
        log::warn!("gagal emit ringkasan sesi: {e}");
    }
    crate::alerts::notify_info(
        app,
        "sessionEnded",
        format!("Stream selesai · {}", duration_text(s.duration_ms)),
        Some("Ringkasan sesi ada di dashboard → Riwayat.".into()),
        15_000,
    );
    let open = app.state::<SettingsState>().get().session.open_on_end;
    if open && s.duration_ms >= OPEN_MIN_MS {
        app.state::<SessionService>().lock().focus = Some(s.id.clone());
        let handle = app.clone();
        if let Err(e) = app.run_on_main_thread(move || crate::show_dashboard(&handle)) {
            log::warn!("gagal membuka dashboard: {e}");
        }
    }
}

/// Pesan chat baru (dari `youtube::apply`).
pub fn on_chat(app: &AppHandle, messages: &[ChatMessage]) {
    if messages.is_empty() {
        return;
    }
    if let Some(svc) = app.try_state::<SessionService>() {
        svc.lock().recorder.on_chat(messages);
    }
}

/// Donasi Trakteer live (bukan test/contoh).
pub fn on_donation(app: &AppHandle, supporter: &str, amount_rp: u64) {
    if let Some(svc) = app.try_state::<SessionService>() {
        svc.lock().recorder.on_donation(supporter, amount_rp);
    }
}

pub fn on_alerts(app: &AppHandle, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }
    if let Some(svc) = app.try_state::<SessionService>() {
        let mut inner = svc.lock();
        for alert in alerts {
            inner.recorder.on_alert(alert);
        }
    }
}

/// Saat aplikasi keluar: sesi yang berjalan disimpan dan dipulihkan saat start berikutnya.
pub fn save_now(app: &AppHandle) {
    if let Some(svc) = app.try_state::<SessionService>() {
        svc.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_text_formats() {
        assert_eq!(duration_text(42_000), "42 detik");
        assert_eq!(duration_text(45 * 60_000 + 30_000), "45 menit");
        assert_eq!(duration_text(120 * 60_000), "2 jam");
        assert_eq!(duration_text(92 * 60_000), "1 jam 32 menit");
    }
}
