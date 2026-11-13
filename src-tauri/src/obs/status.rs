//! Snapshot status OBS yang dikirim ke frontend, beserta fungsi murni yang
//! memperbaruinya dari hasil polling dan event OBS.

use obws::events::{Event, OutputState};
use obws::responses::general::Stats;
use obws::responses::recording::RecordStatus;
use obws::responses::scenes::Scene;
use obws::responses::streaming::StreamStatus;
use serde::Serialize;

use super::stats::StatsTracker;

/// Transisi start/stop yang tidak selesai dalam waktu ini dianggap batal.
const TRANSITION_TIMEOUT_MS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Connecting,
    Connected,
    /// Gagal sementara (OBS belum dibuka, WebSocket belum aktif, koneksi putus); dicoba ulang otomatis.
    Retrying,
    /// Password salah atau belum diisi. Tidak dicoba ulang sampai pengaturan/password berubah.
    AuthFailed,
    /// Tidak akan pulih tanpa tindakan user (mis. versi OBS terlalu lama).
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Transition {
    Starting,
    Stopping,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub active: bool,
    pub reconnecting: bool,
    pub transition: Option<Transition>,
    #[serde(skip)]
    transition_at_ms: u64,
    pub duration_ms: u64,
    /// Rata-rata 3 detik terakhir. `None` jika belum cukup sampel atau tidak live.
    pub bitrate_kbps: Option<f64>,
    /// Frame di-drop karena jaringan, 10 detik terakhir.
    pub dropped_pct: Option<f64>,
    /// Kumulatif sejak stream dimulai (seperti dock Stats OBS).
    pub dropped_frames: u32,
    pub total_frames: u32,
    pub congestion: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordInfo {
    pub active: bool,
    pub paused: bool,
    pub transition: Option<Transition>,
    #[serde(skip)]
    transition_at_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfInfo {
    pub fps: f64,
    /// CPU yang dipakai proses OBS (persen).
    pub cpu_pct: f64,
    pub render_lag_pct: Option<f64>,
    pub encode_lag_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicInfo {
    pub name: String,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsStatus {
    pub connection: Connection,
    /// Penjelasan untuk user saat tidak terhubung.
    pub message: Option<String>,
    /// Percobaan ulang berturut-turut yang gagal.
    pub attempt: u32,
    /// `host:port` yang sedang dituju.
    pub endpoint: String,
    pub obs_version: Option<String>,
    pub stream: StreamInfo,
    pub record: RecordInfo,
    pub perf: Option<PerfInfo>,
    pub scene: Option<String>,
    /// Urutan sama seperti panel Scenes di OBS (paling atas dulu).
    pub scenes: Vec<String>,
    pub mic: Option<MicInfo>,
    /// Alasan mic tidak tersedia (mis. input tidak ditemukan).
    pub mic_note: Option<String>,
}

impl ObsStatus {
    pub fn new(endpoint: String) -> Self {
        Self {
            connection: Connection::Connecting,
            message: None,
            attempt: 0,
            endpoint,
            obs_version: None,
            stream: StreamInfo::default(),
            record: RecordInfo::default(),
            perf: None,
            scene: None,
            scenes: Vec::new(),
            mic: None,
            mic_note: None,
        }
    }

    /// Status tanpa koneksi: semua data sesi sebelumnya dibuang.
    pub fn set_disconnected(
        &mut self,
        connection: Connection,
        message: Option<String>,
        attempt: u32,
        endpoint: &str,
    ) {
        *self = Self { connection, message, attempt, ..Self::new(endpoint.to_string()) };
    }
}

/// Hasil satu putaran polling.
#[derive(Debug, Clone)]
pub struct PollSample {
    pub stream: StreamStatus,
    pub record: RecordStatus,
    pub stats: Stats,
}

fn millis(d: time::Duration) -> u64 {
    u64::try_from(d.whole_milliseconds().max(0)).unwrap_or(u64::MAX)
}

fn expire_transition(transition: &mut Option<Transition>, since_ms: u64, t_ms: u64) {
    if transition.is_some() && t_ms.saturating_sub(since_ms) > TRANSITION_TIMEOUT_MS {
        *transition = None;
    }
}

pub fn apply_poll(status: &mut ObsStatus, tracker: &mut StatsTracker, t_ms: u64, p: &PollSample) {
    let s = &p.stream;
    let rates = tracker.update_stream(t_ms, s.active, s.bytes, s.skipped_frames, s.total_frames);
    let stream = &mut status.stream;
    // Transisi selesai begitu polling melihat keadaan akhirnya.
    match stream.transition {
        Some(Transition::Starting) if s.active => stream.transition = None,
        Some(Transition::Stopping) if !s.active => stream.transition = None,
        _ => expire_transition(&mut stream.transition, stream.transition_at_ms, t_ms),
    }
    stream.active = s.active;
    stream.reconnecting = s.active && s.reconnecting;
    stream.duration_ms = if s.active { millis(s.duration) } else { 0 };
    stream.bitrate_kbps = rates.bitrate_kbps;
    stream.dropped_pct = rates.dropped_pct;
    stream.dropped_frames = if s.active { s.skipped_frames } else { 0 };
    stream.total_frames = if s.active { s.total_frames } else { 0 };
    stream.congestion = if s.active { s.congestion } else { 0.0 };

    let r = &p.record;
    let record = &mut status.record;
    match record.transition {
        Some(Transition::Starting) if r.active => record.transition = None,
        Some(Transition::Stopping) if !r.active => record.transition = None,
        _ => expire_transition(&mut record.transition, record.transition_at_ms, t_ms),
    }
    record.active = r.active;
    record.paused = r.active && r.paused;
    record.duration_ms = if r.active { millis(r.duration) } else { 0 };

    let st = &p.stats;
    let perf = tracker.update_perf(
        t_ms,
        st.render_skipped_frames,
        st.render_total_frames,
        st.output_skipped_frames,
        st.output_total_frames,
    );
    status.perf = Some(PerfInfo {
        fps: st.active_fps,
        cpu_pct: st.cpu_usage,
        render_lag_pct: perf.render_lag_pct,
        encode_lag_pct: perf.encode_lag_pct,
    });
}

/// Nama scene dengan urutan seperti di OBS: indeks terbesar tampil paling atas.
fn ordered_names<'a>(scenes: impl IntoIterator<Item = (usize, &'a str)>) -> Vec<String> {
    let mut sorted: Vec<(usize, &str)> = scenes.into_iter().collect();
    sorted.sort_by_key(|&(index, _)| std::cmp::Reverse(index));
    sorted.into_iter().map(|(_, name)| name.to_string()).collect()
}

/// Dari respons `GetSceneList`.
pub fn scene_names(scenes: &[Scene]) -> Vec<String> {
    ordered_names(scenes.iter().map(|s| (s.index, s.id.name.as_str())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventEffect {
    Unchanged,
    Changed,
    /// OBS menutup koneksi; sesi harus diakhiri dengan alasan ini.
    Closing(&'static str),
}

pub fn apply_event(
    status: &mut ObsStatus,
    tracker: &mut StatsTracker,
    t_ms: u64,
    event: &Event,
) -> EventEffect {
    match event {
        Event::StreamStateChanged { active, state } => {
            let stream = &mut status.stream;
            stream.active = *active;
            match state {
                OutputState::Starting => {
                    stream.transition = Some(Transition::Starting);
                    stream.transition_at_ms = t_ms;
                }
                OutputState::Stopping => {
                    stream.transition = Some(Transition::Stopping);
                    stream.transition_at_ms = t_ms;
                }
                OutputState::Reconnecting => stream.reconnecting = true,
                OutputState::Reconnected => stream.reconnecting = false,
                OutputState::Started => {
                    stream.transition = None;
                    stream.reconnecting = false;
                }
                OutputState::Stopped => {
                    tracker.reset_stream();
                    *stream = StreamInfo::default();
                }
                _ => return EventEffect::Unchanged,
            }
            EventEffect::Changed
        }
        Event::RecordStateChanged { active, state, .. } => {
            let record = &mut status.record;
            record.active = *active;
            match state {
                OutputState::Starting => {
                    record.transition = Some(Transition::Starting);
                    record.transition_at_ms = t_ms;
                }
                OutputState::Stopping => {
                    record.transition = Some(Transition::Stopping);
                    record.transition_at_ms = t_ms;
                }
                OutputState::Started => record.transition = None,
                OutputState::Stopped => *record = RecordInfo::default(),
                OutputState::Paused => record.paused = true,
                OutputState::Resumed => record.paused = false,
                _ => return EventEffect::Unchanged,
            }
            EventEffect::Changed
        }
        Event::CurrentProgramSceneChanged { id } => {
            status.scene = Some(id.name.clone());
            EventEffect::Changed
        }
        Event::SceneListChanged { scenes } => {
            status.scenes = ordered_names(scenes.iter().map(|s| (s.index, s.name.as_str())));
            EventEffect::Changed
        }
        Event::SceneNameChanged { old_name, new_name, .. } => {
            for name in status.scenes.iter_mut().chain(status.scene.as_mut()) {
                if name == old_name {
                    name.clone_from(new_name);
                }
            }
            EventEffect::Changed
        }
        Event::InputMuteStateChanged { id, muted } => match &mut status.mic {
            Some(mic) if mic.name == id.name => {
                mic.muted = *muted;
                EventEffect::Changed
            }
            _ => EventEffect::Unchanged,
        },
        Event::InputNameChanged { old_name, new_name, .. } => match &mut status.mic {
            Some(mic) if &mic.name == old_name => {
                mic.name.clone_from(new_name);
                EventEffect::Changed
            }
            _ => EventEffect::Unchanged,
        },
        Event::InputRemoved { id } if status.mic.as_ref().is_some_and(|m| m.name == id.name) => {
            status.mic_note = Some(format!("Input mic \"{}\" dihapus dari OBS.", id.name));
            status.mic = None;
            EventEffect::Changed
        }
        Event::ExitStarted => EventEffect::Closing("OBS sedang ditutup."),
        Event::ServerStopping | Event::ServerStopped => {
            EventEffect::Closing("Server WebSocket OBS berhenti.")
        }
        _ => EventEffect::Unchanged,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const UUID: &str = "00000000-0000-4000-8000-000000000001";

    fn ev(ty: &str, data: serde_json::Value) -> Event {
        // Seperti OBS: event tanpa data tidak membawa `eventData`.
        let raw = if data.is_null() {
            json!({ "eventType": ty })
        } else {
            json!({ "eventType": ty, "eventData": data })
        };
        serde_json::from_value(raw).expect("event valid")
    }

    fn stream_ev(active: bool, state: &str) -> Event {
        ev(
            "StreamStateChanged",
            json!({ "outputActive": active, "outputState": format!("OBS_WEBSOCKET_OUTPUT_{state}") }),
        )
    }

    fn connected() -> ObsStatus {
        let mut s = ObsStatus::new("127.0.0.1:4455".into());
        s.connection = Connection::Connected;
        s.mic = Some(MicInfo { name: "Mic/Aux".into(), muted: false });
        s.scene = Some("Game".into());
        s.scenes = vec!["Game".into(), "BRB".into()];
        s
    }

    fn poll(active: bool, bytes: u64, duration_ms: i64) -> PollSample {
        PollSample {
            stream: StreamStatus {
                active,
                bytes,
                duration: time::Duration::milliseconds(duration_ms),
                total_frames: (duration_ms / 16) as u32,
                ..StreamStatus::default()
            },
            record: RecordStatus::default(),
            stats: Stats { active_fps: 60.0, cpu_usage: 3.0, ..Stats::default() },
        }
    }

    #[test]
    fn stream_lifecycle_from_events_and_polls() {
        let mut s = connected();
        let mut t = StatsTracker::default();

        assert_eq!(apply_event(&mut s, &mut t, 0, &stream_ev(false, "STARTING")), EventEffect::Changed);
        assert_eq!(s.stream.transition, Some(Transition::Starting));
        // Polling sebelum stream benar-benar aktif tidak menghapus transisi.
        apply_poll(&mut s, &mut t, 500, &poll(false, 0, 0));
        assert_eq!(s.stream.transition, Some(Transition::Starting));

        apply_event(&mut s, &mut t, 900, &stream_ev(true, "STARTED"));
        assert!(s.stream.active);
        assert_eq!(s.stream.transition, None);

        apply_poll(&mut s, &mut t, 1_000, &poll(true, 0, 1_000));
        apply_poll(&mut s, &mut t, 2_000, &poll(true, 750_000, 2_000));
        assert_eq!(s.stream.duration_ms, 2_000);
        assert!((s.stream.bitrate_kbps.unwrap() - 6000.0).abs() < 1e-6);

        apply_event(&mut s, &mut t, 2_500, &stream_ev(true, "RECONNECTING"));
        assert!(s.stream.reconnecting);
        apply_event(&mut s, &mut t, 2_600, &stream_ev(true, "RECONNECTED"));
        assert!(!s.stream.reconnecting);

        apply_event(&mut s, &mut t, 3_000, &stream_ev(true, "STOPPING"));
        assert_eq!(s.stream.transition, Some(Transition::Stopping));
        apply_event(&mut s, &mut t, 3_100, &stream_ev(false, "STOPPED"));
        assert_eq!(s.stream, StreamInfo::default());
    }

    #[test]
    fn stuck_transition_expires() {
        let mut s = connected();
        let mut t = StatsTracker::default();
        apply_event(&mut s, &mut t, 0, &stream_ev(false, "STARTING"));
        apply_poll(&mut s, &mut t, TRANSITION_TIMEOUT_MS + 1, &poll(false, 0, 0));
        assert_eq!(s.stream.transition, None);
    }

    #[test]
    fn record_pause_and_stop() {
        let mut s = connected();
        let mut t = StatsTracker::default();
        let rec = |active: bool, state: &str| {
            ev(
                "RecordStateChanged",
                json!({ "outputActive": active, "outputState": format!("OBS_WEBSOCKET_OUTPUT_{state}"), "outputPath": null }),
            )
        };
        apply_event(&mut s, &mut t, 0, &rec(true, "STARTED"));
        apply_event(&mut s, &mut t, 1, &rec(true, "PAUSED"));
        assert!(s.record.active && s.record.paused);
        apply_event(&mut s, &mut t, 2, &rec(true, "RESUMED"));
        assert!(!s.record.paused);
        apply_event(&mut s, &mut t, 3, &rec(false, "STOPPED"));
        assert_eq!(s.record, RecordInfo::default());
    }

    #[test]
    fn mic_events_only_affect_tracked_input() {
        let mut s = connected();
        let mut t = StatsTracker::default();
        let mute = |name: &str| {
            ev("InputMuteStateChanged", json!({ "inputName": name, "inputUuid": UUID, "inputMuted": true }))
        };
        assert_eq!(apply_event(&mut s, &mut t, 0, &mute("Desktop Audio")), EventEffect::Unchanged);
        assert!(!s.mic.as_ref().unwrap().muted);
        assert_eq!(apply_event(&mut s, &mut t, 0, &mute("Mic/Aux")), EventEffect::Changed);
        assert!(s.mic.as_ref().unwrap().muted);

        let rename = ev(
            "InputNameChanged",
            json!({ "inputUuid": UUID, "oldInputName": "Mic/Aux", "inputName": "Mic Utama" }),
        );
        apply_event(&mut s, &mut t, 0, &rename);
        assert_eq!(s.mic.as_ref().unwrap().name, "Mic Utama");

        let removed = ev("InputRemoved", json!({ "inputName": "Mic Utama", "inputUuid": UUID }));
        apply_event(&mut s, &mut t, 0, &removed);
        assert_eq!(s.mic, None);
        assert!(s.mic_note.unwrap().contains("dihapus"));
    }

    #[test]
    fn scene_events() {
        let mut s = connected();
        let mut t = StatsTracker::default();
        let switched = ev("CurrentProgramSceneChanged", json!({ "sceneName": "BRB", "sceneUuid": UUID }));
        apply_event(&mut s, &mut t, 0, &switched);
        assert_eq!(s.scene.as_deref(), Some("BRB"));

        let renamed = ev(
            "SceneNameChanged",
            json!({ "sceneUuid": UUID, "oldSceneName": "BRB", "sceneName": "Istirahat" }),
        );
        apply_event(&mut s, &mut t, 0, &renamed);
        assert_eq!(s.scene.as_deref(), Some("Istirahat"));
        assert_eq!(s.scenes, vec!["Game", "Istirahat"]);

        let list = ev(
            "SceneListChanged",
            json!({ "scenes": [
                { "sceneIndex": 0, "sceneName": "Bawah", "sceneUuid": UUID },
                { "sceneIndex": 2, "sceneName": "Atas", "sceneUuid": UUID },
                { "sceneIndex": 1, "sceneName": "Tengah", "sceneUuid": UUID },
            ] }),
        );
        apply_event(&mut s, &mut t, 0, &list);
        assert_eq!(s.scenes, vec!["Atas", "Tengah", "Bawah"]);
    }

    #[test]
    fn obs_shutdown_closes_session() {
        let mut s = connected();
        let mut t = StatsTracker::default();
        let exit = ev("ExitStarted", serde_json::Value::Null);
        assert!(matches!(apply_event(&mut s, &mut t, 0, &exit), EventEffect::Closing(_)));
    }

    #[test]
    fn disconnect_clears_session_data() {
        let mut s = connected();
        s.stream.active = true;
        s.set_disconnected(Connection::Retrying, Some("putus".into()), 2, "127.0.0.1:4455");
        assert_eq!(s.connection, Connection::Retrying);
        assert_eq!(s.attempt, 2);
        assert!(!s.stream.active && s.mic.is_none() && s.scenes.is_empty());
    }
}
