//! Riwayat sesi di `sessions.json` (folder data aplikasi). Hanya lokal.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::recorder::{EndReason, SessionSummary};

pub const FILE_NAME: &str = "sessions.json";
pub const HISTORY_CAP: usize = 30;
const VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreFile {
    pub version: u32,
    /// Terbaru dulu.
    pub history: Vec<SessionSummary>,
    /// Sesi yang sedang berjalan saat terakhir disimpan (untuk pemulihan setelah crash).
    pub current: Option<SessionSummary>,
}

impl StoreFile {
    pub fn new(history: Vec<SessionSummary>, current: Option<SessionSummary>) -> Self {
        Self { version: VERSION, history, current }
    }

    /// Sesi yang tertinggal (aplikasi ditutup/crash saat live) dipindah ke riwayat.
    /// Mengembalikan `true` jika ada yang dipulihkan.
    pub fn recover(&mut self) -> bool {
        let Some(mut s) = self.current.take() else { return false };
        s.ended_at = Some(s.started_at + s.duration_ms);
        s.end_reason = Some(EndReason::AppClosed);
        push(&mut self.history, s);
        true
    }
}

/// Menambah sesi di depan; sesi dengan ID sama diganti, riwayat dibatasi 30.
pub fn push(history: &mut Vec<SessionSummary>, s: SessionSummary) {
    history.retain(|h| h.id != s.id);
    history.insert(0, s);
    history.truncate(HISTORY_CAP);
}

/// File yang tidak ada atau rusak menghasilkan riwayat kosong (file rusak disisihkan).
pub fn load(path: &Path) -> StoreFile {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return StoreFile::default(),
        Err(e) => {
            log::warn!("gagal membaca {}: {e}", path.display());
            return StoreFile::default();
        }
    };
    match serde_json::from_str::<StoreFile>(&text) {
        Ok(mut file) => {
            file.history.truncate(HISTORY_CAP);
            file
        }
        Err(e) => {
            log::warn!("{FILE_NAME} rusak, riwayat sesi dimulai dari kosong: {e}");
            let _ = fs::rename(path, path.with_extension("json.rusak"));
            StoreFile::default()
        }
    }
}

/// Tulis ke file sementara lalu ganti, agar file tidak setengah tertulis saat crash.
pub fn save(path: &Path, file: &StoreFile) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec(file).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::super::recorder::{Recorder, Tick};
    use super::*;

    fn summary(id_wall: u64, live_s: u64) -> SessionSummary {
        let mut r = Recorder::default();
        for t in 0..live_s {
            r.on_tick(&Tick {
                t_ms: t * 1000,
                wall_ms: id_wall + t * 1000,
                obs_connected: true,
                live: true,
                duration_ms: t * 1000,
                ..Tick::default()
            });
        }
        r.current().unwrap()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lso-session-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = temp_path("roundtrip");
        assert_eq!(load(&path), StoreFile::default(), "file belum ada");
        let file = StoreFile::new(vec![summary(1_000, 5)], Some(summary(9_000, 3)));
        save(&path, &file).unwrap();
        assert_eq!(load(&path), file);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_set_aside() {
        let path = temp_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{bukan json").unwrap();
        assert_eq!(load(&path), StoreFile::default());
        assert!(path.with_extension("json.rusak").exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recover_moves_current_to_history() {
        let mut file = StoreFile::new(vec![summary(1_000, 5)], Some(summary(50_000, 61)));
        assert!(file.recover());
        assert!(file.current.is_none());
        let s = &file.history[0];
        assert_eq!(s.end_reason, Some(EndReason::AppClosed));
        assert_eq!(s.ended_at, Some(50_000 + 60_000));
        assert_eq!(file.history.len(), 2);
        assert!(!file.recover(), "tidak ada lagi yang dipulihkan");
    }

    #[test]
    fn history_is_capped_newest_first() {
        let mut history = Vec::new();
        for i in 0..40 {
            push(&mut history, summary(i * 10_000_000, 2));
        }
        assert_eq!(history.len(), HISTORY_CAP);
        assert_eq!(history[0].started_at, 39 * 10_000_000);
        // ID sama diganti, bukan digandakan.
        push(&mut history, summary(39 * 10_000_000, 4));
        assert_eq!(history.len(), HISTORY_CAP);
        assert_eq!(history[0].duration_ms, 3_000);
    }
}
