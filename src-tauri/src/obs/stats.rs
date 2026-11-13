//! Statistik stream dari sampel polling OBS (fungsi murni, teruji).
//!
//! OBS tidak memberi bitrate secara langsung; yang tersedia hanya penghitung kumulatif
//! (`outputBytes`, `outputSkippedFrames`, `outputTotalFrames`, dst.). Bitrate dan persentase
//! frame yang di-drop dihitung dari selisih penghitung di dalam jendela waktu yang bergeser.

use std::collections::VecDeque;

/// Bitrate = rata-rata 3 detik terakhir (cukup halus, tetap cepat bereaksi).
pub const BITRATE_WINDOW_MS: u64 = 3_000;
/// Persentase drop = 10 detik terakhir (lonjakan singkat tetap terlihat).
pub const DROP_WINDOW_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy)]
struct Sample {
    t_ms: u64,
    a: u64,
    b: u64,
}

/// Jendela geser untuk sepasang penghitung kumulatif `(a, b)`.
#[derive(Debug, Clone)]
pub struct CounterWindow {
    span_ms: u64,
    samples: VecDeque<Sample>,
}

impl CounterWindow {
    pub fn new(span_ms: u64) -> Self {
        Self { span_ms, samples: VecDeque::new() }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Menambah sampel. Penghitung yang mundur berarti output di-restart, jadi riwayat dibuang.
    /// Sampel dengan waktu yang tidak maju diabaikan.
    pub fn push(&mut self, t_ms: u64, a: u64, b: u64) {
        if let Some(last) = self.samples.back() {
            if t_ms <= last.t_ms {
                return;
            }
            if a < last.a || b < last.b {
                self.samples.clear();
            }
        }
        self.samples.push_back(Sample { t_ms, a, b });

        // Sisakan satu sampel di/sebelum batas jendela agar jendela selalu penuh.
        let cutoff = t_ms.saturating_sub(self.span_ms);
        while self.samples.len() >= 2 && self.samples[1].t_ms <= cutoff {
            self.samples.pop_front();
        }
    }

    /// `(Δt ms, Δa, Δb)` antara sampel tertua dan terbaru di jendela.
    pub fn delta(&self) -> Option<(u64, u64, u64)> {
        let first = self.samples.front()?;
        let last = self.samples.back()?;
        (last.t_ms > first.t_ms).then(|| (last.t_ms - first.t_ms, last.a - first.a, last.b - first.b))
    }

    /// Laju `a` dalam kilobit per detik, dengan `a` berupa byte (bit per ms = kbit per detik).
    pub fn kbps(&self) -> Option<f64> {
        let (dt, bytes, _) = self.delta()?;
        Some(bytes as f64 * 8.0 / dt as f64)
    }

    /// Persentase `Δa / Δb`, mis. frame yang di-skip terhadap total frame.
    /// `None` jika tidak ada frame baru di jendela (tidak bisa dihitung).
    pub fn ratio_pct(&self) -> Option<f64> {
        let (_, part, total) = self.delta()?;
        (total > 0).then(|| part as f64 * 100.0 / total as f64)
    }
}

/// Hasil olahan satu sampel `GetStreamStatus`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StreamRates {
    pub bitrate_kbps: Option<f64>,
    pub dropped_pct: Option<f64>,
}

/// Hasil olahan satu sampel `GetStats`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PerfRates {
    /// Frame yang terlewat saat render (GPU/scene terlalu berat).
    pub render_lag_pct: Option<f64>,
    /// Frame yang terlewat saat encoding (encoder kewalahan).
    pub encode_lag_pct: Option<f64>,
}

/// Menyimpan riwayat penghitung OBS selama satu koneksi.
#[derive(Debug, Clone)]
pub struct StatsTracker {
    stream_bytes: CounterWindow,
    stream_frames: CounterWindow,
    render_frames: CounterWindow,
    encode_frames: CounterWindow,
}

impl Default for StatsTracker {
    fn default() -> Self {
        Self {
            stream_bytes: CounterWindow::new(BITRATE_WINDOW_MS),
            stream_frames: CounterWindow::new(DROP_WINDOW_MS),
            render_frames: CounterWindow::new(DROP_WINDOW_MS),
            encode_frames: CounterWindow::new(DROP_WINDOW_MS),
        }
    }
}

impl StatsTracker {
    /// Sampel output stream. Saat stream tidak aktif, riwayat dibuang agar angka dari stream
    /// sebelumnya tidak tercampur.
    pub fn update_stream(
        &mut self,
        t_ms: u64,
        active: bool,
        bytes: u64,
        skipped_frames: u32,
        total_frames: u32,
    ) -> StreamRates {
        if !active {
            self.reset_stream();
            return StreamRates::default();
        }
        self.stream_bytes.push(t_ms, bytes, 0);
        self.stream_frames.push(t_ms, skipped_frames.into(), total_frames.into());
        StreamRates {
            bitrate_kbps: self.stream_bytes.kbps(),
            dropped_pct: self.stream_frames.ratio_pct(),
        }
    }

    pub fn update_perf(
        &mut self,
        t_ms: u64,
        render_skipped: u32,
        render_total: u32,
        encode_skipped: u32,
        encode_total: u32,
    ) -> PerfRates {
        self.render_frames.push(t_ms, render_skipped.into(), render_total.into());
        self.encode_frames.push(t_ms, encode_skipped.into(), encode_total.into());
        PerfRates {
            render_lag_pct: self.render_frames.ratio_pct(),
            encode_lag_pct: self.encode_frames.ratio_pct(),
        }
    }

