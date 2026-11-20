//! Mesin alert: aturan murni yang dievaluasi tiap detik dari snapshot status OBS & chat.
//!
//! Tiga jenis alert:
//! - **Berbasis kondisi** (frame drop, bitrate, CPU, mic, chat): punya debounce (kondisi harus
//!   bertahan dulu), lama tampil, jeda sebelum muncul lagi (cooldown), dan histeresis sebelum
//!   dianggap pulih. Level kritis tampil terus selama kondisinya ada.
//! - **Berbasis kejadian** (stream putus, OBS putus saat live): menetap sampai pulih atau ditutup.
//! - **Info singkat** (stream dihentikan dari OBS, stream tersambung kembali): hilang sendiri.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::obs::{MicAudio, StreamStop};
use crate::settings::AlertSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Info,
    Reminder,
    Warning,
    Critical,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Reminder => "pengingat",
            Level::Warning => "peringatan",
            Level::Critical => "KRITIS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub detail: Option<String>,
    pub raised_at_ms: u64,
    /// Naik setiap kali alert (di)tampilkan, agar UI bisa memberi animasi masuk.
    pub seq: u64,
}

/// Kesehatan chat YouTube dari sudut pandang alert.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChatHealth {
    /// Sumber chat tidak diisi: tidak ada alert chat.
    #[default]
    NotConfigured,
    Connected,
    /// Live belum terdeteksi di YouTube.
    Waiting,
    /// Gagal / mencoba ulang; berisi penjelasan.
    Problem(String),
}

/// Kesehatan koneksi Trakteer dari sudut pandang alert.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TrakteerHealth {
    /// Kunci tidak diisi: tidak ada alert.
    #[default]
    NotConfigured,
    Connected,
    Problem(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub t_ms: u64,
    pub obs_connected: bool,
    pub live: bool,
    pub reconnecting: bool,
    pub bitrate_kbps: Option<f64>,
    pub dropped_pct: Option<f64>,
    pub congestion: f32,
    pub cpu_pct: Option<f64>,
    pub render_lag_pct: Option<f64>,
    pub encode_lag_pct: Option<f64>,
    pub mic_muted: Option<bool>,
    pub mic_name: Option<String>,
    /// Level mic dari OBS; `None` jika tidak diketahui (langganan mati, OBS tidak mengirim).
    pub mic_audio: Option<MicAudio>,
    pub last_stop: Option<StreamStop>,
    pub app_stop_at_ms: Option<u64>,
    pub chat: ChatHealth,
    pub trakteer: TrakteerHealth,
}

struct Content {
    level: Level,
    title: String,
    detail: Option<String>,
}

impl Content {
    fn new(level: Level, title: impl Into<String>, detail: &str) -> Self {
        Self { level, title: title.into(), detail: Some(detail.to_string()) }
    }
}

#[derive(Clone, Copy)]
struct Spec {
    id: &'static str,
    /// Kondisi harus bertahan selama ini sebelum alert muncul.
    debounce_ms: u64,
    /// Lama tampil untuk level selain kritis.
    show_ms: u64,
    /// Muncul lagi setelah jeda ini jika kondisi masih ada (`None` = sekali saja).
    repeat_ms: Option<u64>,
    /// Kondisi harus hilang selama ini sebelum aturan direset (mencegah kedip).
    clear_ms: u64,
}

const RECONNECTING: Spec =
    Spec { id: "reconnecting", debounce_ms: 0, show_ms: 0, repeat_ms: None, clear_ms: 0 };
const DROPPED: Spec =
    Spec { id: "droppedFrames", debounce_ms: 3_000, show_ms: 10_000, repeat_ms: Some(60_000), clear_ms: 5_000 };
const LOW_BITRATE: Spec =
    Spec { id: "lowBitrate", debounce_ms: 10_000, show_ms: 10_000, repeat_ms: Some(60_000), clear_ms: 10_000 };
const CONGESTION: Spec =
    Spec { id: "congestion", debounce_ms: 10_000, show_ms: 10_000, repeat_ms: Some(120_000), clear_ms: 10_000 };
const CPU: Spec = Spec { id: "cpuHigh", debounce_ms: 10_000, show_ms: 10_000, repeat_ms: Some(120_000), clear_ms: 10_000 };
const RENDER_LAG: Spec =
    Spec { id: "renderLag", debounce_ms: 5_000, show_ms: 10_000, repeat_ms: Some(120_000), clear_ms: 10_000 };
const ENCODE_LAG: Spec =
    Spec { id: "encodeLag", debounce_ms: 5_000, show_ms: 10_000, repeat_ms: Some(120_000), clear_ms: 10_000 };
const MIC_MUTED: Spec =
    Spec { id: "micMuted", debounce_ms: 10_000, show_ms: 8_000, repeat_ms: Some(120_000), clear_ms: 0 };
const MIC_SILENT: Spec =
    Spec { id: "micSilent", debounce_ms: 0, show_ms: 10_000, repeat_ms: Some(300_000), clear_ms: 0 };
const MIC_CLIPPING: Spec =
    Spec { id: "micClipping", debounce_ms: 0, show_ms: 8_000, repeat_ms: Some(120_000), clear_ms: 10_000 };
