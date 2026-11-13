// Cermin tipe Rust (src-tauri/src/{overlay,hotkeys,settings}.rs).

export interface OverlayStatus {
  visible: boolean;
  editMode: boolean;
  contentProtected: boolean;
}

export type HotkeyId = keyof HotkeySettings;

export interface HotkeyInfo {
  id: HotkeyId;
  label: string;
  accelerator: string;
  registered: boolean;
  error: string | null;
}

export interface MonitorInfo {
  id: string;
  label: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface ObsSettings {
  host: string;
  port: number;
}

export interface YoutubeSettings {
  source: string;
}

export interface OverlaySettings {
  /** null = monitor utama */
  monitor: string | null;
  widgetOpacity: number;
  fontScale: number;
  /** 0 = pesan tidak pernah memudar */
  chatFadeSeconds: number;
}

export interface HotkeySettings {
  toggleOverlay: string;
  toggleEdit: string;
}

export interface AppSettings {
  obs: ObsSettings;
  youtube: YoutubeSettings;
  overlay: OverlaySettings;
  hotkeys: HotkeySettings;
}

export type WidgetId = "obsStatus" | "chat";

/** Posisi & ukuran widget dalam pecahan viewport (0..1), agar tidak bergantung resolusi. */
export interface WidgetRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Layout = Record<WidgetId, WidgetRect>;
