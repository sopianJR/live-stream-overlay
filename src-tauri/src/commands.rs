//! Command yang dipanggil frontend via `invoke`.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::alerts::{self, engine::Alert, AlertService};
use crate::hotkeys::{self, HotkeyInfo, HotkeyRegistry};
use crate::obs::{self, MicAudio, ObsService, ObsStatus};
use crate::overlay::{self, MonitorInfo, OverlayManager, OverlayStatus};
use crate::secrets;
use crate::session::{SessionService, SessionState};
use crate::settings::{self, AppSettings, SettingsState};
use crate::trakteer::{self, KeyStatus, TrakteerService, TrakteerSnapshot};
use crate::youtube::account::{self, AccountService, AccountStatus};
use crate::youtube::{ChatService, ChatSnapshot};

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

    // Servis OBS membaca pengaturan dari state, jadi diberi tahu setelah `state.set`.
    let obs = app.state::<ObsService>();
    if !next.obs.same_endpoint(&prev.obs) {
        obs.reconnect();
    } else {
        if next.obs.mic_input != prev.obs.mic_input {
            obs.refresh_mic();
        }
        if next.alerts.wants_mic_levels() != prev.alerts.wants_mic_levels() {
            obs.resubscribe();
        }
    }
    if next.youtube.source != prev.youtube.source {
        app.state::<ChatService>().reconnect();
    }
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
pub fn obs_password_set(obs: State<'_, ObsService>, password: String) -> Result<(), String> {
    secrets::set_obs_password(&password)?;
    obs.reconnect();
    Ok(())
}

#[tauri::command]
pub fn obs_password_clear(obs: State<'_, ObsService>) -> Result<(), String> {
    secrets::clear_obs_password()?;
    obs.reconnect();
    Ok(())
}

// ---------- OBS ----------

#[tauri::command]
pub fn obs_get_status(obs: State<'_, ObsService>) -> ObsStatus {
    obs.status()
}

/// Level mic terkini untuk meter "Tes mic" di dashboard.
#[tauri::command]
pub fn obs_mic_level(obs: State<'_, ObsService>) -> MicAudio {
    obs.mic_audio_for_meter()
}

#[tauri::command]
pub fn obs_reconnect(obs: State<'_, ObsService>) {
    obs.reconnect();
}

#[tauri::command]
pub async fn obs_start_stream(app: AppHandle) -> Result<(), String> {
    obs::start_stream(&app).await
}

/// Konfirmasi (tahan tombol) dilakukan di UI; command ini langsung menghentikan stream.
#[tauri::command]
pub async fn obs_stop_stream(app: AppHandle) -> Result<(), String> {
    obs::stop_stream(&app).await
}

#[tauri::command]
pub async fn obs_toggle_record(app: AppHandle) -> Result<bool, String> {
    obs::toggle_record(&app).await
}

#[tauri::command]
pub async fn obs_toggle_mic(app: AppHandle) -> Result<bool, String> {
    obs::toggle_mic(&app).await
}

#[tauri::command]
pub async fn obs_set_scene(app: AppHandle, name: String) -> Result<(), String> {
    obs::set_scene(&app, &name).await
}

#[tauri::command]
pub async fn obs_list_audio_inputs(app: AppHandle) -> Result<Vec<String>, String> {
    obs::list_audio_inputs(&app).await
}

// ---------- Chat YouTube ----------

#[tauri::command]
pub fn chat_get_snapshot(chat: State<'_, ChatService>) -> ChatSnapshot {
    chat.snapshot()
}

#[tauri::command]
pub fn chat_reconnect(chat: State<'_, ChatService>) {
    chat.reconnect();
}

/// Mengirim pesan ke live chat yang sedang ditampilkan (butuh login Google).
#[tauri::command]
pub async fn chat_send(app: AppHandle, text: String) -> Result<(), String> {
    account::send_chat(&app, &text).await
}

// ---------- Akun Google ----------

#[tauri::command]
pub fn account_get_status(account: State<'_, AccountService>) -> AccountStatus {
    account.status()
}

/// Client secret hanya masuk ke Credential Manager, tidak pernah dikirim balik.
#[tauri::command]
pub fn account_set_client(
    app: AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<AccountStatus, String> {
    account::set_client(&app, &client_id, &client_secret)
}

#[tauri::command]
pub async fn account_clear_client(app: AppHandle) -> Result<AccountStatus, String> {
    account::clear_client(&app).await
}

#[tauri::command]
pub async fn account_login(app: AppHandle) -> Result<AccountStatus, String> {
    account::login(&app).await
}

#[tauri::command]
pub fn account_cancel_login(app: AppHandle) {
    account::cancel_login(&app);
}

#[tauri::command]
pub async fn account_logout(app: AppHandle) -> Result<AccountStatus, String> {
    account::logout(&app).await
}

// ---------- Trakteer ----------

#[tauri::command]
pub fn trakteer_get_state(trakteer: State<'_, TrakteerService>) -> TrakteerSnapshot {
    trakteer.snapshot()
}

/// Hanya memberi tahu apakah kunci ada (dan versinya); kuncinya sendiri tidak dikirim.
#[tauri::command]
pub fn trakteer_key_status() -> KeyStatus {
    trakteer::key_status()
}

#[tauri::command]
pub fn trakteer_set_key(app: AppHandle, key: String) -> Result<KeyStatus, String> {
    trakteer::set_key(&app, &key)
}

#[tauri::command]
pub fn trakteer_clear_key(app: AppHandle) -> Result<KeyStatus, String> {
    trakteer::clear_key(&app)
}

#[tauri::command]
pub fn trakteer_reconnect(trakteer: State<'_, TrakteerService>) {
    trakteer.reconnect();
}

/// Donasi contoh di overlay (tidak masuk riwayat), untuk mengatur posisi widget.
#[tauri::command]
pub fn trakteer_test_donation(app: AppHandle) {
    trakteer::emit_sample(&app);
}

// ---------- Alert ----------

#[tauri::command]
pub fn alerts_get(alerts: State<'_, AlertService>) -> Vec<Alert> {
    alerts.active()
}

#[tauri::command]
pub fn alerts_dismiss(app: AppHandle, id: String) -> Vec<Alert> {
    alerts::dismiss(&app, &id)
}

/// Menampilkan contoh alert selama 10 detik (untuk mengatur posisi widget).
#[tauri::command]
pub fn alerts_test(app: AppHandle) -> Vec<Alert> {
    alerts::raise_test(&app)
}

// ---------- Ringkasan sesi ----------

/// `takeFocus`: dashboard yang baru dibuka mengambil penanda "tampilkan sesi ini" (sekali saja).
#[tauri::command]
pub fn session_get_state(sessions: State<'_, SessionService>, take_focus: bool) -> SessionState {
    sessions.state(take_focus)
}

#[tauri::command]
pub fn session_delete(sessions: State<'_, SessionService>, id: String) -> SessionState {
    sessions.delete(&id)
}

#[tauri::command]
pub fn session_clear(sessions: State<'_, SessionService>) -> SessionState {
    sessions.clear()
}

// ---------- Aplikasi ----------

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}

/// Membuka folder log (riwayat koneksi & alert) di File Explorer.
#[tauri::command]
pub fn app_open_log_folder(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app.path().app_log_dir().map_err(|e| format!("Folder log tidak ditemukan: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Gagal membuat folder log: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Gagal membuka folder log: {e}"))
}
