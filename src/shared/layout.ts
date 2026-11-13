import { load, type Store } from "@tauri-apps/plugin-store";
import type { Layout, WidgetId, WidgetRect } from "./types";

const STORE_FILE = "layout.json";
const KEY = "widgets";

/** Dikirim dashboard setelah reset, agar overlay memuat ulang layout. */
export const LAYOUT_RESET_EVENT = "layout://reset";

export const DEFAULT_LAYOUT: Layout = {
  // Bar status tipis di kiri atas.
  obsStatus: { x: 0.012, y: 0.015, w: 0.36, h: 0.06 },
  // Panel chat di kanan bawah.
  chat: { x: 0.75, y: 0.5, w: 0.235, h: 0.45 },
  // Alert di tengah atas, mudah terlihat tapi tidak menutupi tengah layar.
  alerts: { x: 0.33, y: 0.09, w: 0.34, h: 0.2 },
  // Donasi Trakteer di kanan atas, di atas panel chat.
  trakteer: { x: 0.75, y: 0.12, w: 0.235, h: 0.22 },
};

let storePromise: Promise<Store> | null = null;
function store(): Promise<Store> {
  storePromise ??= load(STORE_FILE, { autoSave: false, defaults: {} });
  return storePromise;
}

function isRect(v: unknown): v is WidgetRect {
  if (!v || typeof v !== "object") return false;
  const r = v as Record<string, unknown>;
  return ["x", "y", "w", "h"].every((k) => typeof r[k] === "number" && Number.isFinite(r[k]));
}

export async function loadLayout(): Promise<Layout> {
  try {
    const saved = await (await store()).get<Partial<Record<WidgetId, unknown>>>(KEY);
    const layout = { ...DEFAULT_LAYOUT };
    for (const id of Object.keys(DEFAULT_LAYOUT) as WidgetId[]) {
      const rect = saved?.[id];
      if (isRect(rect)) layout[id] = rect;
    }
    return layout;
  } catch (e) {
    console.warn("Gagal memuat layout, pakai default", e);
    return { ...DEFAULT_LAYOUT };
  }
}

export async function saveLayout(layout: Layout): Promise<void> {
  try {
    const s = await store();
    await s.set(KEY, layout);
    await s.save();
  } catch (e) {
    console.warn("Gagal menyimpan layout", e);
  }
}

export async function resetLayout(): Promise<Layout> {
  await saveLayout(DEFAULT_LAYOUT);
  return { ...DEFAULT_LAYOUT };
}
