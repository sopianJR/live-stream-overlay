mod accelerator;
mod commands;
mod hotkeys;
mod overlay;
mod secrets;
mod settings;

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WindowEvent};

const DASHBOARD_LABEL: &str = "main";

/// Membuka dashboard. Saat ditutup, dashboard dihancurkan (bukan disembunyikan) agar
/// renderer WebView2-nya tidak memakan RAM selama bermain game; dibuat ulang di sini.
fn show_dashboard(app: &AppHandle) {
    let win = match app.get_webview_window(DASHBOARD_LABEL) {
        Some(win) => win,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == DASHBOARD_LABEL)
                .cloned();
            let Some(config) = config else { return };
            match WebviewWindowBuilder::from_config(app, &config).and_then(|b| b.build()) {
                Ok(win) => win,
                Err(e) => {
                    log::warn!("gagal membuat ulang dashboard: {e}");
                    return;
                }
            }
        }
    };
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Buka dashboard", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Tampilkan / sembunyikan overlay", true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", "Mode edit overlay", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &edit, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("LiveStream Overlay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "show" => {
                    show_dashboard(app);
                    Ok(())
                }
                "toggle" => overlay::toggle_visible(app).map(|_| ()),
                "edit" => overlay::toggle_edit_mode(app).map(|_| ()),
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                log::warn!("aksi tray gagal: {e}");
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_dashboard(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Harus plugin pertama: instance kedua cukup membuka dashboard yang sudah ada.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_dashboard(app);
        }))
        // Log ke stdout dan ke file di folder log aplikasi
        // (%LOCALAPPDATA%\com.livestreamapps.overlay\logs) untuk diagnosis.
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .max_file_size(2 * 1024 * 1024)
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(overlay::OverlayManager::default())
        .manage(hotkeys::HotkeyRegistry::default())
        .setup(|app| {
            log::info!("LiveStream Overlay {} dimulai", app.package_info().version);
            // Pengaturan harus sudah tersedia sebelum webview overlay dibuat dan memanggilnya.
            let loaded = settings::load(app.handle());
            app.manage(settings::SettingsState(Mutex::new(loaded.clone())));
            overlay::create(app.handle(), loaded.overlay.monitor.as_deref())?;
            hotkeys::init(app)?;
            hotkeys::apply(app.handle(), &loaded.hotkeys);
            build_tray(app)?;
            overlay::spawn_topmost_watchdog(app.handle().clone());
            // Overlay sudah tampil (pasif); pastikan dashboard yang mendapat fokus saat start.
            show_dashboard(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Menutup dashboard dibiarkan (window dihancurkan); aplikasi tetap hidup karena
            // window overlay tidak pernah ditutup. Alt+F4 saat mode edit hanya keluar dari mode edit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == overlay::OVERLAY_LABEL {
                    api.prevent_close();
                    let _ = overlay::set_edit_mode(window.app_handle(), false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::overlay_get_status,
            commands::overlay_set_visible,
            commands::overlay_toggle_visible,
            commands::overlay_set_edit_mode,
            commands::overlay_set_content_protected,
            commands::monitors_list,
            commands::settings_get,
            commands::settings_save,
            commands::hotkeys_get,
            commands::hotkeys_set_suspended,
            commands::obs_password_status,
            commands::obs_password_set,
            commands::obs_password_clear,
            commands::app_quit,
        ])
        .run(tauri::generate_context!())
        .expect("gagal menjalankan aplikasi");
}