    pub fn reset_stream(&mut self) {
        self.stream_bytes.clear();
        self.stream_frames.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 6000 kbps = 750.000 byte per detik.
    const BYTES_6000_KBPS: u64 = 750_000;

    fn approx(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn single_sample_has_no_rate() {
        let mut t = StatsTracker::default();
        let r = t.update_stream(0, true, 1_000, 0, 60);
        assert_eq!(r, StreamRates::default());
    }

    #[test]
    fn bitrate_from_byte_deltas() {
        let mut t = StatsTracker::default();
        let mut r = StreamRates::default();
        for i in 0..4 {
            r = t.update_stream(i * 1000, true, i * BYTES_6000_KBPS, 0, 0);
        }
        assert!(approx(r.bitrate_kbps, 6000.0), "{r:?}");
    }

    #[test]
    fn bitrate_only_uses_last_three_seconds() {
        let mut t = StatsTracker::default();
        let mut bytes = 0;
        // 5 detik pada 1000 kbps, lalu 3 detik pada 6000 kbps.
        for i in 0..=5 {
            t.update_stream(i * 1000, true, bytes, 0, 0);
            bytes += 125_000;
        }
        bytes -= 125_000;
        let mut r = StreamRates::default();
        for i in 6..=8 {
            bytes += BYTES_6000_KBPS;
            r = t.update_stream(i * 1000, true, bytes, 0, 0);
        }
        assert!(approx(r.bitrate_kbps, 6000.0), "{r:?}");
    }

    #[test]
    fn irregular_intervals_are_time_weighted() {
        let mut t = StatsTracker::default();
        t.update_stream(0, true, 0, 0, 0);
        // 1,5 detik kemudian: 1.125.000 byte = 6000 kbps.
        let r = t.update_stream(1_500, true, 1_125_000, 0, 0);
        assert!(approx(r.bitrate_kbps, 6000.0), "{r:?}");
    }

    #[test]
    fn counter_reset_starts_fresh() {
        let mut t = StatsTracker::default();
        t.update_stream(0, true, 10_000_000, 50, 6_000);
        t.update_stream(1_000, true, 10_750_000, 50, 6_060);
        // Stream di-restart: penghitung kembali dari nol.
        let r = t.update_stream(2_000, true, 5_000, 0, 30);
        assert_eq!(r, StreamRates::default());
        let r = t.update_stream(3_000, true, 5_000 + BYTES_6000_KBPS, 0, 90);
        assert!(approx(r.bitrate_kbps, 6000.0), "{r:?}");
        assert!(approx(r.dropped_pct, 0.0), "{r:?}");
    }

    #[test]
    fn inactive_stream_clears_history() {
        let mut t = StatsTracker::default();
        t.update_stream(0, true, 0, 0, 0);
        t.update_stream(1_000, true, BYTES_6000_KBPS, 0, 60);
        assert_eq!(t.update_stream(2_000, false, 0, 0, 0), StreamRates::default());
        // Sampel pertama setelah live lagi belum punya pembanding.
        assert_eq!(t.update_stream(3_000, true, 100, 0, 1), StreamRates::default());
    }

    #[test]
    fn dropped_percentage_over_window() {
        let mut t = StatsTracker::default();
        let mut r = StreamRates::default();
        // 60 frame/detik, 3 di antaranya di-drop = 5%.
        for i in 0..=10u32 {
            r = t.update_stream(u64::from(i) * 1000, true, 0, i * 3, i * 60);
        }
        assert!(approx(r.dropped_pct, 5.0), "{r:?}");
    }

    #[test]
    fn drop_spike_leaves_window_after_ten_seconds() {
        let mut t = StatsTracker::default();
        let (mut skipped, mut total) = (0, 0);
        let mut r = StreamRates::default();
        for i in 0..=25u64 {
            total += 60;
            if i == 2 {
                skipped += 30; // lonjakan sekali
            }
            r = t.update_stream(i * 1000, true, 0, skipped, total);
            if i == 5 {
                assert!(r.dropped_pct.unwrap() > 1.0, "lonjakan masih di jendela: {r:?}");
            }
        }
        assert!(approx(r.dropped_pct, 0.0), "{r:?}");
    }

    #[test]
    fn no_new_frames_means_unknown_drop() {
        let mut t = StatsTracker::default();
        t.update_stream(0, true, 0, 5, 100);
        let r = t.update_stream(1_000, true, 1_000, 5, 100);
        assert_eq!(r.dropped_pct, None);
        assert!(r.bitrate_kbps.is_some());
    }

    #[test]
    fn stale_or_duplicate_timestamps_are_ignored() {
        let mut t = StatsTracker::default();
        t.update_stream(1_000, true, 0, 0, 0);
        t.update_stream(2_000, true, BYTES_6000_KBPS, 0, 0);
        let r = t.update_stream(2_000, true, 999_999_999, 0, 0);
        assert!(approx(r.bitrate_kbps, 6000.0), "{r:?}");
    }

    #[test]
    fn perf_lag_percentages() {
        let mut t = StatsTracker::default();
        t.update_perf(0, 10, 1_000, 0, 1_000);
        let r = t.update_perf(1_000, 16, 1_060, 3, 1_060);
        assert!(approx(r.render_lag_pct, 10.0), "{r:?}");
        assert!(approx(r.encode_lag_pct, 5.0), "{r:?}");
    }
}
