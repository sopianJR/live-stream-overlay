import { useEffect, useRef, useState } from "react";
import { ipc, onTrakteerDonation, onTrakteerRecent, onTrakteerStatus } from "./ipc";
import type { DonationEvent, TrakteerStatus } from "./types";

/** Status koneksi Trakteer + riwayat donasi (untuk dashboard & onboarding). */
export function useTrakteer(): { status: TrakteerStatus | null; recent: DonationEvent[] } {
  const [status, setStatus] = useState<TrakteerStatus | null>(null);
  const [recent, setRecent] = useState<DonationEvent[]>([]);

  useEffect(() => {
    let disposed = false;
    let gotRecent = false;
    const unStatus = onTrakteerStatus((s) => {
      if (!disposed) setStatus(s);
    });
    const unRecent = onTrakteerRecent((list) => {
      if (disposed) return;
      gotRecent = true;
      setRecent(list);
    });
    ipc
      .trakteerGetState()
      .then((snap) => {
        if (disposed) return;
        setStatus((prev) => prev ?? snap.status);
        if (!gotRecent) setRecent(snap.recent);
      })
      .catch((e) => console.error("trakteer_get_state gagal", e));
    return () => {
      disposed = true;
      unStatus.then((fn) => fn());
      unRecent.then((fn) => fn());
    };
  }, []);

  return { status, recent };
}

export type DonationPhase = "idle" | "entering" | "showing" | "exiting";

const ENTER_MS = 350;
const EXIT_MS = 350;
/** Antrean dibatasi agar banjir donasi tidak menumpuk berjam-jam. */
const MAX_QUEUE = 30;
/** Jika antrean sepanjang ini, durasi tiap kartu dipersingkat. */
const BUSY_QUEUE = 5;

/**
 * Antrean kartu donasi: satu kartu tampil pada satu waktu, berurutan. Diadaptasi dari
 * `useTrakteerAlertQueue` di trakteer-live-suite (timer kini dibersihkan saat unmount).
 */
export function useDonationQueue(alertSeconds: number) {
  const [current, setCurrent] = useState<DonationEvent | null>(null);
  const [phase, setPhase] = useState<DonationPhase>("idle");
  const [pending, setPending] = useState(0);
  const queue = useRef<DonationEvent[]>([]);
  const busy = useRef(false);
  const seconds = useRef(alertSeconds);
  seconds.current = alertSeconds;

  useEffect(() => {
    let disposed = false;
    const timers = new Set<number>();
    const later = (ms: number, fn: () => void) => {
      const id = window.setTimeout(() => {
        timers.delete(id);
        if (!disposed) fn();
      }, ms);
      timers.add(id);
    };

    const next = () => {
      const item = queue.current.shift();
      setPending(queue.current.length);
      if (!item) {
        busy.current = false;
        setCurrent(null);
        setPhase("idle");
        return;
      }
      busy.current = true;
      setCurrent(item);
      setPhase("entering");
      const base = Math.max(3, seconds.current) * 1000;
      const show = queue.current.length >= BUSY_QUEUE ? Math.max(3000, base / 2) : base;
      later(ENTER_MS, () => {
        setPhase("showing");
        later(show, () => {
          setPhase("exiting");
          later(EXIT_MS, next);
        });
      });
    };

    const unlisten = onTrakteerDonation((event) => {
      if (disposed) return;
      queue.current.push(event);
      if (queue.current.length > MAX_QUEUE) {
        queue.current.splice(0, queue.current.length - MAX_QUEUE);
      }
      setPending(queue.current.length);
      if (!busy.current) next();
    });

    return () => {
      disposed = true;
      timers.forEach((id) => window.clearTimeout(id));
      unlisten.then((fn) => fn());
    };
  }, []);

  return { current, phase, pending };
}
