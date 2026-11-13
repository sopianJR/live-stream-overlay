import { useEffect, useState } from "react";
import { ipc, onOverlayStatus } from "./ipc";
import type { OverlayStatus } from "./types";

/** Status overlay terkini: diambil sekali di awal, lalu diperbarui lewat event dari Rust. */
export function useOverlayStatus(): OverlayStatus | null {
  const [status, setStatus] = useState<OverlayStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisten = onOverlayStatus((s) => {
      if (!disposed) setStatus(s);
    });
    ipc
      .overlayGetStatus()
      .then((s) => {
        if (!disposed) setStatus(s);
      })
      .catch((e) => console.error("overlay_get_status gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return status;
}
