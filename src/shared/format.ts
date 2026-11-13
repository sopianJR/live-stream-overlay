import type { ChatState, ObsConnection, ObsStatus } from "./types";

const NUMBER = new Intl.NumberFormat("id-ID");

/** 3215 → "3.215" */
export function formatViewers(n: number): string {
  return NUMBER.format(n);
}

export const CHAT_STATE_LABEL: Record<ChatState, string> = {
  idle: "belum diatur",
  resolving: "mencari live…",
  waiting: "menunggu live",
  connected: "tersambung",
  retrying: "mencoba ulang",
  failed: "gagal",
};

/** 3723000 → "01:02:03" */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

/** 6000 → "6.0 Mbps", 850 → "850 kbps" */
export function formatBitrate(kbps: number | null): string {
  if (kbps === null) return "– kbps";
  if (kbps >= 1000) return `${(kbps / 1000).toFixed(1)} Mbps`;
  return `${Math.round(kbps)} kbps`;
}

export function formatPct(pct: number | null, digits = 1): string {
  return pct === null ? "–" : `${pct.toFixed(digits)}%`;
}

/** Ambang warna dropped frames; sama dengan ambang alert di rencana Fase 4. */
export function dropLevel(pct: number | null): "ok" | "warn" | "bad" {
  if (pct === null) return "ok";
  if (pct >= 5) return "bad";
  if (pct >= 1) return "warn";
  return "ok";
}

export const CONNECTION_LABEL: Record<ObsConnection, string> = {
  connecting: "Menghubungkan…",
  connected: "Terhubung",
  retrying: "Belum terhubung",
  authFailed: "Password salah",
  failed: "Gagal",
};

/** Label singkat keadaan stream untuk ditampilkan di bar status. */
export function streamLabel(status: ObsStatus): { text: string; tone: "live" | "warn" | "idle" } {
  const { stream } = status;
  if (stream.transition === "starting") return { text: "MEMULAI…", tone: "warn" };
  if (stream.transition === "stopping") return { text: "MENGHENTIKAN…", tone: "warn" };
  if (stream.active && stream.reconnecting) return { text: "RECONNECTING", tone: "warn" };
  if (stream.active) return { text: "LIVE", tone: "live" };
  return { text: "OFFLINE", tone: "idle" };
}
