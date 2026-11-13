// Cermin tipe Rust (src-tauri/src/{overlay,hotkeys,settings}.rs, obs/status.rs, youtube/*.rs,
// alerts/engine.rs, trakteer/*.rs).

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

export interface AlertSettings {
  enabled: boolean;
  /** Peringatan jika frame di-drop (10 detik terakhir) mencapai persentase ini. */
  droppedWarnPct: number;
  /** Alert kritis (menetap) mulai persentase ini. */
  droppedCriticalPct: number;
  /** Persentase dari bitrate normal; 0 = mati. */
  lowBitratePct: number;
  /** CPU OBS; 0 = mati. */
  cpuPct: number;
  micReminder: boolean;
}

export interface TrakteerSettings {
  /** Lama satu kartu donasi tampil (3–60 detik). */
  alertSeconds: number;
  /** 0 = semua donasi ditampilkan. */
  minAmountRp: number;
  showTest: boolean;
  showMessage: boolean;
}

export interface AppSettings {
  obs: ObsSettings;
  youtube: YoutubeSettings;
  overlay: OverlaySettings;
  hotkeys: HotkeySettings;
  alerts: AlertSettings;
  trakteer: TrakteerSettings;
}

export type WidgetId = "obsStatus" | "chat" | "alerts" | "trakteer";

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

// ---------- Chat YouTube ----------

export type ChatRun = { type: "text"; text: string } | { type: "emoji"; alt: string; url: string };

export type ChatMessageKind =
  | "text"
  | "superChat"
  | "superSticker"
  | "membership"
  | "giftPurchase"
  | "giftRedemption";

export interface ChatAuthor {
  name: string;
  channelId: string;
  photo: string | null;
  owner: boolean;
  moderator: boolean;
  verified: boolean;
  member: boolean;
  memberBadge: string | null;
  memberLabel: string | null;
}

/** Warna CSS dari YouTube sesuai tier Super Chat. */
export interface PaidColors {
  headerBg: string;
  headerText: string;
  bodyBg: string;
  bodyText: string;
}

export interface ChatMessage {
  id: string;
  kind: ChatMessageKind;
  author: ChatAuthor;
  runs: ChatRun[];
  timestampMs: number;
  amount: string | null;
  header: string | null;
  sticker: string | null;
  colors: PaidColors | null;
}

export type ChatState = "idle" | "resolving" | "waiting" | "connected" | "retrying" | "failed";

export interface ChatStatus {
  state: ChatState;
  message: string | null;
  source: string;
  videoId: string | null;
  title: string | null;
  channel: string | null;
  viewers: number | null;
}

export interface ChatUpdate {
  /** Buang semua pesan lama dulu (pindah ke live lain). */
  reset: boolean;
  upserts: ChatMessage[];
  removedIds: string[];
  removedAuthors: string[];
}

export interface ChatSnapshot {
  status: ChatStatus;
  messages: ChatMessage[];
}

// ---------- Alert ----------

export type AlertLevel = "info" | "reminder" | "warning" | "critical";

export interface Alert {
  id: string;
  level: AlertLevel;
  title: string;
  detail: string | null;
  raisedAtMs: number;
  /** Naik setiap kali alert (di)tampilkan. */
  seq: number;
}

// ---------- Akun Google (balas chat) ----------

export type AccountState = "signedOut" | "signingIn" | "signedIn";

export interface AccountStatus {
  /** OAuth client (Client ID + secret) sudah diisi. */
  hasClient: boolean;
  state: AccountState;
  channelTitle: string | null;
  message: string | null;
}

// ---------- Trakteer ----------

export type TrakteerState = "idle" | "connecting" | "connected" | "retrying" | "failed";
export type TrakteerProtocol = "V1" | "V2";

export interface TrakteerStatus {
  state: TrakteerState;
  message: string | null;
  protocol: TrakteerProtocol | null;
  attempt: number;
}

export interface TrakteerDonation {
  supporterName: string;
  unit: string;
  quantity: number;
  amountRp: number;
  /** mis. "Rp 5.000" */
  priceText: string;
  message: string;
  avatarUrl: string;
  unitIconUrl: string | null;
}

/** live = donasi asli, test = "Test alert" dari dashboard Trakteer, sample = contoh lokal. */
export type DonationKind = "live" | "test" | "sample";

export interface DonationEvent {
  id: string;
  kind: DonationKind;
  donation: TrakteerDonation;
  receivedAtMs: number;
}

export interface TrakteerSnapshot {
  status: TrakteerStatus;
  recent: DonationEvent[];
}

export interface TrakteerKeyStatus {
  hasKey: boolean;
  protocol: TrakteerProtocol | null;
}
