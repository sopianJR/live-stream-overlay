import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, onSettingsChanged } from "./ipc";
import type { AppSettings } from "./types";

export type SaveSettings = (update: (current: AppSettings) => AppSettings) => Promise<AppSettings>;

/**
 * Pengaturan terkini dari Rust: diambil sekali, lalu diperbarui lewat event
 * `settings://changed` (termasuk perubahan dari window lain).
 */
export function useSettings(): { settings: AppSettings | null; save: SaveSettings } {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  // Ref agar penyimpanan beruntun (mis. slider dengan debounce) selalu memakai nilai terbaru.
  const latest = useRef<AppSettings | null>(null);

  const apply = useCallback((s: AppSettings) => {
    latest.current = s;
    setSettings(s);
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisten = onSettingsChanged((s) => {
      if (!disposed) apply(s);
    });
    ipc
      .settingsGet()
      .then((s) => {
        if (!disposed) apply(s);
      })
      .catch((e) => console.error("settings_get gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, [apply]);

  /** Simpan perubahan berbasis pengaturan terkini. Melempar pesan error dari Rust. */
  const save = useCallback<SaveSettings>(
    async (update) => {
      const current = latest.current ?? (await ipc.settingsGet());
      const saved = await ipc.settingsSave(update(current));
      apply(saved);
      return saved;
    },
    [apply],
  );

  return { settings, save };
}
