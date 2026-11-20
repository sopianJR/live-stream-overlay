import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountStatus,
  Alert,
  AppSettings,
  DonationEvent,
  MicAudio,
  SessionState,
  SessionSummary,
  TrakteerKeyStatus,
  TrakteerSnapshot,
  TrakteerStatus,
  ChatSnapshot,
  ChatStatus,
  ChatUpdate,
  HotkeyInfo,
  MonitorInfo,
  ObsNotice,
  ObsStatus,
  OverlayStatus,
} from "./types";

export const OVERLAY_STATE_EVENT = "overlay://state";
export const SETTINGS_CHANGED_EVENT = "settings://changed";
export const OBS_STATUS_EVENT = "obs://status";
export const OBS_NOTICE_EVENT = "obs://notice";
export const CHAT_STATUS_EVENT = "chat://status";
export const CHAT_UPDATE_EVENT = "chat://update";
export const ALERT_UPDATE_EVENT = "alert://update";
export const ACCOUNT_STATUS_EVENT = "account://status";
export const TRAKTEER_STATUS_EVENT = "trakteer://status";
export const TRAKTEER_DONATION_EVENT = "trakteer://donation";
export const SESSION_SUMMARY_EVENT = "session://summary";
export const TRAKTEER_RECENT_EVENT = "trakteer://recent";

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

  // OBS
  obsGetStatus: () => invoke<ObsStatus>("obs_get_status"),
  obsReconnect: () => invoke<void>("obs_reconnect"),
  /** Level mic terkini (meter "Tes mic"). */
  obsMicLevel: () => invoke<MicAudio>("obs_mic_level"),
  obsStartStream: () => invoke<void>("obs_start_stream"),
  /** Tanpa konfirmasi di sisi Rust: UI wajib meminta konfirmasi (tahan tombol) dulu. */
  obsStopStream: () => invoke<void>("obs_stop_stream"),
  /** true = rekaman kini berjalan. */
  obsToggleRecord: () => invoke<boolean>("obs_toggle_record"),
  /** true = mic kini di-mute. */
  obsToggleMic: () => invoke<boolean>("obs_toggle_mic"),
  obsSetScene: (name: string) => invoke<void>("obs_set_scene", { name }),
  obsListAudioInputs: () => invoke<string[]>("obs_list_audio_inputs"),

  // Chat YouTube
  chatGetSnapshot: () => invoke<ChatSnapshot>("chat_get_snapshot"),
  chatReconnect: () => invoke<void>("chat_reconnect"),
  /** Mengirim pesan ke live chat yang sedang tampil (butuh login Google, 50 unit kuota). */
  chatSend: (text: string) => invoke<void>("chat_send", { text }),

  // Akun Google (client secret & token tidak pernah dikirim balik)
  accountGetStatus: () => invoke<AccountStatus>("account_get_status"),
  accountSetClient: (clientId: string, clientSecret: string) =>
    invoke<AccountStatus>("account_set_client", { clientId, clientSecret }),
  accountClearClient: () => invoke<AccountStatus>("account_clear_client"),
  /** Membuka browser; selesai setelah login selesai, gagal, atau dibatalkan. */
  accountLogin: () => invoke<AccountStatus>("account_login"),
  accountCancelLogin: () => invoke<void>("account_cancel_login"),
  accountLogout: () => invoke<AccountStatus>("account_logout"),

  // Trakteer (kunci tidak pernah dikirim balik ke frontend)
  trakteerGetState: () => invoke<TrakteerSnapshot>("trakteer_get_state"),
  trakteerKeyStatus: () => invoke<TrakteerKeyStatus>("trakteer_key_status"),
  trakteerSetKey: (key: string) => invoke<TrakteerKeyStatus>("trakteer_set_key", { key }),
  trakteerClearKey: () => invoke<TrakteerKeyStatus>("trakteer_clear_key"),
  trakteerReconnect: () => invoke<void>("trakteer_reconnect"),
  /** Donasi contoh di overlay (tidak masuk riwayat). */
  trakteerTestDonation: () => invoke<void>("trakteer_test_donation"),

  // Ringkasan sesi
  /** `takeFocus`: ambil penanda "tampilkan sesi yang baru selesai" (sekali saja). */
  sessionGetState: (takeFocus = false) => invoke<SessionState>("session_get_state", { takeFocus }),
  sessionDelete: (id: string) => invoke<SessionState>("session_delete", { id }),
  sessionClear: () => invoke<SessionState>("session_clear"),

  // Alert
  alertsGet: () => invoke<Alert[]>("alerts_get"),
  alertsDismiss: (id: string) => invoke<Alert[]>("alerts_dismiss", { id }),
  /** Contoh alert selama 10 detik. */
  alertsTest: () => invoke<Alert[]>("alerts_test"),

  appQuit: () => invoke<void>("app_quit"),
  /** Membuka folder log di File Explorer. */
  appOpenLogFolder: () => invoke<void>("app_open_log_folder"),
};

export function onOverlayStatus(cb: (status: OverlayStatus) => void): Promise<UnlistenFn> {
  return listen<OverlayStatus>(OVERLAY_STATE_EVENT, (e) => cb(e.payload));
}

export function onObsStatus(cb: (status: ObsStatus) => void): Promise<UnlistenFn> {
  return listen<ObsStatus>(OBS_STATUS_EVENT, (e) => cb(e.payload));
}

export function onObsNotice(cb: (notice: ObsNotice) => void): Promise<UnlistenFn> {
  return listen<ObsNotice>(OBS_NOTICE_EVENT, (e) => cb(e.payload));
}

export function onChatStatus(cb: (status: ChatStatus) => void): Promise<UnlistenFn> {
  return listen<ChatStatus>(CHAT_STATUS_EVENT, (e) => cb(e.payload));
}

export function onChatUpdate(cb: (update: ChatUpdate) => void): Promise<UnlistenFn> {
  return listen<ChatUpdate>(CHAT_UPDATE_EVENT, (e) => cb(e.payload));
}

export function onAlerts(cb: (alerts: Alert[]) => void): Promise<UnlistenFn> {
  return listen<Alert[]>(ALERT_UPDATE_EVENT, (e) => cb(e.payload));
}

export function onAccountStatus(cb: (status: AccountStatus) => void): Promise<UnlistenFn> {
  return listen<AccountStatus>(ACCOUNT_STATUS_EVENT, (e) => cb(e.payload));
}

export function onTrakteerStatus(cb: (status: TrakteerStatus) => void): Promise<UnlistenFn> {
  return listen<TrakteerStatus>(TRAKTEER_STATUS_EVENT, (e) => cb(e.payload));
}

export function onTrakteerDonation(cb: (event: DonationEvent) => void): Promise<UnlistenFn> {
  return listen<DonationEvent>(TRAKTEER_DONATION_EVENT, (e) => cb(e.payload));
}

export function onSessionSummary(cb: (summary: SessionSummary) => void): Promise<UnlistenFn> {
  return listen<SessionSummary>(SESSION_SUMMARY_EVENT, (e) => cb(e.payload));
}

export function onTrakteerRecent(cb: (recent: DonationEvent[]) => void): Promise<UnlistenFn> {
  return listen<DonationEvent[]>(TRAKTEER_RECENT_EVENT, (e) => cb(e.payload));
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
