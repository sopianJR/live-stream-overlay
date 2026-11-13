//! Pengaturan aplikasi: struktur data, nilai default, validasi (fungsi murni),
//! serta muat/simpan ke `settings.json` lewat tauri-plugin-store.
//!
//! Rahasia (password OBS, token OAuth) TIDAK disimpan di sini; lihat `secrets.rs`.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::accelerator;
use crate::youtube::source;

pub const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";
pub const CHANGED_EVENT: &str = "settings://changed";

pub const OPACITY_RANGE: (f64, f64) = (0.3, 1.0);
pub const FONT_SCALE_RANGE: (f64, f64) = (0.8, 1.6);
/// 0 = pesan chat tidak pernah memudar.
pub const CHAT_FADE_RANGE: (u32, u32) = (5, 300);
pub const DROPPED_PCT_RANGE: (f64, f64) = (0.1, 50.0);
/// 0 = alert bitrate rendah dimatikan.
pub const LOW_BITRATE_PCT_RANGE: (u32, u32) = (10, 90);
/// 0 = alert CPU dimatikan.
pub const CPU_PCT_RANGE: (u32, u32) = (50, 100);
pub const TRAKTEER_ALERT_SECONDS_RANGE: (u32, u32) = (3, 60);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObsSettings {
    pub host: String,
    pub port: u16,
    /// Nama input audio OBS yang dipantau sebagai mic. `None` = input "Mic/Aux" global OBS.
    pub mic_input: Option<String>,
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self { host: "127.0.0.1".into(), port: 4455, mic_input: None }
    }
}

impl ObsSettings {
    /// Perubahan yang mengharuskan koneksi dibuat ulang.
    pub fn same_endpoint(&self, other: &Self) -> bool {
        self.host == other.host && self.port == other.port
    }
}

