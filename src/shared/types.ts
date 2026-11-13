// Cermin tipe Rust (src-tauri/src/{overlay,hotkeys,settings}.rs, src-tauri/src/obs/status.rs).

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
  /** Nama input audio OBS yang dipantau sebagai mic. null = "Mic/Aux" global OBS. */
  micInput: string | null;
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
  toggleMic: string;
  toggleRecord: string;
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

// ---------- OBS ----------

export type ObsConnection = "connecting" | "connected" | "retrying" | "authFailed" | "failed";
export type OutputTransition = "starting" | "stopping";

export interface ObsStreamInfo {
  active: boolean;
  reconnecting: boolean;
  transition: OutputTransition | null;
  durationMs: number;
  /** Rata-rata 3 detik. null jika belum cukup sampel / tidak live. */
  bitrateKbps: number | null;
  /** Frame di-drop karena jaringan, 10 detik terakhir. */
  droppedPct: number | null;
  droppedFrames: number;
  totalFrames: number;
  congestion: number;
}

export interface ObsRecordInfo {
  active: boolean;
  paused: boolean;
  transition: OutputTransition | null;
  durationMs: number;
}

export interface ObsPerfInfo {
  fps: number;
  cpuPct: number;
  renderLagPct: number | null;
  encodeLagPct: number | null;
}

export interface ObsMicInfo {
  name: string;
  muted: boolean;
}

export interface ObsStatus {
  connection: ObsConnection;
  message: string | null;
  attempt: number;
  endpoint: string;
  obsVersion: string | null;
  stream: ObsStreamInfo;
  record: ObsRecordInfo;
  perf: ObsPerfInfo | null;
  scene: string | null;
  scenes: string[];
  mic: ObsMicInfo | null;
  micNote: string | null;
}

export interface ObsNotice {
  text: string;
  error: boolean;
}
