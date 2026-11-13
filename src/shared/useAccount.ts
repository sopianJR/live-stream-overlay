import { useEffect, useState } from "react";
import { ipc, onAccountStatus } from "./ipc";
import type { AccountStatus } from "./types";

/** Status login Google: diambil di awal, lalu mengikuti event dari Rust. */
export function useAccount(): AccountStatus | null {
  const [status, setStatus] = useState<AccountStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisten = onAccountStatus((s) => {
      if (!disposed) setStatus(s);
    });
    ipc
      .accountGetStatus()
      .then((s) => {
        if (!disposed) setStatus((prev) => prev ?? s);
      })
      .catch((e) => console.error("account_get_status gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return status;
}
