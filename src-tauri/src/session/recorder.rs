//! Perekam sesi stream (fungsi murni): mengumpulkan angka selama live dan menghasilkan
//! [`SessionSummary`] saat stream selesai.
//!
//! Siklus:
//! - sesi dimulai saat OBS live;
//! - stop yang diminta (didahului `STOPPING`) langsung menutup sesi;
//! - stream yang putus ditahan 3 menit: jika live lagi, sesi dilanjutkan (`drops` + 1),
//!   jika tidak, sesi ditutup dengan alasan `lost`.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::alerts::engine::{Alert, Level};
use crate::obs::StreamStop;
use crate::youtube::parse::{ChatMessage, MessageKind};

/// Stream putus yang tidak live lagi dalam waktu ini dianggap selesai.
pub const RESUME_WINDOW_MS: u64 = 180_000;
/// Menunggu event STOPPED setelah polling melihat stream mati (sama dengan mesin alert).
const STOP_EVENT_WAIT_MS: u64 = 3_000;
const STOP_MATCH_MS: u64 = 5_000;
const MINUTE_MS: u64 = 60_000;
/// Grafik per menit dibatasi 12 jam.
const MAX_TIMELINE: usize = 720;
const MAX_ALERTS: usize = 50;
const MAX_PAID: usize = 20;
const MAX_TOP_DONORS: usize = 3;
const MAX_CHATTERS: usize = 20_000;
const SEEN_IDS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    /// Dihentikan dari OBS atau aplikasi ini.
    Stopped,
    /// Stream putus dan tidak live lagi dalam 3 menit.
    Lost,
    /// Aplikasi ditutup (atau crash) saat sesi berjalan.
    AppClosed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewerStats {
    pub peak: Option<u64>,
    pub average: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaidItem {
    pub name: String,
    pub amount: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatStats {
    pub messages: u32,
    pub chatters: u32,
    pub super_chat_count: u32,
    /// Maks. 20 pertama.
    pub super_chats: Vec<PaidItem>,
    pub memberships: u32,
    pub gifts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Donor {
    pub name: String,
    pub total_rp: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DonationStats {
    pub count: u32,
    pub total_rp: u64,
    pub top: Vec<Donor>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamStats {
    pub avg_bitrate_kbps: Option<f64>,
    pub dropped_frames: u32,
    pub total_frames: u32,
    pub dropped_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinuteSample {
    pub minute: u32,
    pub viewers: Option<u64>,
    pub bitrate_kbps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertMark {
    /// Detik sejak sesi dimulai.
    pub at_s: u64,
    pub level: Level,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    /// Unix ms.
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub duration_ms: u64,
    /// `None` = masih berjalan.
    pub end_reason: Option<EndReason>,
    /// Berapa kali stream putus lalu dimulai ulang (koneksi OBS yang sempat putus
    /// sementara stream tetap berjalan tidak dihitung).
    pub drops: u32,
    /// Berapa kali OBS menyambung ulang (reconnecting) di tengah live.
    pub reconnects: u32,
    #[serde(default)]
    pub viewers: ViewerStats,
    #[serde(default)]
    pub chat: ChatStats,
    #[serde(default)]
    pub trakteer: DonationStats,
    #[serde(default)]
    pub stream: StreamStats,
    #[serde(default)]
    pub timeline: Vec<MinuteSample>,
    #[serde(default)]
    pub alerts: Vec<AlertMark>,
}

/// Satu pembacaan per detik.
#[derive(Debug, Clone, Default)]
pub struct Tick {
    /// Jam monoton (sama dengan servis OBS dan mesin alert).
    pub t_ms: u64,
    /// Unix ms.
    pub wall_ms: u64,
    pub obs_connected: bool,
    pub live: bool,
    pub reconnecting: bool,
    pub duration_ms: u64,
    pub bitrate_kbps: Option<f64>,
    /// Kumulatif segmen stream ini (di-reset OBS saat stream mati).
    pub dropped_frames: u32,
    pub total_frames: u32,
    pub last_stop: Option<StreamStop>,
    pub viewers: Option<u64>,
}

#[derive(Default)]
struct Minute {
    index: u32,
    viewers: Option<u64>,
    bitrate_sum: f64,
    bitrate_n: u32,
}

impl Minute {
    fn sample(&self) -> MinuteSample {
        MinuteSample {
            minute: self.index,
            viewers: self.viewers,
            bitrate_kbps: (self.bitrate_n > 0).then(|| self.bitrate_sum / f64::from(self.bitrate_n)),
        }
    }
}

struct Live {
    summary: SessionSummary,
    started_t: u64,
    /// Polling melihat stream mati; menunggu event STOPPED.
    stop_seen_at: Option<u64>,
    /// Stream putus (atau koneksi OBS putus); menunggu live lagi.
    lost_at: Option<u64>,
    /// Baru kembali live setelah putus; `drops` dihitung hanya jika OBS ternyata memulai
    /// stream baru (durasi/frame kembali dari nol).
    resuming: bool,
    /// Durasi & frame segmen-segmen sebelumnya (sebelum putus).
    duration_base: u64,
    frames_base: (u32, u32),
    segment: (u64, u32, u32),
    was_reconnecting: bool,
    viewer_sum: u64,
    viewer_n: u64,
    bitrate_sum: f64,
    bitrate_n: u64,
    minute: Minute,
    seen: HashSet<String>,
    seen_order: VecDeque<String>,
    chatters: HashSet<String>,
    donors: HashMap<String, (u64, u32)>,
}

impl Live {
    fn new(t: &Tick) -> Self {
        // Aplikasi bisa tersambung saat stream sudah berjalan: mulai dihitung dari durasi OBS.
        let started_at = t.wall_ms.saturating_sub(t.duration_ms);
        Self {
            summary: SessionSummary {
                id: format!("s{started_at}"),
                started_at,
                ended_at: None,
                duration_ms: 0,
                end_reason: None,
                drops: 0,
                reconnects: 0,
                viewers: ViewerStats::default(),
                chat: ChatStats::default(),
                trakteer: DonationStats::default(),
                stream: StreamStats::default(),
                timeline: Vec::new(),
                alerts: Vec::new(),
            },
            started_t: t.t_ms,
            stop_seen_at: None,
            lost_at: None,
            resuming: false,
            duration_base: 0,
            frames_base: (0, 0),
            segment: (0, 0, 0),
            was_reconnecting: false,
            viewer_sum: 0,
            viewer_n: 0,
            bitrate_sum: 0.0,
            bitrate_n: 0,
            minute: Minute::default(),
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
            chatters: HashSet::new(),
            donors: HashMap::new(),
        }
    }

    /// Menutup segmen berjalan (stream putus) agar angkanya tidak hilang saat OBS me-reset.
    fn close_segment(&mut self) {
        let (duration, dropped, total) = self.segment;
        self.duration_base += duration;
        self.frames_base.0 += dropped;
        self.frames_base.1 += total;
        self.segment = (0, 0, 0);
    }

    fn push_minute(&mut self) {
        if self.summary.timeline.len() < MAX_TIMELINE {
            self.summary.timeline.push(self.minute.sample());
        }
    }

    fn record_live(&mut self, t: &Tick) {
        // Durasi/frame OBS turun = stream baru dimulai (stats di-reset). Jika durasi terus naik,
        // stream tidak pernah berhenti (mis. hanya koneksi ke OBS yang sempat putus).
        if t.total_frames < self.segment.2 || t.duration_ms < self.segment.0 {
            self.close_segment();
            if self.resuming {
                self.summary.drops += 1;
            }
        }
        self.resuming = false;
        self.segment = (t.duration_ms, t.dropped_frames, t.total_frames);
        self.summary.duration_ms = self.duration_base + t.duration_ms;

        if t.reconnecting && !self.was_reconnecting {
            self.summary.reconnects += 1;
        }
        self.was_reconnecting = t.reconnecting;

        let index = u32::try_from(t.t_ms.saturating_sub(self.started_t) / MINUTE_MS).unwrap_or(u32::MAX);
        if index != self.minute.index {
            self.push_minute();
            self.minute = Minute { index, ..Minute::default() };
        }
        if let Some(v) = t.viewers {
            self.viewer_sum += v;
            self.viewer_n += 1;
            self.summary.viewers.peak = Some(self.summary.viewers.peak.map_or(v, |p| p.max(v)));
            self.minute.viewers = Some(self.minute.viewers.map_or(v, |p| p.max(v)));
        }
        if let Some(b) = t.bitrate_kbps.filter(|b| b.is_finite() && *b > 0.0) {
            self.bitrate_sum += b;
            self.bitrate_n += 1;
            self.minute.bitrate_sum += b;
            self.minute.bitrate_n += 1;
        }
    }

    /// Ringkasan dengan angka terkini (dipakai juga untuk sesi yang masih berjalan).
    fn summary(&self) -> SessionSummary {
        let mut s = self.summary.clone();
        s.viewers.average = (self.viewer_n > 0).then(|| (self.viewer_sum as f64 / self.viewer_n as f64).round() as u64);
        s.chat.chatters = u32::try_from(self.chatters.len()).unwrap_or(u32::MAX);
        let dropped = self.frames_base.0 + self.segment.1;
        let total = self.frames_base.1 + self.segment.2;
        s.stream = StreamStats {
            avg_bitrate_kbps: (self.bitrate_n > 0).then(|| self.bitrate_sum / self.bitrate_n as f64),
            dropped_frames: dropped,
            total_frames: total,
            dropped_pct: (total > 0).then(|| f64::from(dropped) * 100.0 / f64::from(total)),
        };
        if s.timeline.len() < MAX_TIMELINE && (self.minute.viewers.is_some() || self.minute.bitrate_n > 0) {
            s.timeline.push(self.minute.sample());
        }
        let mut top: Vec<Donor> = self
            .donors
            .iter()
            .map(|(name, (total_rp, count))| Donor { name: name.clone(), total_rp: *total_rp, count: *count })
            .collect();
        top.sort_by(|a, b| b.total_rp.cmp(&a.total_rp).then(a.name.cmp(&b.name)));
        top.truncate(MAX_TOP_DONORS);
        s.trakteer.top = top;
        s
    }

    fn mark_seen(&mut self, id: &str) -> bool {
        if !self.seen.insert(id.to_string()) {
            return false;
        }
        self.seen_order.push_back(id.to_string());
        if self.seen_order.len() > SEEN_IDS {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }
}

#[derive(Default)]
pub struct Recorder {
    live: Option<Live>,
}

impl Recorder {
    pub fn is_active(&self) -> bool {
        self.live.is_some()
    }

    /// Ringkasan sementara sesi yang sedang berjalan.
    pub fn current(&self) -> Option<SessionSummary> {
        self.live.as_ref().map(Live::summary)
    }

    /// Dipanggil tiap detik. Mengembalikan ringkasan saat sesi selesai.
    pub fn on_tick(&mut self, t: &Tick) -> Option<SessionSummary> {
        let live = t.obs_connected && t.live;
        if live {
            let session = self.live.get_or_insert_with(|| Live::new(t));
            if session.lost_at.take().is_some() {
                session.resuming = true;
            }
            session.stop_seen_at = None;
            session.record_live(t);
            return None;
        }

        let session = self.live.as_mut()?;
        if let Some(lost) = session.lost_at {
            if t.t_ms.saturating_sub(lost) >= RESUME_WINDOW_MS {
                return self.finish(t.wall_ms, EndReason::Lost);
            }
            return None;
        }

        let seen = *session.stop_seen_at.get_or_insert(t.t_ms);
        let stop = t.last_stop.filter(|s| s.at_ms + STOP_MATCH_MS >= seen);
        match stop {
            Some(StreamStop { graceful: true, .. }) if t.obs_connected => self.finish(t.wall_ms, EndReason::Stopped),
            // Segmen tidak ditutup di sini: `record_live` menutupnya jika OBS ternyata memulai
            // stream baru, sehingga stream yang tetap berjalan tidak dihitung dua kali.
            Some(_) => {
                session.lost_at = Some(t.t_ms);
                None
            }
            // OBS putus, atau STOPPED tidak kunjung datang: anggap putus, tunggu live lagi.
            None if !t.obs_connected || t.t_ms >= seen + STOP_EVENT_WAIT_MS => {
                session.lost_at = Some(t.t_ms);
                None
            }
            None => None,
        }
    }

    pub fn finish(&mut self, wall_ms: u64, reason: EndReason) -> Option<SessionSummary> {
        let session = self.live.take()?;
        let mut s = session.summary();
        s.ended_at = Some(wall_ms);
        s.end_reason = Some(reason);
        Some(s)
    }

    /// Pesan chat dari `chat://update`; ID yang sudah pernah dihitung diabaikan.
    pub fn on_chat(&mut self, messages: &[ChatMessage]) {
        let Some(session) = self.live.as_mut().filter(|s| s.lost_at.is_none()) else { return };
        let started_at = i64::try_from(session.summary.started_at).unwrap_or(i64::MAX);
        for m in messages {
            // Riwayat chat yang diputar ulang saat tersambung bisa berisi pesan dari sebelum live.
            let before_live = m.timestamp_ms > 0 && m.timestamp_ms < started_at;
            if before_live || !session.mark_seen(&m.id) {
                continue;
            }
            let chat = &mut session.summary.chat;
            match m.kind {
                MessageKind::Text | MessageKind::SuperChat | MessageKind::SuperSticker => {
                    chat.messages += 1;
                    if session.chatters.len() < MAX_CHATTERS {
                        session.chatters.insert(m.author.channel_id.clone());
                    }
                    if m.kind != MessageKind::Text {
                        chat.super_chat_count += 1;
                        if chat.super_chats.len() < MAX_PAID {
                            chat.super_chats.push(PaidItem {
                                name: m.author.name.clone(),
                                amount: m.amount.clone().unwrap_or_default(),
                            });
                        }
                    }
                }
                MessageKind::Membership => chat.memberships += 1,
                MessageKind::GiftPurchase => chat.gifts += 1,
                MessageKind::GiftRedemption => {}
            }
        }
    }

    /// Hanya donasi live (bukan test/contoh).
    pub fn on_donation(&mut self, supporter: &str, amount_rp: u64) {
        let Some(session) = self.live.as_mut() else { return };
        let stats = &mut session.summary.trakteer;
        stats.count += 1;
        stats.total_rp += amount_rp;
        let entry = session.donors.entry(supporter.trim().to_string()).or_default();
        entry.0 += amount_rp;
        entry.1 += 1;
    }

    /// Alert level peringatan ke atas masuk timeline.
    pub fn on_alert(&mut self, alert: &Alert) {
        let Some(session) = self.live.as_mut() else { return };
        if alert.level < Level::Warning || session.summary.alerts.len() >= MAX_ALERTS {
            return;
        }
        session.summary.alerts.push(AlertMark {
            at_s: alert.raised_at_ms.saturating_sub(session.started_t) / 1000,
            level: alert.level,
            title: alert.title.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::youtube::parse::Author;

    const SEC: u64 = 1_000;
    const WALL0: u64 = 1_800_000_000_000;

    fn tick(t: u64, live: bool) -> Tick {
        Tick {
            t_ms: t,
            wall_ms: WALL0 + t,
            obs_connected: true,
            live,
            duration_ms: if live { t } else { 0 },
            bitrate_kbps: live.then_some(6000.0),
            dropped_frames: if live { (t / SEC) as u32 } else { 0 },
            total_frames: if live { (t / SEC) as u32 * 60 } else { 0 },
            viewers: live.then_some(10 + t / (60 * SEC)),
            ..Tick::default()
        }
    }

    fn graceful(at: u64) -> Option<StreamStop> {
        Some(StreamStop { at_ms: at, graceful: true })
    }

    fn msg(id: &str, kind: MessageKind, author: &str, amount: Option<&str>) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            kind,
            author: Author { name: author.into(), channel_id: format!("UC-{author}"), ..Author::default() },
            runs: Vec::new(),
            timestamp_ms: 0,
            amount: amount.map(str::to_string),
            header: None,
            sticker: None,
            colors: None,
        }
    }

    fn alert(level: Level, title: &str, at: u64) -> Alert {
        Alert { id: title.into(), level, title: title.into(), detail: None, raised_at_ms: at, seq: 1 }
    }

    /// Live dari `from` sampai `to` (eksklusif), tiap detik.
    fn run_live(r: &mut Recorder, from: u64, to: u64) {
        for t in (from..to).step_by(SEC as usize) {
            assert!(r.on_tick(&tick(t, true)).is_none());
        }
    }

    #[test]
    fn graceful_stop_produces_summary() {
        let mut r = Recorder::default();
        assert!(r.on_tick(&tick(0, false)).is_none());
        assert!(!r.is_active(), "belum live = belum ada sesi");

        run_live(&mut r, 0, 150 * SEC);
        r.on_chat(&[
            msg("a", MessageKind::Text, "ani", None),
            msg("b", MessageKind::Text, "budi", None),
            msg("a", MessageKind::Text, "ani", None),
            msg("c", MessageKind::SuperChat, "ani", Some("Rp 50.000")),
            msg("d", MessageKind::Membership, "cici", None),
            msg("e", MessageKind::GiftPurchase, "dedi", None),
        ]);
        r.on_donation("Penonton A", 10_000);
        r.on_donation("Penonton B", 50_000);
        r.on_donation("Penonton A ", 5_000);
        r.on_alert(&alert(Level::Reminder, "Mic masih mute!", 20 * SEC));
        r.on_alert(&alert(Level::Warning, "Frame drop 2%", 90 * SEC));

        let mut stop = tick(150 * SEC, false);
        stop.last_stop = graceful(150 * SEC);
        let s = r.on_tick(&stop).expect("sesi selesai");
        assert!(!r.is_active());

        assert_eq!(s.id, format!("s{WALL0}"));
        assert_eq!(s.started_at, WALL0);
        assert_eq!(s.ended_at, Some(WALL0 + 150 * SEC));
        assert_eq!(s.end_reason, Some(EndReason::Stopped));
        assert_eq!(s.duration_ms, 149 * SEC);
        assert_eq!(s.drops, 0);
        assert_eq!(s.viewers.peak, Some(12));
        assert_eq!(s.viewers.average, Some(11));
        assert_eq!(s.chat.messages, 3);
        assert_eq!(s.chat.chatters, 2);
        assert_eq!(s.chat.super_chat_count, 1);
        assert_eq!(s.chat.super_chats, vec![PaidItem { name: "ani".into(), amount: "Rp 50.000".into() }]);
        assert_eq!((s.chat.memberships, s.chat.gifts), (1, 1));
        assert_eq!((s.trakteer.count, s.trakteer.total_rp), (3, 65_000));
        assert_eq!(
            s.trakteer.top,
            vec![
                Donor { name: "Penonton B".into(), total_rp: 50_000, count: 1 },
                Donor { name: "Penonton A".into(), total_rp: 15_000, count: 2 },
            ]
        );
        assert_eq!(s.alerts, vec![AlertMark { at_s: 90, level: Level::Warning, title: "Frame drop 2%".into() }]);
        assert_eq!(s.stream.avg_bitrate_kbps, Some(6000.0));
        assert_eq!((s.stream.dropped_frames, s.stream.total_frames), (149, 149 * 60));
        let minutes: Vec<(u32, Option<u64>)> = s.timeline.iter().map(|m| (m.minute, m.viewers)).collect();
        assert_eq!(minutes, vec![(0, Some(10)), (1, Some(11)), (2, Some(12))]);
    }

    #[test]
    fn stop_waits_for_stopped_event() {
        let mut r = Recorder::default();
        run_live(&mut r, 0, 10 * SEC);
        // Polling melihat stream mati sebelum event STOPPED tiba.
        assert!(r.on_tick(&tick(10 * SEC, false)).is_none());
        let mut later = tick(11 * SEC, false);
        later.last_stop = graceful(11 * SEC);
        assert_eq!(r.on_tick(&later).unwrap().end_reason, Some(EndReason::Stopped));
    }

    #[test]
    fn drop_then_resume_is_one_session() {
        let mut r = Recorder::default();
        run_live(&mut r, 0, 60 * SEC);
        // Stream mati karena error (tanpa STOPPING).
        let mut lost = tick(60 * SEC, false);
        lost.last_stop = Some(StreamStop { at_ms: 60 * SEC, graceful: false });
        assert!(r.on_tick(&lost).is_none());
        // Chat selama putus tidak dihitung.
        r.on_chat(&[msg("x", MessageKind::Text, "ani", None)]);
        for t in (61..120).map(|s| s * SEC) {
            assert!(r.on_tick(&tick(t, false)).is_none());
        }
        // Live lagi: OBS memulai durasi & frame dari nol.
        for t in (120..180).map(|s| s * SEC) {
            let mut k = tick(t, true);
            k.duration_ms = t - 120 * SEC;
            k.dropped_frames = 0;
            k.total_frames = ((t - 120 * SEC) / SEC) as u32 * 60;
            assert!(r.on_tick(&k).is_none());
        }
        let mut stop = tick(180 * SEC, false);
        stop.last_stop = graceful(180 * SEC);
        let s = r.on_tick(&stop).unwrap();
        assert_eq!(s.drops, 1);
        assert_eq!(s.duration_ms, 59 * SEC + 59 * SEC);
        assert_eq!(s.stream.dropped_frames, 59);
        assert_eq!(s.stream.total_frames, 59 * 60 + 59 * 60);
        assert_eq!(s.chat.messages, 0);
        assert_eq!(s.end_reason, Some(EndReason::Stopped));
    }

    #[test]
    fn drop_without_resume_ends_as_lost() {
        let mut r = Recorder::default();
        run_live(&mut r, 0, 30 * SEC);
        // STOPPED tidak datang: setelah 3 detik dianggap putus.
        for t in 30..33 {
            assert!(r.on_tick(&tick(t * SEC, false)).is_none());
        }
        let mut t = 33 * SEC;
        let summary = loop {
            if let Some(s) = r.on_tick(&tick(t, false)) {
                break s;
            }
            t += SEC;
            assert!(t < 400 * SEC);
        };
        assert_eq!(summary.end_reason, Some(EndReason::Lost));
        assert_eq!(t, 33 * SEC + RESUME_WINDOW_MS);
        assert_eq!(summary.duration_ms, 29 * SEC);
    }

    #[test]
    fn obs_connection_blip_is_not_a_drop() {
        // Koneksi ke OBS putus 30 detik, tapi stream tetap berjalan (durasi OBS terus naik).
        let mut r = Recorder::default();
        run_live(&mut r, 0, 30 * SEC);
        let gone = Tick { obs_connected: false, ..tick(30 * SEC, false) };
        assert!(r.on_tick(&gone).is_none());
        run_live(&mut r, 60 * SEC, 70 * SEC);
        let c = r.current().unwrap();
        assert_eq!(c.drops, 0);
        assert_eq!(c.duration_ms, 69 * SEC, "tidak dihitung dua kali");
        assert_eq!((c.stream.dropped_frames, c.stream.total_frames), (69, 69 * 60));
    }

    #[test]
    fn joining_a_running_stream_backdates_start() {
        let mut r = Recorder::default();
        // Aplikasi tersambung saat stream sudah berjalan 30 menit.
        let mut k = tick(5 * SEC, true);
        k.duration_ms = 1_800 * SEC;
        r.on_tick(&k);
        let c = r.current().unwrap();
        assert_eq!(c.started_at, WALL0 + 5 * SEC - 1_800 * SEC);
        assert_eq!(c.id, format!("s{}", c.started_at));
    }

    #[test]
    fn chat_from_before_live_is_ignored() {
        let mut r = Recorder::default();
        run_live(&mut r, 0, 5 * SEC);
        let mut early = msg("lama", MessageKind::Text, "ani", None);
        early.timestamp_ms = (WALL0 - 60 * SEC) as i64;
        let mut fresh = msg("baru", MessageKind::Text, "budi", None);
        fresh.timestamp_ms = (WALL0 + 2 * SEC) as i64;
        r.on_chat(&[early, fresh]);
        let c = r.current().unwrap();
        assert_eq!((c.chat.messages, c.chat.chatters), (1, 1));
    }

    #[test]
    fn current_summary_and_limits() {
        let mut r = Recorder::default();
        assert!(r.current().is_none());
        run_live(&mut r, 0, 5 * SEC);
        let names: Vec<ChatMessage> = (0..30)
            .map(|i| msg(&format!("sc{i}"), MessageKind::SuperSticker, &format!("u{i}"), Some("$1.00")))
            .collect();
        r.on_chat(&names);
        for i in 0..60 {
            r.on_alert(&alert(Level::Critical, &format!("alert {i}"), SEC));
        }
        let c = r.current().unwrap();
        assert_eq!(c.end_reason, None);
        assert_eq!(c.chat.super_chat_count, 30);
        assert_eq!(c.chat.super_chats.len(), MAX_PAID);
        assert_eq!(c.alerts.len(), MAX_ALERTS);
        assert_eq!(c.timeline.len(), 1, "menit berjalan ikut tampil");

        let closed = r.finish(WALL0 + 9, EndReason::AppClosed).unwrap();
        assert_eq!(closed.end_reason, Some(EndReason::AppClosed));
        assert!(r.finish(0, EndReason::AppClosed).is_none());
    }

    #[test]
    fn summary_roundtrips_through_json() {
        let mut r = Recorder::default();
        run_live(&mut r, 0, 3 * SEC);
        r.on_alert(&alert(Level::Critical, "Stream berhenti mendadak!", 2 * SEC));
        let s = r.finish(WALL0, EndReason::Stopped).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"endReason\":\"stopped\""), "{json}");
        assert!(json.contains("\"level\":\"critical\""), "{json}");
        let back: SessionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
