//! Pengaturan aplikasi: struktur data, nilai default, validasi (fungsi murni),
//! serta muat/simpan ke `settings.json` lewat tauri-plugin-store.
//!
//! Rahasia (password OBS, token OAuth) TIDAK disimpan di sini; lihat `secrets.rs`.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::accelerator;

pub const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";
pub const CHANGED_EVENT: &str = "settings://changed";

pub const OPACITY_RANGE: (f64, f64) = (0.3, 1.0);
pub const FONT_SCALE_RANGE: (f64, f64) = (0.8, 1.6);
/// 0 = pesan chat tidak pernah memudar.
pub const CHAT_FADE_RANGE: (u32, u32) = (5, 300);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObsSettings {
    pub host: String,
    pub port: u16,
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self { host: "127.0.0.1".into(), port: 4455 }
    }
}

impl ObsSettings {
    fn validate(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        self.host = self.host.trim().to_string();
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
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self { toggle_overlay: "Ctrl+Shift+O".into(), toggle_edit: "Ctrl+Shift+E".into() }
    }
}

impl HotkeySettings {
    fn validate(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for field in [&mut self.toggle_overlay, &mut self.toggle_edit] {
            match accelerator::normalize(field) {
                Ok(normalized) => *field = normalized,
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() && self.toggle_overlay == self.toggle_edit {
            errors.push(format!("Hotkey \"{}\" dipakai untuk dua aksi berbeda.", self.toggle_overlay));
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub obs: ObsSettings,
    pub youtube: YoutubeSettings,
    pub overlay: OverlaySettings,
    pub hotkeys: HotkeySettings,
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
