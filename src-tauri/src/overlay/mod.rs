//! Manajemen window overlay: pembuatan, tampil/sembunyi, mode pasif/edit,
//! proteksi capture, dan watchdog topmost.

#[cfg(windows)]
mod win32;

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const OVERLAY_LABEL: &str = "overlay";
pub const STATE_EVENT: &str = "overlay://state";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatus {
    pub visible: bool,
    pub edit_mode: bool,
    pub content_protected: bool,
}

impl Default for OverlayStatus {
    fn default() -> Self {
        Self {
            visible: false,
            edit_mode: false,
            // Default aktif: overlay tidak ikut tertangkap OBS (Display Capture).
            content_protected: true,
        }
    }
}

#[derive(Default)]
struct Inner {
    status: OverlayStatus,
    /// HWND window yang fokus sebelum masuk mode edit (biasanya game).
    prev_foreground: isize,
}

#[derive(Default)]
pub struct OverlayManager {
    inner: Mutex<Inner>,
}

impl OverlayManager {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> OverlayStatus {
        self.lock().status
    }
}

fn window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    app.get_webview_window(OVERLAY_LABEL)
        .ok_or(tauri::Error::WindowNotFound)
}

fn emit_status(app: &AppHandle) -> OverlayStatus {
    let status = app.state::<OverlayManager>().status();
    if let Err(e) = app.emit(STATE_EVENT, status) {
        log::warn!("gagal emit status overlay: {e}");
    }
    status
}

/// Bit Win32 yang tidak dikelola tao perlu diterapkan ulang setelah tao mengubah flag.
fn reapply_native_style(win: &WebviewWindow) {
    #[cfg(windows)]
    win32::ensure_toolwindow(win);
    #[cfg(not(windows))]
    let _ = win;
}

fn show_without_focus(win: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(windows)]
    {
        win32::show_no_activate(win);
        win32::ensure_toolwindow(win);
        Ok(())
    }
    #[cfg(not(windows))]
    win.show()
}

fn hide_window(win: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(windows)]
    {
        win32::hide(win);
        Ok(())
    }
    #[cfg(not(windows))]
    win.hide()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// Nama perangkat Windows (mis. `\\.\DISPLAY2`); disimpan di pengaturan.
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

fn monitor_id(monitor: &Monitor, index: usize) -> String {
    monitor.name().cloned().unwrap_or_else(|| format!("#{index}"))
}

/// Monitor yang dipilih di pengaturan; jika tidak ada (mis. dicabut), pakai monitor utama.
fn resolve_monitor(app: &AppHandle, wanted: Option<&str>) -> tauri::Result<Option<Monitor>> {
    if let Some(wanted) = wanted {
        let found = app
            .available_monitors()?
            .into_iter()
            .enumerate()
            .find(|(i, m)| monitor_id(m, *i) == wanted)
            .map(|(_, m)| m);
        if found.is_some() {
            return Ok(found);
        }
        log::warn!("monitor {wanted} tidak ditemukan, pakai monitor utama");
    }
    app.primary_monitor()
}

pub fn list_monitors(app: &AppHandle) -> tauri::Result<Vec<MonitorInfo>> {
    let primary_pos = app.primary_monitor()?.map(|m| *m.position());
    let mut monitors: Vec<(i32, MonitorInfo)> = app
        .available_monitors()?
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let size = *m.size();
            let primary = Some(*m.position()) == primary_pos;
            (
                m.position().x,
                MonitorInfo {
                    id: monitor_id(&m, i),
                    label: String::new(),
                    width: size.width,
                    height: size.height,
                    primary,
                },
            )
        })
        .collect();
    // Urut dari kiri ke kanan agar penomoran sesuai susunan fisik.
    monitors.sort_by_key(|(x, _)| *x);
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(n, (_, mut info))| {
            info.label = format!(
                "Monitor {} · {}×{}{}",
                n + 1,
                info.width,
                info.height,
                if info.primary { " (utama)" } else { "" }
            );
            info
        })
        .collect())
}

/// Memindahkan overlay agar menutupi monitor yang dipilih.
pub fn move_to_monitor(app: &AppHandle, wanted: Option<&str>) -> tauri::Result<()> {
    let win = window(app)?;
    if let Some(m) = resolve_monitor(app, wanted)? {
        // Posisi diset dua kali: pindah antar-monitor dengan DPI berbeda memicu
        // penyesuaian ukuran otomatis dari Windows di antaranya.
        win.set_position(*m.position())?;
        win.set_size(*m.size())?;
        win.set_position(*m.position())?;
    }
    if app.state::<OverlayManager>().status().visible {
        #[cfg(windows)]
        win32::assert_topmost(&win);
    }
    Ok(())
}

