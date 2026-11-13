import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSettings, HotkeyInfo, MonitorInfo, OverlayStatus } from "./types";

export const OVERLAY_STATE_EVENT = "overlay://state";
export const SETTINGS_CHANGED_EVENT = "settings://changed";

export const ipc = {
  // Overlay
  overlayGetStatus: () => invoke<OverlayStatus>("overlay_get_status"),
  overlaySetVisible: (visible: boolean) =>
    invoke<OverlayStatus>("overlay_set_visible", { visible }),
  overlayToggleVisible: () => invoke<OverlayStatus>("overlay_toggle_visible"),
  overlaySetEditMode: (edit: boolean) =>
    invoke<OverlayStatus>("overlay_set_edit_mode", { edit }),
  overlaySetContentProtected: (enabled: boolean) =>
    invoke<OverlayStatus>("overlay_set_content_protected", { enabled }),
  monitorsList: () => invoke<MonitorInfo[]>("monitors_list"),

  // Pengaturan
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsSave: (settings: AppSettings) => invoke<AppSettings>("settings_save", { settings }),

  // Hotkey
  hotkeysGet: () => invoke<HotkeyInfo[]>("hotkeys_get"),
  hotkeysSetSuspended: (suspended: boolean) =>
    invoke<HotkeyInfo[]>("hotkeys_set_suspended", { suspended }),

  // Rahasia (password tidak pernah dikirim balik ke frontend)
  obsPasswordStatus: () => invoke<boolean>("obs_password_status"),
  obsPasswordSet: (password: string) => invoke<void>("obs_password_set", { password }),
  obsPasswordClear: () => invoke<void>("obs_password_clear"),

  appQuit: () => invoke<void>("app_quit"),
};

export function onOverlayStatus(cb: (status: OverlayStatus) => void): Promise<UnlistenFn> {
  return listen<OverlayStatus>(OVERLAY_STATE_EVENT, (e) => cb(e.payload));
}

export function onSettingsChanged(cb: (settings: AppSettings) => void): Promise<UnlistenFn> {
  return listen<AppSettings>(SETTINGS_CHANGED_EVENT, (e) => cb(e.payload));
}

/** Pesan error dari Rust berupa string; kesalahan lain diubah ke teks. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