impl ObsSettings {
    fn validate(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        self.host = self.host.trim().to_string();
        self.mic_input = self.mic_input.take().map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
        if self.host.is_empty() {
            errors.push("Host OBS tidak boleh kosong.".to_string());
        } else if self.host.contains("://") || self.host.chars().any(char::is_whitespace) {
            errors.push("Host OBS cukup nama host atau IP, tanpa \"ws://\" dan tanpa spasi.".into());
        }
        if self.port == 0 {
            errors.push("Port OBS harus di antara 1 dan 65535.".to_string());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct YoutubeSettings {
    /// URL video live, URL channel, atau @handle. Diolah di Fase 3.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlaySettings {
    /// ID monitor (nama perangkat Windows, mis. `\\.\DISPLAY2`). `None` = monitor utama.
    pub monitor: Option<String>,
    /// Pengali opasitas latar widget.
    pub widget_opacity: f64,
    /// Pengali ukuran huruf.
    pub font_scale: f64,
    pub chat_fade_seconds: u32,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self { monitor: None, widget_opacity: 1.0, font_scale: 1.0, chat_fade_seconds: 15 }
    }
}

fn clamp_f64(value: f64, (min, max): (f64, f64), fallback: f64) -> f64 {
    if value.is_finite() { value.clamp(min, max) } else { fallback }
}

impl OverlaySettings {
    /// Tidak pernah gagal: nilai di luar rentang dipaskan ke batas terdekat.
    fn sanitize(&mut self) {
        let d = Self::default();
        self.monitor = self.monitor.take().map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
        self.widget_opacity = clamp_f64(self.widget_opacity, OPACITY_RANGE, d.widget_opacity);
        self.font_scale = clamp_f64(self.font_scale, FONT_SCALE_RANGE, d.font_scale);
        if self.chat_fade_seconds != 0 {
            self.chat_fade_seconds = self.chat_fade_seconds.clamp(CHAT_FADE_RANGE.0, CHAT_FADE_RANGE.1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
    pub toggle_overlay: String,
    pub toggle_edit: String,
    pub toggle_mic: String,
    pub toggle_record: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_overlay: "Ctrl+Shift+O".into(),
            toggle_edit: "Ctrl+Shift+E".into(),
            toggle_mic: "Ctrl+Shift+M".into(),
            toggle_record: "Ctrl+Shift+R".into(),
        }
    }
}

impl HotkeySettings {
    fn fields_mut(&mut self) -> [&mut String; 4] {
        [&mut self.toggle_overlay, &mut self.toggle_edit, &mut self.toggle_mic, &mut self.toggle_record]
    }

    fn validate(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for field in self.fields_mut() {
            match accelerator::normalize(field) {
                Ok(normalized) => *field = normalized,
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() {
            let all = self.fields_mut().map(|f| f.clone());
            for (i, accel) in all.iter().enumerate() {
                if all[..i].contains(accel) {
                    errors.push(format!("Hotkey \"{accel}\" dipakai untuk dua aksi berbeda."));
                }
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

/// Ambang alert masalah stream (lihat `alerts/engine.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertSettings {
    pub enabled: bool,
    /// Peringatan jika frame di-drop (10 detik terakhir) mencapai persentase ini.
    pub dropped_warn_pct: f64,
    /// Alert kritis (menetap) mulai persentase ini.
    pub dropped_critical_pct: f64,
    /// Peringatan jika bitrate di bawah persentase ini dari bitrate normal. 0 = mati.
    pub low_bitrate_pct: u32,
    /// Peringatan jika CPU OBS mencapai persentase ini. 0 = mati.
    pub cpu_pct: u32,
    /// Ingatkan tiap 2 menit jika mic masih mute saat live.
    pub mic_reminder: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            dropped_warn_pct: 1.0,
            dropped_critical_pct: 5.0,
            low_bitrate_pct: 50,
            cpu_pct: 90,
            mic_reminder: true,
        }
    }
}

fn clamp_or_off(value: u32, (min, max): (u32, u32)) -> u32 {
    if value == 0 { 0 } else { value.clamp(min, max) }
}

impl AlertSettings {
    /// Nilai di luar rentang dipaskan; ambang kritis tidak boleh di bawah ambang peringatan.
    fn sanitize(&mut self) {
        let d = Self::default();
        self.dropped_warn_pct = clamp_f64(self.dropped_warn_pct, DROPPED_PCT_RANGE, d.dropped_warn_pct);
        self.dropped_critical_pct =
            clamp_f64(self.dropped_critical_pct, DROPPED_PCT_RANGE, d.dropped_critical_pct);
        self.low_bitrate_pct = clamp_or_off(self.low_bitrate_pct, LOW_BITRATE_PCT_RANGE);
        self.cpu_pct = clamp_or_off(self.cpu_pct, CPU_PCT_RANGE);
    }

    fn validate(&mut self) -> Result<(), Vec<String>> {
        self.sanitize();
        if self.dropped_critical_pct < self.dropped_warn_pct {
            return Err(vec![
                "Ambang kritis frame drop harus sama atau lebih besar dari ambang peringatan.".into(),
            ]);
        }
        Ok(())
    }
}

/// Tampilan alert donasi Trakteer di overlay. Kuncinya ada di Credential Manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrakteerSettings {
    /// Lama satu kartu donasi tampil.
    pub alert_seconds: u32,
    /// Donasi di bawah nominal ini tidak ditampilkan. 0 = semua.
    pub min_amount_rp: u64,
    /// Tampilkan donasi test dari tombol "Test alert" di dashboard Trakteer.
    pub show_test: bool,
    /// Tampilkan pesan donatur di kartu.
    pub show_message: bool,
}

impl Default for TrakteerSettings {
    fn default() -> Self {
        Self { alert_seconds: 8, min_amount_rp: 0, show_test: true, show_message: true }
    }
}

impl TrakteerSettings {
    fn sanitize(&mut self) {
        let (min, max) = TRAKTEER_ALERT_SECONDS_RANGE;
        self.alert_seconds = self.alert_seconds.clamp(min, max);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub obs: ObsSettings,
    pub youtube: YoutubeSettings,
    pub overlay: OverlaySettings,
    pub hotkeys: HotkeySettings,
    pub alerts: AlertSettings,
    pub trakteer: TrakteerSettings,
}

impl AppSettings {
    /// Merapikan dan memvalidasi. Dipakai saat user menyimpan: kesalahan dikembalikan
    /// apa adanya agar bisa ditampilkan di dashboard.
    pub fn validated(mut self) -> Result<Self, Vec<String>> {
        let mut errors = Vec::new();
        if let Err(e) = self.obs.validate() {
            errors.extend(e);
        }
        if let Err(e) = self.hotkeys.validate() {
            errors.extend(e);
        }
        self.youtube.source = self.youtube.source.trim().to_string();
        if !self.youtube.source.is_empty() {
            if let Err(e) = source::parse(&self.youtube.source) {
                errors.push(format!("Sumber chat: {e}"));
            }
        }
        if let Err(e) = self.alerts.validate() {
            errors.extend(e);
        }
        self.trakteer.sanitize();
        self.overlay.sanitize();
        if errors.is_empty() { Ok(self) } else { Err(errors) }
    }

    /// Dipakai saat memuat file: bagian yang rusak diganti default tanpa
    /// membuang bagian lain yang masih valid.
    pub fn repaired(mut self) -> Self {
        if let Err(e) = self.obs.validate() {
            log::warn!("pengaturan OBS tidak valid, pakai default: {e:?}");
            self.obs = ObsSettings::default();
        }
        if let Err(e) = self.hotkeys.validate() {
            log::warn!("pengaturan hotkey tidak valid, pakai default: {e:?}");
            self.hotkeys = HotkeySettings::default();
        }
        self.youtube.source = self.youtube.source.trim().to_string();
        if self.alerts.validate().is_err() {
            log::warn!("ambang alert tidak konsisten, ambang kritis disamakan dengan peringatan");
            self.alerts.dropped_critical_pct = self.alerts.dropped_warn_pct;
        }
        self.trakteer.sanitize();
        self.overlay.sanitize();
        self
    }
}

/// State global pengaturan aktif.
pub struct SettingsState(pub Mutex<AppSettings>);

impl SettingsState {
    pub fn get(&self) -> AppSettings {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, settings: AppSettings) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    }
}

pub fn load(app: &AppHandle) -> AppSettings {
    let stored = match app.store(STORE_FILE) {
        Ok(store) => store.get(STORE_KEY),
        Err(e) => {
            log::warn!("gagal membuka {STORE_FILE}: {e}");
            None
        }
    };
    match stored.map(serde_json::from_value::<AppSettings>) {
        Some(Ok(settings)) => {
            log::info!("pengaturan dimuat dari {STORE_FILE}");
            settings.repaired()
        }
        Some(Err(e)) => {
            log::warn!("{STORE_FILE} rusak, pakai default: {e}");
            AppSettings::default()
        }
        None => {
            log::info!("{STORE_FILE} belum ada, pakai pengaturan default");
            AppSettings::default()
        }
    }
}

pub fn persist(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    store.set(STORE_KEY, value);
    store.save().map_err(|e| format!("gagal menyimpan pengaturan: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let d = AppSettings::default();
        assert_eq!(d.clone().validated().unwrap(), d);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"obs":{"port":4460}}"#).unwrap();
        assert_eq!(s.obs.port, 4460);
        assert_eq!(s.obs.host, "127.0.0.1");
        assert_eq!(s.obs.mic_input, None);
        assert_eq!(s.hotkeys, HotkeySettings::default());
        assert_eq!(s.overlay.chat_fade_seconds, 15);
    }

    #[test]
    fn rejects_bad_obs_host_and_port() {
        let mut s = AppSettings::default();
        s.obs.host = "ws://localhost".into();
        s.obs.port = 0;
        let errors = s.validated().unwrap_err();
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("ws://"));
        assert!(errors[1].contains("Port"));
    }

    #[test]
    fn trims_and_normalizes() {
        let mut s = AppSettings::default();
        s.obs.host = "  192.168.1.5 ".into();
        s.youtube.source = "  @kanalku  ".into();
        s.hotkeys.toggle_overlay = "shift+ctrl+o".into();
        s.overlay.monitor = Some("   ".into());
        let v = s.validated().unwrap();
        assert_eq!(v.obs.host, "192.168.1.5");
        assert_eq!(v.youtube.source, "@kanalku");
        assert_eq!(v.hotkeys.toggle_overlay, "Ctrl+Shift+O");
        assert_eq!(v.overlay.monitor, None);
    }

    #[test]
    fn clamps_overlay_values() {
        let mut s = AppSettings::default();
        s.overlay.widget_opacity = 5.0;
        s.overlay.font_scale = 0.1;
        s.overlay.chat_fade_seconds = 1;
        let v = s.validated().unwrap();
        assert_eq!(v.overlay.widget_opacity, OPACITY_RANGE.1);
        assert_eq!(v.overlay.font_scale, FONT_SCALE_RANGE.0);
        assert_eq!(v.overlay.chat_fade_seconds, CHAT_FADE_RANGE.0);

        let mut never = AppSettings::default();
        never.overlay.chat_fade_seconds = 0;
        assert_eq!(never.validated().unwrap().overlay.chat_fade_seconds, 0);
    }

    #[test]
    fn rejects_duplicate_hotkeys() {
        let mut s = AppSettings::default();
        s.hotkeys.toggle_edit = "ctrl+shift+o".into();
        let errors = s.validated().unwrap_err();
        assert!(errors[0].contains("dua aksi"));

        let mut s = AppSettings::default();
        s.hotkeys.toggle_record = "Shift+Ctrl+M".into();
        let errors = s.validated().unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Ctrl+Shift+M"), "{errors:?}");
    }

    #[test]
    fn settings_from_phase_one_get_new_hotkeys() {
        let s: AppSettings = serde_json::from_str(
            r#"{"hotkeys":{"toggleOverlay":"Ctrl+Alt+O","toggleEdit":"Ctrl+Alt+E"}}"#,
        )
        .unwrap();
        let v = s.validated().unwrap();
        assert_eq!(v.hotkeys.toggle_overlay, "Ctrl+Alt+O");
        assert_eq!(v.hotkeys.toggle_mic, "Ctrl+Shift+M");
        assert_eq!(v.hotkeys.toggle_record, "Ctrl+Shift+R");
    }

    #[test]
    fn youtube_source_is_validated() {
        let mut s = AppSettings::default();
        s.youtube.source = " @LofiGirl ".into();
        assert_eq!(s.clone().validated().unwrap().youtube.source, "@LofiGirl");
        s.youtube.source = "bukan sumber".into();
        let errors = s.validated().unwrap_err();
        assert!(errors[0].starts_with("Sumber chat:"), "{errors:?}");
    }

    #[test]
    fn trakteer_settings() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.trakteer, TrakteerSettings::default());
        let mut s = AppSettings::default();
        s.trakteer.alert_seconds = 1;
        assert_eq!(s.clone().validated().unwrap().trakteer.alert_seconds, TRAKTEER_ALERT_SECONDS_RANGE.0);
        s.trakteer.alert_seconds = 999;
        assert_eq!(s.repaired().trakteer.alert_seconds, TRAKTEER_ALERT_SECONDS_RANGE.1);
    }

    #[test]
    fn alert_thresholds() {
        let mut s = AppSettings::default();
        s.alerts.dropped_warn_pct = 0.0;
        s.alerts.low_bitrate_pct = 5;
        s.alerts.cpu_pct = 0;
        let v = s.validated().unwrap();
        assert_eq!(v.alerts.dropped_warn_pct, DROPPED_PCT_RANGE.0);
        assert_eq!(v.alerts.low_bitrate_pct, LOW_BITRATE_PCT_RANGE.0);
        assert_eq!(v.alerts.cpu_pct, 0, "0 = dimatikan");

        let mut s = AppSettings::default();
        s.alerts.dropped_warn_pct = 8.0;
        s.alerts.dropped_critical_pct = 3.0;
        assert!(s.clone().validated().unwrap_err()[0].contains("kritis"));
        assert_eq!(s.repaired().alerts.dropped_critical_pct, 8.0);
    }

    #[test]
    fn mic_input_is_trimmed() {
        let mut s = AppSettings::default();
        s.obs.mic_input = Some("  Mic Podcast ".into());
        assert_eq!(s.clone().validated().unwrap().obs.mic_input.as_deref(), Some("Mic Podcast"));
        s.obs.mic_input = Some("   ".into());
        assert_eq!(s.validated().unwrap().obs.mic_input, None);
    }

    #[test]
    fn repaired_keeps_valid_sections() {
        let mut s = AppSettings::default();
        s.obs.port = 4999;
        s.hotkeys.toggle_overlay = "O".into(); // tidak valid: tanpa modifier
        let r = s.repaired();
        assert_eq!(r.obs.port, 4999);
        assert_eq!(r.hotkeys, HotkeySettings::default());
    }
}