const CHAT_PROBLEM: Spec =
    Spec { id: "chatProblem", debounce_ms: 30_000, show_ms: 10_000, repeat_ms: Some(300_000), clear_ms: 0 };
const TRAKTEER_LOST: Spec =
    Spec { id: "trakteerLost", debounce_ms: 30_000, show_ms: 10_000, repeat_ms: Some(300_000), clear_ms: 0 };
const CHAT_NOT_LIVE: Spec =
    Spec { id: "chatNotLive", debounce_ms: 60_000, show_ms: 10_000, repeat_ms: Some(300_000), clear_ms: 0 };

pub const CONGESTION_THRESHOLD: f32 = 0.6;
pub const LAG_THRESHOLD_PCT: f64 = 2.0;
/// Suara mic pecah selama sekian detik (dari 10 detik terakhir) memicu peringatan.
pub const CLIP_MIN_SECONDS: u32 = 3;
/// Bitrate normal = rata-rata detik ke-10 s.d. ke-70 setelah live (lewati fase awal yang naik-turun).
const BASELINE_WARMUP_MS: u64 = 10_000;
const BASELINE_END_MS: u64 = 70_000;
const BASELINE_MIN_SAMPLES: u32 = 20;
/// Event STOPPED dianggap milik deteksi stop jika terjadi selambatnya selama ini sebelumnya.
const STOP_MATCH_MS: u64 = 5_000;
/// Menunggu event STOPPED setelah polling melihat stream mati.
const STOP_EVENT_WAIT_MS: u64 = 3_000;
/// Stop yang diminta aplikasi dalam rentang ini tidak dianggap masalah.
const APP_STOP_WINDOW_MS: u64 = 20_000;

#[derive(Default)]
struct RuleState {
    pending_since: Option<u64>,
    false_since: Option<u64>,
    shown_at: Option<u64>,
    shown_level: Option<Level>,
    visible: bool,
    dismissed: bool,
    alert: Option<Alert>,
}

struct EventAlert {
    alert: Alert,
    expires_at: Option<u64>,
}

#[derive(Default)]
struct Baseline {
    live_since: Option<u64>,
    sum: f64,
    samples: u32,
    value: Option<f64>,
}

impl Baseline {
    fn update(&mut self, t: u64, live: bool, bitrate: Option<f64>) {
        if !live {
            *self = Self::default();
            return;
        }
        let since = *self.live_since.get_or_insert(t);
        if self.value.is_some() {
            return;
        }
        let elapsed = t.saturating_sub(since);
        if elapsed >= BASELINE_WARMUP_MS {
            if let Some(b) = bitrate.filter(|b| *b > 0.0) {
                self.sum += b;
                self.samples += 1;
            }
        }
        if elapsed >= BASELINE_END_MS && self.samples >= BASELINE_MIN_SAMPLES {
            self.value = Some(self.sum / f64::from(self.samples));
        }
    }
}

/// "2 menit", "90 detik".
fn duration_text(secs: u32) -> String {
    if secs >= 60 && secs.is_multiple_of(60) { format!("{} menit", secs / 60) } else { format!("{secs} detik") }
}

fn mbps(kbps: f64) -> String {
    format!("{:.1} Mbps", kbps / 1000.0)
}

#[derive(Default)]
pub struct Engine {
    rules: HashMap<&'static str, RuleState>,
    events: Vec<EventAlert>,
    prev: Option<Snapshot>,
    /// Polling melihat stream mati; menunggu event STOPPED untuk tahu penyebabnya.
    pending_stop: Option<u64>,
    baseline: Baseline,
    /// Sejak kapan live dengan mic tidak mute; diam sebelum itu tidak dihitung.
    mic_open_since: Option<u64>,
    seq: u64,
}

/// Hasil satu evaluasi.
#[derive(Debug, Default)]
pub struct Tick {
    pub changed: bool,
    /// Alert yang baru (di)tampilkan pada evaluasi ini.
    pub raised: Vec<Alert>,
}

impl Engine {
    /// Alert yang sedang tampil, paling penting dan paling baru dulu.
    pub fn active(&self) -> Vec<Alert> {
        let mut list: Vec<Alert> = self
            .rules
            .values()
            .filter(|r| r.visible)
            .filter_map(|r| r.alert.clone())
            .chain(self.events.iter().map(|e| e.alert.clone()))
            .collect();
        list.sort_by(|a, b| b.level.cmp(&a.level).then(b.raised_at_ms.cmp(&a.raised_at_ms)));
        list
    }