/// Membuat window overlay transparan seukuran monitor terpilih.
///
/// Dibuat langsung tampil dengan `focused(false)` agar tampilan pertama memakai
/// `SW_SHOWNOACTIVATE`. Setelah itu tampil/sembunyi ditangani `win32`.
pub fn create(app: &AppHandle, wanted_monitor: Option<&str>) -> tauri::Result<()> {
    let monitor = resolve_monitor(app, wanted_monitor)?;

    let mut builder =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
            .title("LiveStream Overlay (layer)")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .content_protected(true)
            .visible(true);

    if let Some(m) = &monitor {
        let scale = m.scale_factor();
        let pos = m.position().to_logical::<f64>(scale);
        let size = m.size().to_logical::<f64>(scale);
        builder = builder
            .position(pos.x, pos.y)
            .inner_size(size.width, size.height);
    }

    let win = builder.build()?;

    if let Some(m) = &monitor {
        // Set ulang dalam piksel fisik agar pas persis dengan monitor.
        win.set_position(*m.position())?;
        win.set_size(*m.size())?;
    }

    win.set_ignore_cursor_events(true)?;
    reapply_native_style(&win);

    app.state::<OverlayManager>().lock().status.visible = true;
    emit_status(app);
    Ok(())
}

pub fn set_visible(app: &AppHandle, visible: bool) -> tauri::Result<OverlayStatus> {
    let win = window(app)?;
    if !visible && app.state::<OverlayManager>().status().edit_mode {
        set_edit_mode(app, false)?;
    }

    if visible {
        show_without_focus(&win)?;
    } else {
        hide_window(&win)?;
    }
    app.state::<OverlayManager>().lock().status.visible = visible;
    Ok(emit_status(app))
}

pub fn toggle_visible(app: &AppHandle) -> tauri::Result<OverlayStatus> {
    let visible = app.state::<OverlayManager>().status().visible;
    set_visible(app, !visible)
}

pub fn set_edit_mode(app: &AppHandle, edit: bool) -> tauri::Result<OverlayStatus> {
    let win = window(app)?;
    let manager = app.state::<OverlayManager>();

    if manager.status().edit_mode == edit {
        return Ok(emit_status(app));
    }

    if edit {
        if !manager.status().visible {
            show_without_focus(&win)?;
            manager.lock().status.visible = true;
        }
        #[cfg(windows)]
        {
            let fg = win32::foreground();
            if !win32::is_same_window(&win, fg) {
                manager.lock().prev_foreground = fg;
            }
        }
        win.set_ignore_cursor_events(false)?;
        win.set_focusable(true)?;
        reapply_native_style(&win);
        win.set_focus()?;
    } else {
        win.set_ignore_cursor_events(true)?;
        win.set_focusable(false)?;
        reapply_native_style(&win);
        #[cfg(windows)]
        {
            let prev = std::mem::take(&mut manager.lock().prev_foreground);
            win32::restore_foreground(prev);
        }
    }

    manager.lock().status.edit_mode = edit;
    Ok(emit_status(app))
}

pub fn toggle_edit_mode(app: &AppHandle) -> tauri::Result<OverlayStatus> {
    let edit = app.state::<OverlayManager>().status().edit_mode;
    set_edit_mode(app, !edit)
}

pub fn set_content_protected(app: &AppHandle, enabled: bool) -> tauri::Result<OverlayStatus> {
    window(app)?.set_content_protected(enabled)?;
    app.state::<OverlayManager>().lock().status.content_protected = enabled;
    Ok(emit_status(app))
}

/// Sebagian game ikut mengklaim topmost; tegaskan ulang posisi overlay secara berkala.
pub fn spawn_topmost_watchdog(app: AppHandle) {
    std::thread::Builder::new()
        .name("overlay-topmost".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            let status = app.state::<OverlayManager>().status();
            if !status.visible || status.edit_mode {
                continue;
            }
            if let Some(_win) = app.get_webview_window(OVERLAY_LABEL) {
                #[cfg(windows)]
                {
                    win32::ensure_toolwindow(&_win);
                    win32::assert_topmost(&_win);
                }
            }
        })
        .expect("gagal membuat thread watchdog overlay");
}
