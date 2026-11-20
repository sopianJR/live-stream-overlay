//! Pemantau level mic dari event high-volume `InputVolumeMeters` OBS (20×/detik).
//!
//! Fungsi murni: dipakai mesin alert untuk "Mic tidak terdengar" dan "Suara mic pecah",
//! serta meter "Tes mic" di dashboard. Tidak ikut di-emit lewat `obs://status`.

use std::collections::VecDeque;

use serde::Serialize;

/// Peak di atas ini dianggap ada suara. Noise floor mic tanpa noise gate biasanya
/// −60 s.d. −50 dBFS; suara bicara jauh di atasnya.
pub const SOUND_DB: f32 = -50.0;
/// Peak di atas ini dianggap pecah (menyentuh 0 dBFS).
pub const CLIP_DB: f32 = -0.5;
/// Tanpa event meter selama ini, level mic dianggap tidak diketahui.
const STALE_MS: u64 = 3_000;
const CLIP_WINDOW_MS: u64 = 10_000;
/// Meter dashboard menampilkan peak tertinggi dalam jendela ini.
const PEAK_HOLD_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicAudio {
    /// Ada event meter untuk input mic dalam 3 detik terakhir.
    pub known: bool,
    /// Lama tanpa suara (sejak suara terakhir, atau sejak pemantauan dimulai).
    pub silent_for_ms: u64,
    /// Berapa detik (dari 10 detik terakhir) suara mic pecah.
    pub clip_seconds: u32,
    /// Peak terbaru dalam dBFS; `None` = senyap total (−∞).
    pub peak_db: Option<f32>,
}

#[derive(Debug, Default)]
pub struct MicMeter {
    input: Option<String>,
    since_ms: u64,
    last_event_ms: Option<u64>,
    last_sound_ms: Option<u64>,
    /// Indeks detik (t / 1000) saat suara pecah, terurut.
    clip_secs: VecDeque<u64>,
    peaks: VecDeque<(u64, Option<f32>)>,
}

fn to_db(mul: f32) -> Option<f32> {
    (mul.is_finite() && mul > 0.0).then(|| 20.0 * mul.log10())
}