    /// Menutup alert. Alert berbasis kondisi tidak muncul lagi sampai kondisinya pulih.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if let Some(rule) = self.rules.get_mut(id).filter(|r| r.visible) {
            rule.visible = false;
            rule.dismissed = true;
            return true;
        }
        let before = self.events.len();
        self.events.retain(|e| e.alert.id != id);
        before != self.events.len()
    }

    /// Contoh alert untuk mengatur posisi widget dan melihat tampilannya.
    pub fn raise_test(&mut self, t: u64) -> Vec<Alert> {
        let samples = [
            (Level::Critical, "Contoh alert kritis", "Mis. stream putus atau frame drop parah."),
            (Level::Warning, "Contoh peringatan", "Mis. bitrate turun atau CPU OBS tinggi."),
            (Level::Reminder, "Contoh pengingat", "Mis. mic masih mute saat live."),
        ];
        for (level, title, detail) in samples {
            self.raise_event(&format!("test-{level:?}"), Content::new(level, title, detail), t, Some(10_000));
        }
        self.active().into_iter().filter(|a| a.id.starts_with("test-")).collect()
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    fn raise_event(&mut self, id: &str, c: Content, t: u64, ttl_ms: Option<u64>) {
        self.events.retain(|e| e.alert.id != id);
        let seq = self.next_seq();
        self.events.push(EventAlert {
            alert: Alert { id: id.into(), level: c.level, title: c.title, detail: c.detail, raised_at_ms: t, seq },
            expires_at: ttl_ms.map(|ttl| t + ttl),
        });
    }

    /// Info singkat dari servis lain (mis. "Stream selesai"), hilang sendiri.
    pub fn raise_info(&mut self, id: &str, title: String, detail: Option<String>, t: u64, ttl_ms: u64) {
        self.raise_event(id, Content { level: Level::Info, title, detail }, t, Some(ttl_ms));
    }

    fn clear_event(&mut self, id: &str) {
        self.events.retain(|e| e.alert.id != id);
    }

    fn drive(&mut self, spec: Spec, t: u64, content: Option<Content>) {
        let Engine { rules, seq, .. } = self;
        let st = rules.entry(spec.id).or_default();

        let Some(c) = content else {
            st.pending_since = None;
            if st.shown_at.is_some() || st.dismissed {
                let since = *st.false_since.get_or_insert(t);
                if t.saturating_sub(since) >= spec.clear_ms {
                    *st = RuleState::default();
                }
            }
            return;
        };

        st.false_since = None;
        let since = *st.pending_since.get_or_insert(t);
        if t.saturating_sub(since) < spec.debounce_ms {
            return;
        }

        let escalated = st.shown_level.is_some_and(|l| c.level > l);
        if escalated {
            // Naik level (mis. peringatan → kritis) selalu ditampilkan, walau sempat ditutup.
            st.dismissed = false;
        }
        let due = match st.shown_at {
            None => true,
            Some(at) => escalated || (!st.visible && spec.repeat_ms.is_some_and(|r| t.saturating_sub(at) >= r)),
        };
        if due && !st.dismissed {
            *seq += 1;
            st.visible = true;
            st.shown_at = Some(t);
            st.alert = Some(Alert {
                id: spec.id.into(),
                level: c.level,
                title: c.title.clone(),
                detail: c.detail.clone(),
                raised_at_ms: t,
                seq: *seq,
            });
        }
        if st.visible {
            st.shown_level = Some(c.level);
            if let Some(a) = st.alert.as_mut() {
                a.level = c.level;
                a.title = c.title;
                a.detail = c.detail;
            }
            let shown_for = t.saturating_sub(st.shown_at.unwrap_or(t));
            if c.level != Level::Critical && shown_for >= spec.show_ms {
                st.visible = false;
            }
        }
    }

    fn detect_stop(&mut self, s: &Snapshot, was_live: bool) {
        if !s.obs_connected {
            self.pending_stop = None;
            return;
        }
        if s.live {
            self.pending_stop = None;
            self.clear_event("streamDropped");
            return;
        }
        if was_live {
            self.pending_stop = Some(s.t_ms);
        }
        let Some(detected) = self.pending_stop else { return };

        let by_app = s.app_stop_at_ms.is_some_and(|at| at + APP_STOP_WINDOW_MS >= detected);
        let stop = s.last_stop.filter(|st| st.at_ms + STOP_MATCH_MS >= detected);
        let unexpected = Content::new(
            Level::Critical,
            "Stream berhenti mendadak!",
            "OBS menghentikan stream karena error atau koneksi putus. Cek internet, lalu mulai ulang stream.",
        );
        match stop {
            _ if by_app => self.pending_stop = None,
            Some(StreamStop { graceful: true, .. }) => {
                self.pending_stop = None;
                let info = Content { level: Level::Info, title: "Stream dihentikan dari OBS.".into(), detail: None };
                self.raise_event("streamStopped", info, s.t_ms, Some(8_000));
            }
            Some(_) => {
                self.pending_stop = None;
                self.raise_event("streamDropped", unexpected, s.t_ms, None);
            }
            None if s.t_ms >= detected + STOP_EVENT_WAIT_MS => {
                self.pending_stop = None;
                self.raise_event("streamDropped", unexpected, s.t_ms, None);
            }
            None => {}
        }
    }

    pub fn tick(&mut self, s: &Snapshot, cfg: &AlertSettings) -> Tick {
        let before = self.active();
        let t = s.t_ms;

        self.events.retain(|e| e.expires_at.is_none_or(|at| t < at));

        if !cfg.enabled {
            // Contoh alert tetap boleh tampil agar posisi widget bisa diatur.
            self.events.retain(|e| e.alert.id.starts_with("test-"));
            self.rules.clear();
            self.pending_stop = None;
            self.baseline = Baseline::default();
            self.mic_open_since = None;
            self.prev = Some(s.clone());
            return self.finish(before);
        }

        let prev = self.prev.take();
        let was_live = prev.as_ref().is_some_and(|p| p.obs_connected && p.live);
        let was_reconnecting = prev.as_ref().is_some_and(|p| p.obs_connected && p.live && p.reconnecting);
        let live = s.obs_connected && s.live;

        // OBS putus saat live: kondisi stream tidak diketahui.
        if was_live && !s.obs_connected {
            let c = Content::new(
                Level::Critical,
                "Koneksi ke OBS putus saat live!",
                "Status stream tidak diketahui. Pastikan OBS masih berjalan.",
            );
            self.raise_event("obsLost", c, t, None);
        }
        if s.obs_connected {
            self.clear_event("obsLost");
        }

        self.detect_stop(s, was_live);

        let reconnecting = live && s.reconnecting;
        self.drive(
            RECONNECTING,
            t,
            reconnecting.then(|| {
                Content::new(
                    Level::Critical,
                    "Stream terputus, OBS menyambung ulang…",
                    "Penonton mungkin melihat layar beku. Cek koneksi internet.",
                )
            }),
        );
        if was_reconnecting && live && !s.reconnecting {
            let c = Content { level: Level::Info, title: "Stream tersambung kembali.".into(), detail: None };
            self.raise_event("streamRecovered", c, t, Some(6_000));
        }

        let dropped = s.dropped_pct.filter(|_| live).and_then(|pct| {
            let level = if pct >= cfg.dropped_critical_pct {
                Level::Critical
            } else if pct >= cfg.dropped_warn_pct {
                Level::Warning
            } else {
                return None;
            };
            Some(Content::new(
                level,
                format!("Frame di-drop {pct:.1}%"),
                "Koneksi internet tidak stabil atau bitrate terlalu tinggi untuk upload Anda.",
            ))
        });
        self.drive(DROPPED, t, dropped);

        self.baseline.update(t, live, s.bitrate_kbps);
        let low_bitrate = match (self.baseline.value, s.bitrate_kbps) {
            (Some(base), Some(now)) if live && cfg.low_bitrate_pct > 0 => {
                (now < base * f64::from(cfg.low_bitrate_pct) / 100.0).then(|| Content {
                    level: Level::Warning,
                    title: format!("Bitrate turun ke {}", mbps(now)),
                    detail: Some(format!("Normalnya sekitar {}. Upload internet mungkin sedang turun.", mbps(base))),
                })
            }
            _ => None,
        };
        self.drive(LOW_BITRATE, t, low_bitrate);

        let congestion = (live && s.congestion >= CONGESTION_THRESHOLD).then(|| {
            Content::new(
                Level::Warning,
                format!("Jaringan padat ({:.0}%)", s.congestion * 100.0),
                "Data stream tertahan di jaringan. Turunkan bitrate jika berlanjut.",
            )
        });
        self.drive(CONGESTION, t, congestion);

        let cpu = s
            .cpu_pct
            .filter(|cpu| s.obs_connected && cfg.cpu_pct > 0 && *cpu >= f64::from(cfg.cpu_pct))
            .map(|cpu| {
                Content::new(
                    Level::Warning,
                    format!("CPU OBS {cpu:.0}%"),
                    "Turunkan preset encoder atau tutup aplikasi lain agar stream tidak patah-patah.",
                )
            });
        self.drive(CPU, t, cpu);

        let render = s.render_lag_pct.filter(|p| live && *p >= LAG_THRESHOLD_PCT).map(|p| {
            Content::new(
                Level::Warning,
                format!("Frame terlewat saat render ({p:.1}%)"),
                "GPU kewalahan. Batasi FPS game atau turunkan pengaturan grafis.",
            )
        });
        self.drive(RENDER_LAG, t, render);

        let encode = s.encode_lag_pct.filter(|p| live && *p >= LAG_THRESHOLD_PCT).map(|p| {
            Content::new(
                Level::Warning,
                format!("Encoder kewalahan ({p:.1}%)"),
                "Turunkan resolusi/FPS output atau pakai encoder hardware (NVENC/AMF/QSV).",
            )
        });
        self.drive(ENCODE_LAG, t, encode);

        let mic = (live && cfg.mic_reminder && s.mic_muted == Some(true)).then(|| {
            Content::new(Level::Reminder, "Mic masih mute!", "Penonton tidak bisa mendengar suara Anda.")
        });
        self.drive(MIC_MUTED, t, mic);
        self.mic_levels(s, cfg, live);

        let (chat_problem, chat_not_live) = match &s.chat {
            ChatHealth::Problem(msg) if live => (
                Some(Content {
                    level: Level::Reminder,
                    title: "Chat YouTube tidak tersambung".into(),
                    detail: Some(msg.clone()),
                }),
                None,
            ),
            ChatHealth::Waiting if live => (
                None,
                Some(Content::new(
                    Level::Reminder,
                    "OBS sudah live, tapi live di YouTube belum terdeteksi",
                    "Cek stream key. Untuk live unlisted, isi URL video di dashboard → Koneksi.",
                )),
            ),
            _ => (None, None),
        };
        self.drive(CHAT_PROBLEM, t, chat_problem);
        self.drive(CHAT_NOT_LIVE, t, chat_not_live);

        let trakteer = match &s.trakteer {
            TrakteerHealth::Problem(msg) if live => Some(Content {
                level: Level::Reminder,
                title: "Koneksi Trakteer terputus".into(),
                detail: Some(format!("Alert donasi tidak akan muncul. {msg}")),
            }),
            _ => None,
        };
        self.drive(TRAKTEER_LOST, t, trakteer);

        self.prev = Some(s.clone());
        self.finish(before)
    }

    /// "Mic tidak terdengar" dan "Suara mic pecah", dari level mic OBS.
    fn mic_levels(&mut self, s: &Snapshot, cfg: &AlertSettings, live: bool) {
        let t = s.t_ms;
        let open = live && s.mic_muted == Some(false);
        if !open {
            self.mic_open_since = None;
        }
        let open_since = open.then(|| *self.mic_open_since.get_or_insert(t));
        let audio = s.mic_audio.filter(|a| a.known);
        let name = s.mic_name.as_deref().unwrap_or("mic");

        let silent = match (open_since, audio) {
            (Some(since), Some(a)) if cfg.mic_silence_s > 0 => {
                let silent_ms = a.silent_for_ms.min(t.saturating_sub(since));
                (silent_ms >= u64::from(cfg.mic_silence_s) * 1000).then(|| {
                    Content::new(
                        Level::Warning,
                        "Mic tidak terdengar",
                        &format!(
                            "{} tanpa suara dari \"{name}\". Cek device mic di OBS. Abaikan jika Anda \
                             memang sedang diam.",
                            duration_text(cfg.mic_silence_s)
                        ),
                    )
                })
            }
            _ => None,
        };
        self.drive(MIC_SILENT, t, silent);

        let clipping = match (open_since, audio) {
            (Some(_), Some(a)) if cfg.mic_clipping && a.clip_seconds >= CLIP_MIN_SECONDS => Some(Content::new(
                Level::Warning,
                "Suara mic pecah (terlalu keras)",
                "Turunkan gain mic atau tambahkan filter Limiter di OBS.",
            )),
            _ => None,
        };
        self.drive(MIC_CLIPPING, t, clipping);
    }

    fn finish(&self, before: Vec<Alert>) -> Tick {
        let after = self.active();
        let raised = after
            .iter()
            .filter(|a| !before.iter().any(|b| b.id == a.id && b.seq == a.seq))
            .cloned()
            .collect();
        Tick { changed: after != before, raised }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: u64 = 1_000;

    fn cfg() -> AlertSettings {
        AlertSettings::default()
    }

    fn live(t: u64) -> Snapshot {
        Snapshot {
            t_ms: t,
            obs_connected: true,
            live: true,
            bitrate_kbps: Some(6000.0),
            dropped_pct: Some(0.0),
            cpu_pct: Some(10.0),
            render_lag_pct: Some(0.0),
            encode_lag_pct: Some(0.0),
            mic_muted: Some(false),
            ..Snapshot::default()
        }
    }

    fn ids(e: &Engine) -> Vec<String> {
        e.active().into_iter().map(|a| a.id).collect()
    }

    /// Menjalankan `f(t)` tiap detik dari `from` sampai `to` (eksklusif).
    fn run(e: &mut Engine, from: u64, to: u64, f: impl Fn(u64) -> Snapshot) -> Vec<Alert> {
        let mut raised = Vec::new();
        for s in (from..to).step_by(SEC as usize) {
            raised.extend(e.tick(&f(s), &cfg()).raised);
        }
        raised
    }

    #[test]
    fn healthy_stream_has_no_alerts() {
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 300 * SEC, live);
        assert!(raised.is_empty(), "{raised:?}");
    }

    #[test]
    fn dropped_frames_warn_then_escalate_then_clear() {
        let mut e = Engine::default();
        let at = |pct: f64| move |t| Snapshot { dropped_pct: Some(pct), ..live(t) };

        // Debounce 3 detik.
        run(&mut e, 0, 3 * SEC, at(2.0));
        assert!(ids(&e).is_empty());
        let raised = run(&mut e, 3 * SEC, 4 * SEC, at(2.0));
        assert_eq!(raised[0].level, Level::Warning);
        assert_eq!(raised[0].title, "Frame di-drop 2.0%");

        // Peringatan hilang sendiri setelah 10 detik.
        run(&mut e, 4 * SEC, 14 * SEC, at(2.0));
        assert!(ids(&e).is_empty());

        // Naik ke kritis: langsung tampil dan menetap.
        let raised = run(&mut e, 14 * SEC, 15 * SEC, at(7.0));
        assert_eq!(raised[0].level, Level::Critical);
        run(&mut e, 15 * SEC, 60 * SEC, at(7.0));
        assert_eq!(ids(&e), vec!["droppedFrames"]);

        // Pulih: hilang setelah histeresis 5 detik.
        run(&mut e, 60 * SEC, 64 * SEC, at(0.0));
        assert_eq!(ids(&e), vec!["droppedFrames"]);
        run(&mut e, 64 * SEC, 66 * SEC, at(0.0));
        assert!(ids(&e).is_empty());
    }

    #[test]
    fn warning_repeats_after_cooldown() {
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 125 * SEC, |t| Snapshot { dropped_pct: Some(2.0), ..live(t) });
        // Muncul di detik 3, 63, dan 123.
        let times: Vec<u64> = raised.iter().map(|a| a.raised_at_ms / SEC).collect();
        assert_eq!(times, vec![3, 63, 123]);
    }

    #[test]
    fn short_spike_is_ignored() {
        let mut e = Engine::default();
        run(&mut e, 0, 2 * SEC, |t| Snapshot { dropped_pct: Some(10.0), ..live(t) });
        let raised = run(&mut e, 2 * SEC, 20 * SEC, live);
        assert!(raised.is_empty() && ids(&e).is_empty());
    }

    #[test]
    fn reconnecting_is_critical_until_recovered() {
        let mut e = Engine::default();
        run(&mut e, 0, 5 * SEC, live);
        let raised = run(&mut e, 5 * SEC, 6 * SEC, |t| Snapshot { reconnecting: true, ..live(t) });
        assert_eq!(raised[0].id, "reconnecting");
        assert_eq!(raised[0].level, Level::Critical);
        run(&mut e, 6 * SEC, 40 * SEC, |t| Snapshot { reconnecting: true, ..live(t) });
        assert_eq!(ids(&e), vec!["reconnecting"]);

        let raised = run(&mut e, 40 * SEC, 41 * SEC, live);
        assert_eq!(ids(&e), vec!["streamRecovered"]);
        assert_eq!(raised[0].level, Level::Info);
        run(&mut e, 41 * SEC, 47 * SEC, live);
        assert!(ids(&e).is_empty());
    }

    fn stopped(t: u64, stop: Option<StreamStop>, app_stop: Option<u64>) -> Snapshot {
        Snapshot {
            t_ms: t,
            obs_connected: true,
            live: false,
            last_stop: stop,
            app_stop_at_ms: app_stop,
            ..Snapshot::default()
        }
    }

    #[test]
    fn unexpected_stop_is_sticky_until_live_again() {
        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        let stop = Some(StreamStop { at_ms: 9_500, graceful: false });
        let raised = run(&mut e, 10 * SEC, 11 * SEC, |t| stopped(t, stop, None));
        assert_eq!(raised[0].id, "streamDropped");
        run(&mut e, 11 * SEC, 300 * SEC, |t| stopped(t, stop, None));
        assert_eq!(ids(&e), vec!["streamDropped"]);
        run(&mut e, 300 * SEC, 301 * SEC, live);
        assert!(ids(&e).is_empty());
    }

    #[test]
    fn graceful_stop_from_obs_is_short_info() {
        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        let stop = Some(StreamStop { at_ms: 9_800, graceful: true });
        let raised = run(&mut e, 10 * SEC, 11 * SEC, |t| stopped(t, stop, None));
        assert_eq!(raised[0].id, "streamStopped");
        assert_eq!(raised[0].level, Level::Info);
        run(&mut e, 11 * SEC, 18 * SEC, |t| stopped(t, stop, None));
        assert_eq!(ids(&e), vec!["streamStopped"], "masih dalam 8 detik");
        run(&mut e, 18 * SEC, 19 * SEC, |t| stopped(t, stop, None));
        assert!(ids(&e).is_empty());
    }

    #[test]
    fn stop_from_app_is_silent() {
        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        let stop = Some(StreamStop { at_ms: 9_800, graceful: true });
        let raised = run(&mut e, 10 * SEC, 20 * SEC, |t| stopped(t, stop, Some(9_000)));
        assert!(raised.is_empty(), "{raised:?}");
    }

    #[test]
    fn stop_without_event_becomes_critical_after_wait() {
        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        run(&mut e, 10 * SEC, 12 * SEC, |t| stopped(t, None, None));
        assert!(ids(&e).is_empty(), "menunggu event STOPPED");
        // Event datang terlambat tapi masih dalam jendela: tetap terbaca graceful.
        let late = Some(StreamStop { at_ms: 11_500, graceful: true });
        run(&mut e, 12 * SEC, 13 * SEC, |t| stopped(t, late, None));
        assert_eq!(ids(&e), vec!["streamStopped"]);

        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        let raised = run(&mut e, 10 * SEC, 14 * SEC, |t| stopped(t, None, None));
        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].id, "streamDropped");
    }

    #[test]
    fn obs_lost_while_live() {
        let mut e = Engine::default();
        run(&mut e, 0, 10 * SEC, live);
        let raised = run(&mut e, 10 * SEC, 30 * SEC, |t| Snapshot { t_ms: t, ..Snapshot::default() });
        assert_eq!(raised.len(), 1, "tidak ikut dianggap stream putus: {raised:?}");
        assert_eq!(raised[0].id, "obsLost");
        // OBS tersambung lagi (stream masih live) → alert hilang.
        run(&mut e, 30 * SEC, 31 * SEC, live);
        assert!(ids(&e).is_empty());
    }

    #[test]
    fn low_bitrate_against_baseline() {
        let mut e = Engine::default();
        run(&mut e, 0, 80 * SEC, live);
        let low = |t| Snapshot { bitrate_kbps: Some(2500.0), ..live(t) };
        run(&mut e, 80 * SEC, 90 * SEC, low);
        assert!(ids(&e).is_empty(), "debounce 10 detik");
        let raised = run(&mut e, 90 * SEC, 91 * SEC, low);
        assert_eq!(raised[0].title, "Bitrate turun ke 2.5 Mbps");
        assert_eq!(raised[0].detail.as_deref(), Some("Normalnya sekitar 6.0 Mbps. Upload internet mungkin sedang turun."));
    }

    #[test]
    fn no_bitrate_alert_before_baseline() {
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 60 * SEC, |t| Snapshot { bitrate_kbps: Some(500.0), ..live(t) });
        assert!(raised.is_empty());
    }

    #[test]
    fn mic_reminder_repeats_every_two_minutes() {
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 260 * SEC, |t| Snapshot { mic_muted: Some(true), ..live(t) });
        let times: Vec<u64> = raised.iter().map(|a| a.raised_at_ms / SEC).collect();
        assert_eq!(times, vec![10, 130, 250]);
        assert!(raised.iter().all(|a| a.level == Level::Reminder));

        // Mic tidak diingatkan saat tidak live.
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 60 * SEC, |t| Snapshot { live: false, mic_muted: Some(true), ..live(t) });
        assert!(raised.is_empty());
    }

    fn mic(silent_for_s: u64, clip_seconds: u32) -> Option<MicAudio> {
        Some(MicAudio { known: true, silent_for_ms: silent_for_s * SEC, clip_seconds, peak_db: None })
    }

    #[test]
    fn mic_silence_after_two_minutes_of_live() {
        let mut e = Engine::default();
        // Mic sudah diam lama sebelum live: hitungan mulai saat live (t = 0).
        let silent = |t: u64| Snapshot { mic_audio: mic(600 + t / SEC, 0), mic_name: Some("Mic/Aux".into()), ..live(t) };
        let raised = run(&mut e, 0, 121 * SEC, silent);
        let times: Vec<u64> = raised.iter().map(|a| a.raised_at_ms / SEC).collect();
        assert_eq!(times, vec![120]);
        assert_eq!(raised[0].id, "micSilent");
        assert_eq!(raised[0].level, Level::Warning);
        assert!(raised[0].detail.as_deref().unwrap().contains("2 menit tanpa suara dari \"Mic/Aux\""));

        // Tetap diam: diulang tiap 5 menit.
        let raised = run(&mut e, 121 * SEC, 421 * SEC, silent);
        assert_eq!(raised.iter().map(|a| a.raised_at_ms / SEC).collect::<Vec<_>>(), vec![420]);

        // Ada suara: hilang, dan butuh 2 menit diam lagi.
        e.tick(&Snapshot { mic_audio: mic(0, 0), ..live(421 * SEC) }, &cfg());
        assert!(!ids(&e).contains(&"micSilent".to_string()));
        let raised = run(&mut e, 422 * SEC, 541 * SEC, |t| Snapshot { mic_audio: mic((t - 421 * SEC) / SEC, 0), ..live(t) });
        assert!(raised.is_empty(), "{raised:?}");
        let raised = run(&mut e, 541 * SEC, 542 * SEC, |t| Snapshot { mic_audio: mic((t - 421 * SEC) / SEC, 0), ..live(t) });
        assert_eq!(raised.len(), 1);
    }

    #[test]
    fn mic_silence_needs_live_unmuted_known_and_enabled() {
        type Case = (&'static str, Box<dyn Fn(u64) -> Snapshot>, AlertSettings);
        let cases: [Case; 5] = [
            ("offline", Box::new(|t| Snapshot { live: false, mic_audio: mic(999, 0), ..live(t) }), cfg()),
            ("mute", Box::new(|t| Snapshot { mic_muted: Some(true), mic_audio: mic(999, 0), ..live(t) }), cfg()),
            (
                "tidak diketahui",
                Box::new(|t| Snapshot {
                    mic_audio: Some(MicAudio { known: false, ..mic(999, 0).unwrap() }),
                    ..live(t)
                }),
                cfg(),
            ),
            ("mati", Box::new(|t| Snapshot { mic_audio: mic(999, 0), ..live(t) }), AlertSettings { mic_silence_s: 0, ..cfg() }),
            ("alert dimatikan", Box::new(|t| Snapshot { mic_audio: mic(999, 0), ..live(t) }), AlertSettings { enabled: false, ..cfg() }),
        ];
        for (name, f, settings) in cases {
            let mut e = Engine::default();
            let mut raised = Vec::new();
            for t in 0..300 {
                raised.extend(e.tick(&f(t * SEC), &settings).raised);
            }
            assert!(raised.iter().all(|a| a.id != "micSilent"), "{name}: {raised:?}");
        }

        // Baru di-unmute: diam sebelum unmute tidak dihitung.
        let mut e = Engine::default();
        let raised = run(&mut e, 0, 200 * SEC, |t| Snapshot {
            mic_muted: Some(t < 100 * SEC),
            mic_audio: mic(999, 0),
            ..live(t)
        });
        let silent: Vec<u64> = raised.iter().filter(|a| a.id == "micSilent").map(|a| a.raised_at_ms / SEC).collect();
        assert!(silent.is_empty(), "{silent:?}");
        let raised = run(&mut e, 200 * SEC, 221 * SEC, |t| Snapshot { mic_audio: mic(999, 0), ..live(t) });
        assert_eq!(raised.iter().map(|a| a.raised_at_ms / SEC).collect::<Vec<_>>(), vec![220]);

        // Ambang mengikuti pengaturan.
        let mut e = Engine::default();
        let fast = AlertSettings { mic_silence_s: 30, ..cfg() };
        let mut raised = Vec::new();
        for t in 0..31 {
            raised.extend(e.tick(&Snapshot { mic_audio: mic(999, 0), ..live(t * SEC) }, &fast).raised);
        }
        assert_eq!(raised.len(), 1);
        assert!(raised[0].detail.as_deref().unwrap().starts_with("30 detik"));
    }

    #[test]
    fn mic_clipping_warns_and_repeats() {
        let mut e = Engine::default();
        let clip = |n: u32| move |t| Snapshot { mic_audio: mic(0, n), ..live(t) };
        assert!(run(&mut e, 0, 10 * SEC, clip(2)).is_empty(), "2 detik pecah belum cukup");
        let raised = run(&mut e, 10 * SEC, 140 * SEC, clip(3));
        assert_eq!(raised.iter().map(|a| a.raised_at_ms / SEC).collect::<Vec<_>>(), vec![10, 130]);
        assert_eq!(raised[0].id, "micClipping");

        let mut e = Engine::default();
        let off = AlertSettings { mic_clipping: false, ..cfg() };
        for t in 0..60 {
            assert!(e.tick(&Snapshot { mic_audio: mic(0, 10), ..live(t * SEC) }, &off).raised.is_empty());
        }
    }

    #[test]
    fn chat_reminders_only_while_live() {
        let mut e = Engine::default();
        let problem = |t| Snapshot { chat: ChatHealth::Problem("jaringan".into()), ..live(t) };
        let raised = run(&mut e, 0, 31 * SEC, problem);
        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].id, "chatProblem");
        assert_eq!(raised[0].detail.as_deref(), Some("jaringan"));

        let mut e = Engine::default();
        let raised = run(&mut e, 0, 61 * SEC, |t| Snapshot { chat: ChatHealth::Waiting, ..live(t) });
        assert_eq!(raised.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["chatNotLive"]);

        let mut e = Engine::default();
        let offline = |t| Snapshot { live: false, chat: ChatHealth::Waiting, ..live(t) };
        assert!(run(&mut e, 0, 120 * SEC, offline).is_empty());
    }

    #[test]
    fn trakteer_reminder_only_while_live() {
        let mut e = Engine::default();
        let lost = |t| Snapshot { trakteer: TrakteerHealth::Problem("putus".into()), ..live(t) };
        let raised = run(&mut e, 0, 31 * SEC, lost);
        assert_eq!(raised.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["trakteerLost"]);
        assert_eq!(raised[0].detail.as_deref(), Some("Alert donasi tidak akan muncul. putus"));

        let mut e = Engine::default();
        let offline = |t| Snapshot { live: false, trakteer: TrakteerHealth::Problem("putus".into()), ..live(t) };
        assert!(run(&mut e, 0, 120 * SEC, offline).is_empty());
    }

    #[test]
    fn dismiss_suppresses_until_condition_clears() {
        let mut e = Engine::default();
        let bad = |t| Snapshot { reconnecting: true, ..live(t) };
        run(&mut e, 0, 2 * SEC, bad);
        assert!(e.dismiss("reconnecting"));
        run(&mut e, 2 * SEC, 30 * SEC, bad);
        assert!(ids(&e).is_empty());
        // Pulih lalu kambuh: muncul lagi.
        run(&mut e, 30 * SEC, 31 * SEC, live);
        e.dismiss("streamRecovered");
        run(&mut e, 31 * SEC, 32 * SEC, bad);
        assert_eq!(ids(&e), vec!["reconnecting"]);
        assert!(!e.dismiss("tidak-ada"));
    }

    #[test]
    fn disabled_engine_is_silent_but_allows_test_alerts() {
        let mut e = Engine::default();
        let off = AlertSettings { enabled: false, ..cfg() };
        for t in 0..30 {
            e.tick(&Snapshot { reconnecting: true, dropped_pct: Some(20.0), ..live(t * SEC) }, &off);
        }
        assert!(e.active().is_empty());
        e.raise_test(30 * SEC);
        e.tick(&live(31 * SEC), &off);
        assert_eq!(e.active().len(), 3);
        e.tick(&live(41 * SEC), &off);
        assert!(e.active().is_empty(), "contoh hilang setelah 10 detik");
    }

    #[test]
    fn thresholds_follow_settings() {
        let mut e = Engine::default();
        let strict = AlertSettings { cpu_pct: 50, ..cfg() };
        let mut raised = Vec::new();
        for t in 0..12 {
            raised.extend(e.tick(&Snapshot { cpu_pct: Some(60.0), ..live(t * SEC) }, &strict).raised);
        }
        assert_eq!(raised.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["cpuHigh"]);

        let mut e = Engine::default();
        let off = AlertSettings { cpu_pct: 0, low_bitrate_pct: 0, mic_reminder: false, ..cfg() };
        let mut raised = Vec::new();
        for t in 0..200 {
            let s = Snapshot {
                cpu_pct: Some(100.0),
                mic_muted: Some(true),
                bitrate_kbps: Some(if t < 100 { 6000.0 } else { 100.0 }),
                ..live(t * SEC)
            };
            raised.extend(e.tick(&s, &off).raised);
        }
        assert!(raised.is_empty(), "{raised:?}");
    }
}
