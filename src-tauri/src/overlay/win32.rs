//! Helper Win32 untuk window overlay.
//!
//! Pembagian tugas dengan Tauri/tao:
//! - Fokus (`WS_EX_NOACTIVATE`) dan tembus-klik (`WS_EX_TRANSPARENT`) dikelola tao lewat
//!   `set_focusable` dan `set_ignore_cursor_events`. Tao menghitung ulang seluruh
//!   `GWL_EXSTYLE` setiap kali flag-nya berubah.
//! - Bit yang tidak dikenal tao (`WS_EX_TOOLWINDOW`, agar tidak muncul di Alt+Tab)
//!   diterapkan ulang di sini setelah setiap perubahan flag.
//! - Tampil/sembunyi memakai `ShowWindow` langsung agar tidak pernah mengaktifkan overlay.
//!   Tao tetap menganggap window "visible", sehingga tao tidak ikut menyembunyikan
//!   atau menampilkannya.

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, IsWindow, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    // `.0` adalah pointer mentah; di-cast agar tidak bergantung pada versi crate `windows` milik Tauri.
    win.hwnd().ok().map(|h| h.0 as HWND)
}

/// Pastikan overlay tidak muncul di Alt+Tab maupun taskbar.
pub fn ensure_toolwindow(win: &WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let next = (current | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        if next != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Menampilkan overlay tanpa mengaktifkannya (fokus tetap di game).
pub fn show_no_activate(win: &WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    assert_topmost(win);
}

pub fn hide(win: &WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
}

/// Menegaskan ulang posisi topmost tanpa memindahkan fokus.
pub fn assert_topmost(win: &WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

/// HWND window yang sedang fokus (biasanya game), disimpan sebagai angka agar `Send`.
pub fn foreground() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

pub fn is_same_window(win: &WebviewWindow, raw: isize) -> bool {
    hwnd_of(win).map(|h| h as isize == raw).unwrap_or(false)
}

/// Mengembalikan fokus ke window sebelumnya (game) setelah keluar dari mode edit.
pub fn restore_foreground(raw: isize) {
    if raw == 0 {
        return;
    }
    let hwnd = raw as HWND;
    unsafe {
        if IsWindow(hwnd) != 0 {
            SetForegroundWindow(hwnd);
        }
    }
}