impl MicMeter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Satu event `InputVolumeMeters`. `inputs` berisi (nama input, level per channel
    /// `[magnitude, peak, inputPeak]` dalam Mul). Input mic yang tidak ada di event
    /// (tidak aktif di OBS) dihitung senyap.
    pub fn observe<'a>(
        &mut self,
        t_ms: u64,
        mic: Option<&str>,
        inputs: impl IntoIterator<Item = (&'a str, &'a [[f32; 3]])>,
    ) {
        let Some(mic) = mic else {
            self.reset();
            return;
        };
        if self.input.as_deref() != Some(mic) {
            *self = Self { input: Some(mic.to_string()), since_ms: t_ms, ..Self::default() };
        }
        self.last_event_ms = Some(t_ms);

        // Peak post-fader: yang benar-benar masuk ke stream (fader −∞ juga terdeteksi).
        let peak = inputs
            .into_iter()
            .find(|(name, _)| *name == mic)
            .map(|(_, levels)| levels.iter().map(|ch| ch[1]).fold(0.0f32, f32::max))
            .unwrap_or(0.0);
        let db = to_db(peak);

        if db.is_some_and(|d| d >= SOUND_DB) {
            self.last_sound_ms = Some(t_ms);
        }
        if db.is_some_and(|d| d >= CLIP_DB) {
            let sec = t_ms / 1000;
            if self.clip_secs.back() != Some(&sec) {
                self.clip_secs.push_back(sec);
            }
        }
        let oldest_sec = t_ms.saturating_sub(CLIP_WINDOW_MS) / 1000;
        while self.clip_secs.front().is_some_and(|s| *s < oldest_sec) {
            self.clip_secs.pop_front();
        }

        self.peaks.push_back((t_ms, db));
        while self.peaks.front().is_some_and(|(at, _)| t_ms.saturating_sub(*at) > PEAK_HOLD_MS) {
            self.peaks.pop_front();
        }
    }

    pub fn audio(&self, t_ms: u64) -> MicAudio {
        let known = self.input.is_some()
            && self.last_event_ms.is_some_and(|at| t_ms.saturating_sub(at) <= STALE_MS);
        if !known {
            return MicAudio { known: false, silent_for_ms: 0, clip_seconds: 0, peak_db: None };
        }
        let since_sound = self.last_sound_ms.unwrap_or(self.since_ms);
        let oldest_sec = t_ms.saturating_sub(CLIP_WINDOW_MS) / 1000;
        let clip_seconds = self.clip_secs.iter().filter(|s| **s >= oldest_sec).count();
        let peak_db = self.peaks.iter().filter_map(|(_, db)| *db).reduce(f32::max);
        MicAudio {
            known,
            silent_for_ms: t_ms.saturating_sub(since_sound),
            clip_seconds: u32::try_from(clip_seconds).unwrap_or(u32::MAX),
            peak_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mul(db: f32) -> f32 {
        10f32.powf(db / 20.0)
    }

    /// Satu event dengan input "Mic" pada peak `db` (`None` = senyap) dan "Desktop" −30 dB.
    fn feed(m: &mut MicMeter, t: u64, db: Option<f32>) {
        let mic = [[0.0, db.map_or(0.0, mul), 0.0]; 2];
        let desktop = [[0.0, mul(-30.0), 0.0]; 2];
        m.observe(t, Some("Mic"), [("Mic", &mic[..]), ("Desktop", &desktop[..])]);
    }

    #[test]
    fn silence_grows_and_sound_resets() {
        let mut m = MicMeter::default();
        for t in (0..=5_000).step_by(50) {
            feed(&mut m, t, None);
        }
        let a = m.audio(5_000);
        assert!(a.known);
        assert_eq!(a.silent_for_ms, 5_000);
        assert_eq!(a.peak_db, None);

        feed(&mut m, 5_050, Some(-20.0));
        let a = m.audio(5_050);
        assert_eq!(a.silent_for_ms, 0);
        assert!((a.peak_db.unwrap() + 20.0).abs() < 0.01);

        // Noise floor −60 dB tetap dihitung senyap.
        feed(&mut m, 9_000, Some(-60.0));
        assert_eq!(m.audio(9_000).silent_for_ms, 3_950);
    }

    #[test]
    fn missing_input_counts_as_silent() {
        let mut m = MicMeter::default();
        let desktop = [[0.0, mul(-10.0), 0.0]];
        m.observe(0, Some("Mic"), [("Desktop", &desktop[..])]);
        m.observe(2_000, Some("Mic"), [("Desktop", &desktop[..])]);
        let a = m.audio(2_000);
        assert!(a.known);
        assert_eq!(a.silent_for_ms, 2_000);
    }

    #[test]
    fn clipping_counted_per_second_in_window() {
        let mut m = MicMeter::default();
        // Tiga tick pecah dalam detik yang sama = 1 detik.
        for t in [100, 200, 300] {
            feed(&mut m, t, Some(-0.1));
        }
        assert_eq!(m.audio(300).clip_seconds, 1);
        feed(&mut m, 1_500, Some(0.0));
        feed(&mut m, 2_500, Some(-0.2));
        feed(&mut m, 3_000, Some(-3.0));
        assert_eq!(m.audio(3_000).clip_seconds, 3);
        // Setelah 10 detik, detik-detik lama keluar dari jendela.
        feed(&mut m, 12_000, Some(-10.0));
        assert_eq!(m.audio(12_000).clip_seconds, 1);
        feed(&mut m, 14_000, Some(-10.0));
        assert_eq!(m.audio(14_000).clip_seconds, 0);
    }

    #[test]
    fn stale_or_no_mic_is_unknown() {
        let mut m = MicMeter::default();
        assert!(!m.audio(0).known);
        feed(&mut m, 1_000, None);
        assert!(m.audio(4_000).known);
        assert!(!m.audio(4_001).known, "tanpa event > 3 detik");

        m.observe(5_000, None, []);
        assert!(!m.audio(5_000).known, "mic belum dipilih");
    }

    #[test]
    fn changing_mic_restarts_tracking() {
        let mut m = MicMeter::default();
        feed(&mut m, 0, None);
        feed(&mut m, 60_000, None);
        assert_eq!(m.audio(60_000).silent_for_ms, 60_000);
        let other = [[0.0, 0.0, 0.0]];
        m.observe(61_000, Some("Mic Podcast"), [("Mic Podcast", &other[..])]);
        assert_eq!(m.audio(61_000).silent_for_ms, 0);
    }

    #[test]
    fn peak_hold_for_dashboard_meter() {
        let mut m = MicMeter::default();
        feed(&mut m, 0, Some(-12.0));
        feed(&mut m, 100, Some(-40.0));
        assert!((m.audio(100).peak_db.unwrap() + 12.0).abs() < 0.01);
        feed(&mut m, 400, Some(-40.0));
        assert!((m.audio(400).peak_db.unwrap() + 40.0).abs() < 0.01);
    }
}
