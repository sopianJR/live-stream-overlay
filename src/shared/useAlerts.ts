import { useEffect, useState } from "react";
import { ipc, onAlerts } from "./ipc";
import type { Alert, AlertLevel } from "./types";

export const ALERT_ICON: Record<AlertLevel, string> = {
  critical: "⛔",
  warning: "⚠️",
  reminder: "🔔",
  info: "ℹ️",
};

export const ALERT_LEVEL_LABEL: Record<AlertLevel, string> = {
  critical: "Kritis",
  warning: "Peringatan",
  reminder: "Pengingat",
  info: "Info",
};

/** Alert aktif (paling penting dulu): diambil di awal, lalu mengikuti event dari Rust. */
export function useAlerts(): Alert[] {
  const [alerts, setAlerts] = useState<Alert[]>([]);

  useEffect(() => {
    let disposed = false;
    let gotEvent = false;
    const unlisten = onAlerts((list) => {
      if (disposed) return;
      gotEvent = true;
      setAlerts(list);
    });
    ipc
      .alertsGet()
      .then((list) => {
        if (!disposed && !gotEvent) setAlerts(list);
      })
      .catch((e) => console.error("alerts_get gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return alerts;
}
