//! Servis alert: tiap detik membaca status OBS & chat, menjalankan mesin aturan
//! (`engine.rs`), lalu menyiarkan daftar alert aktif lewat event `alert://update`.

pub mod engine;

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::time::MissedTickBehavior;

use crate::obs::ObsService;
use crate::settings::SettingsState;
use crate::trakteer::{TrakteerService, TrakteerState};
use crate::youtube::{ChatService, ChatState};
use engine::{Alert, ChatHealth, Engine, Snapshot, Tick, TrakteerHealth};

pub const UPDATE_EVENT: &str = "alert://update";
const INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub struct AlertService {
    engine: Mutex<Engine>,
}

impl AlertService {
    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn active(&self) -> Vec<Alert> {
        self.lock().active()
    }
}

fn emit(app: &AppHandle, alerts: &[Alert]) {
    if let Err(e) = app.emit(UPDATE_EVENT, alerts) {
        log::warn!("gagal emit alert: {e}");
    }
}

/// Ringkasan status OBS & chat untuk mesin aturan.
fn snapshot(app: &AppHandle) -> Snapshot {
    let obs_service = app.state::<ObsService>();
    let obs = obs_service.status();
    let chat = app.state::<ChatService>().status();
    let trakteer = app.state::<TrakteerService>().status();
    let trakteer = match trakteer.state {
        TrakteerState::Idle => TrakteerHealth::NotConfigured,
        TrakteerState::Connected => TrakteerHealth::Connected,
        TrakteerState::Connecting | TrakteerState::Retrying | TrakteerState::Failed => {
            TrakteerHealth::Problem(trakteer.message.unwrap_or_default())
        }
    };
    let perf = obs.perf.as_ref();
    let chat = match chat.state {
        ChatState::Idle => ChatHealth::NotConfigured,
        ChatState::Connected => ChatHealth::Connected,
        ChatState::Waiting | ChatState::Resolving => ChatHealth::Waiting,
        ChatState::Retrying | ChatState::Failed => {
            ChatHealth::Problem(chat.message.unwrap_or_else(|| "Chat YouTube bermasalah.".into()))
        }
    };
    Snapshot {
        t_ms: obs_service.now_ms(),
        obs_connected: obs.connection == crate::obs::Connection::Connected,
        live: obs.stream.active,
        reconnecting: obs.stream.reconnecting,
        bitrate_kbps: obs.stream.bitrate_kbps,
        dropped_pct: obs.stream.dropped_pct,
        congestion: obs.stream.congestion,
        cpu_pct: perf.map(|p| p.cpu_pct),
        render_lag_pct: perf.and_then(|p| p.render_lag_pct),
        encode_lag_pct: perf.and_then(|p| p.encode_lag_pct),
        mic_muted: obs.mic.as_ref().map(|m| m.muted),
        last_stop: obs.last_stop,
        app_stop_at_ms: obs.app_stop_at_ms,
        chat,
        trakteer,
    }
}

fn publish(app: &AppHandle, tick: Tick) {
    for alert in &tick.raised {
        // Tercatat di log agar bisa ditinjau setelah stream.
        log::info!(
            "alert {}: {}{}",
            alert.level.label(),
            alert.title,
            alert.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default()
        );
    }
    if tick.changed {
        emit(app, &app.state::<AlertService>().active());
    }
}

/// Butuh `ObsService` dan `ChatService` sudah terdaftar.
pub fn init(app: &AppHandle) {
    app.manage(AlertService::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(INTERVAL);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let snap = snapshot(&app);
            let cfg = app.state::<SettingsState>().get().alerts;
            let result = app.state::<AlertService>().lock().tick(&snap, &cfg);
            publish(&app, result);
        }
    });
}

pub fn dismiss(app: &AppHandle, id: &str) -> Vec<Alert> {
    let svc = app.state::<AlertService>();
    let (changed, list) = {
        let mut engine = svc.lock();
        let changed = engine.dismiss(id);
        (changed, engine.active())
    };
    if changed {
        emit(app, &list);
    }
    list
}

pub fn raise_test(app: &AppHandle) -> Vec<Alert> {
    let svc = app.state::<AlertService>();
    let now = app.state::<ObsService>().now_ms();
    let list = {
        let mut engine = svc.lock();
        engine.raise_test(now);
        engine.active()
    };
    emit(app, &list);
    list
}
