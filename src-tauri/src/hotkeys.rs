//! Hotkey global yang bisa diubah dari dashboard.
//!
//! - Gagal mendaftar (mis. sudah dipakai aplikasi lain) tidak membuat aplikasi crash;
//!   statusnya ditampilkan di dashboard.
//! - Saat dashboard merekam kombinasi baru, semua hotkey dijeda. Tanpa jeda,
//!   Windows "memakan" kombinasi yang sedang terdaftar sehingga tidak bisa direkam ulang.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{App, AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::settings::HotkeySettings;
use crate::obs::{self, QuickAction};
use crate::{accelerator, overlay};

// Semua aksi memang berupa toggle; nama lengkapnya lebih mudah dibaca di pemanggil.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    ToggleOverlay,
    ToggleEditMode,
    ToggleMic,
    ToggleRecord,
}

impl Action {
    const ALL: [Action; 4] =
        [Action::ToggleOverlay, Action::ToggleEditMode, Action::ToggleMic, Action::ToggleRecord];

    fn id(self) -> &'static str {
        match self {
            Action::ToggleOverlay => "toggleOverlay",
            Action::ToggleEditMode => "toggleEdit",
            Action::ToggleMic => "toggleMic",
            Action::ToggleRecord => "toggleRecord",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::ToggleOverlay => "Tampilkan / sembunyikan overlay",
            Action::ToggleEditMode => "Mode edit / interaksi overlay",
            Action::ToggleMic => "Mute / unmute mic di OBS",
            Action::ToggleRecord => "Mulai / hentikan rekaman OBS",
        }
    }

    fn accelerator(self, settings: &HotkeySettings) -> &str {
        match self {
            Action::ToggleOverlay => &settings.toggle_overlay,
            Action::ToggleEditMode => &settings.toggle_edit,
            Action::ToggleMic => &settings.toggle_mic,
            Action::ToggleRecord => &settings.toggle_record,
        }
    }

    fn run(self, app: &AppHandle) {
        let result = match self {
            Action::ToggleOverlay => overlay::toggle_visible(app).map(|_| ()),
            Action::ToggleEditMode => overlay::toggle_edit_mode(app).map(|_| ()),
            // Perintah OBS berjalan async; hasilnya muncul sebagai notifikasi di overlay.
            Action::ToggleMic => {
                obs::spawn_quick_action(app, QuickAction::ToggleMic);
                Ok(())
            }
            Action::ToggleRecord => {
                obs::spawn_quick_action(app, QuickAction::ToggleRecord);
                Ok(())
            }
        };
        if let Err(e) = result {
            log::warn!("aksi hotkey gagal: {e}");
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyInfo {
    /// Sama dengan nama field di `HotkeySettings` (camelCase).
    pub id: &'static str,
    pub label: &'static str,
    pub accelerator: String,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    bound: Vec<(Shortcut, Action)>,
    infos: Vec<HotkeyInfo>,
    suspended: bool,
}

#[derive(Default)]
pub struct HotkeyRegistry(Mutex<Inner>);

impl HotkeyRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn action_for(&self, shortcut: &Shortcut) -> Option<Action> {
        let inner = self.lock();
        if inner.suspended {
            return None;
        }
        inner.bound.iter().find(|(s, _)| s == shortcut).map(|(_, a)| *a)
    }

    pub fn infos(&self) -> Vec<HotkeyInfo> {
        self.lock().infos.clone()
    }
}

/// Memasang plugin global-shortcut. Pendaftaran hotkey dilakukan lewat `apply`.
pub fn init(app: &App) -> tauri::Result<()> {
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                if let Some(action) = app.state::<HotkeyRegistry>().action_for(shortcut) {
                    action.run(app);
                }
            })
            .build(),
    )
}

/// Mendaftarkan ulang semua hotkey sesuai pengaturan. Selalu berhasil; kegagalan
/// per hotkey dicatat di `HotkeyInfo`.
pub fn apply(app: &AppHandle, settings: &HotkeySettings) -> Vec<HotkeyInfo> {
    let registry = app.state::<HotkeyRegistry>();
    let shortcuts = app.global_shortcut();

    if let Err(e) = shortcuts.unregister_all() {
        log::warn!("gagal melepas hotkey lama: {e}");
    }

    let suspended = registry.lock().suspended;
    let mut bound = Vec::new();
    let mut infos = Vec::new();

    for action in Action::ALL {
        let accel = action.accelerator(settings).to_string();
        let result = accelerator::parse(&accel).and_then(|shortcut| {
            if suspended {
                return Ok(shortcut);
            }
            shortcuts
                .register(shortcut)
                .map(|_| shortcut)
                .map_err(|e| format!("Gagal didaftarkan, mungkin dipakai aplikasi lain ({e})."))
        });
        match &result {
            Err(e) => log::warn!("hotkey {accel}: {e}"),
            Ok(_) if suspended => log::info!("hotkey {accel} dijeda"),
            Ok(_) => log::info!("hotkey {accel} aktif ({})", action.id()),
        }
        infos.push(HotkeyInfo {
            id: action.id(),
            label: action.label(),
            accelerator: accel,
            registered: result.is_ok() && !suspended,
            error: result.as_ref().err().cloned(),
        });
        if let Ok(shortcut) = result {
            bound.push((shortcut, action));
        }
    }

    let mut inner = registry.lock();
    inner.bound = bound;
    inner.infos = infos.clone();
    infos
}

/// Menjeda (melepas) atau melanjutkan (mendaftar ulang) semua hotkey.
pub fn set_suspended(app: &AppHandle, suspended: bool, settings: &HotkeySettings) -> Vec<HotkeyInfo> {
    app.state::<HotkeyRegistry>().lock().suspended = suspended;
    apply(app, settings)
}
