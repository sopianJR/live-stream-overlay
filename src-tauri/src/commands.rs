//! Command yang dipanggil frontend via `invoke`.

use tauri::{AppHandle, Emitter, State};

use crate::hotkeys::{self, HotkeyInfo, HotkeyRegistry};
use crate::overlay::{self, MonitorInfo, OverlayManager, OverlayStatus};
use crate::secrets;
use crate::settings::{self, AppSettings, SettingsState};

fn to_string_err(e: tauri::Error) -> String {
    e.to_string()
}

// ---------- Overlay ----------

#[tauri::command]
pub fn overlay_get_status(state: State<'_, OverlayManager>) -> OverlayStatus {
    state.status()
}

#[tauri::command]
pub fn overlay_set_visible(app: AppHandle, visible: bool) -> Result<OverlayStatus, String> {
    overlay::set_visible(&app, visible).map_err(to_string_err)
}

#[tauri::command]
pub fn overlay_toggle_visible(app: AppHandle) -> Result<OverlayStatus, String> {
    overlay::toggle_visible(&app).map_err(to_string_err)
}

#[tauri::command]
pub fn overlay_set_edit_mode(app: AppHandle, edit: bool) -> Result<OverlayStatus, String> {
    overlay::set_edit_mode(&app, edit).map_err(to_string_err)
}

#[tauri::command]
pub fn overlay_set_content_protected(
    app: AppHandle,
    enabled: bool,
) -> Result<OverlayStatus, String> {
    overlay::set_content_protected(&app, enabled).map_err(to_string_err)
}

#[tauri::command]
pub fn monitors_list(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    overlay::list_monitors(&app).map_err(to_string_err)
}

// ---------- Pengaturan ----------

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> AppSettings {
    state.get()
}

/// Memvalidasi, menerapkan perubahan (monitor, hotkey), menyimpan, lalu memberi tahu semua window.
#[tauri::command]
pub fn settings_save(
    app: AppHandle,
    state: State<'_, SettingsState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let next = settings.validated().map_err(|errors| errors.join("\n"))?;
    let prev = state.get();

    if next.overlay.monitor != prev.overlay.monitor {
        overlay::move_to_monitor(&app, next.overlay.monitor.as_deref()).map_err(to_string_err)?;
    }
    if next.hotkeys != prev.hotkeys {
        hotkeys::apply(&app, &next.hotkeys);
    }

    settings::persist(&app, &next)?;
    state.set(next.clone());
    if let Err(e) = app.emit(settings::CHANGED_EVENT, &next) {
        log::warn!("gagal emit perubahan pengaturan: {e}");
    }
    Ok(next)
}

// ---------- Hotkey ----------

#[tauri::command]
pub fn hotkeys_get(state: State<'_, HotkeyRegistry>) -> Vec<HotkeyInfo> {
    state.infos()
}

/// Dipanggil dashboard saat mulai/selesai merekam kombinasi tombol baru.
#[tauri::command]
pub fn hotkeys_set_suspended(
    app: AppHandle,
    state: State<'_, SettingsState>,
    suspended: bool,
) -> Vec<HotkeyInfo> {
    hotkeys::set_suspended(&app, suspended, &state.get().hotkeys)
}

// ---------- Rahasia ----------

#[tauri::command]
pub fn obs_password_status() -> Result<bool, String> {
    secrets::has_obs_password()
}

#[tauri::command]
pub fn obs_password_set(password: String) -> Result<(), String> {
    secrets::set_obs_password(&password)
}

#[tauri::command]
pub fn obs_password_clear() -> Result<(), String> {
    secrets::clear_obs_password()
}

// ---------- Aplikasi ----------

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}
