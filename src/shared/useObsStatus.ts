import { useEffect, useState } from "react";
import { ipc, onObsNotice, onObsStatus } from "./ipc";
import type { ObsNotice, ObsStatus } from "./types";

/** Status OBS terkini: diambil sekali di awal, lalu diperbarui lewat event dari Rust (±1 Hz). */
export function useObsStatus(): ObsStatus | null {
  const [status, setStatus] = useState<ObsStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisten = onObsStatus((s) => {
      if (!disposed) setStatus(s);
    });
    ipc
      .obsGetStatus()
      .then((s) => {
        // Event yang datang lebih dulu lebih baru; jangan ditimpa hasil awal.
        if (!disposed) setStatus((prev) => prev ?? s);
      })
      .catch((e) => console.error("obs_get_status gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return status;
}

/** Notifikasi singkat dari Rust (mis. hasil hotkey), hilang sendiri setelah `ms`. */
export function useObsNotice(ms = 2500): ObsNotice | null {
  const [notice, setNotice] = useState<ObsNotice | null>(null);

  useEffect(() => {
    let disposed = false;
    let timer = 0;
    const unlisten = onObsNotice((n) => {
      if (disposed) return;
      setNotice(n);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setNotice(null), ms);
    });
    return () => {
      disposed = true;
      window.clearTimeout(timer);
      unlisten.then((fn) => fn());
    };
  }, [ms]);

  return notice;
}
