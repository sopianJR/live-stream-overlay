import { useCallback, useState } from "react";
import { errorText } from "../shared/ipc";
import type { AppSettings } from "../shared/types";
import type { SaveSettings } from "../shared/useSettings";

export interface TabProps {
  settings: AppSettings;
  save: SaveSettings;
}

export type Feedback = { kind: "ok" | "error"; text: string } | null;

/** Menjalankan aksi async dan menyimpan pesan sukses/gagal untuk ditampilkan. */
export function useFeedback() {
  const [feedback, setFeedback] = useState<Feedback>(null);
  const [busy, setBusy] = useState(false);

  const run = useCallback(async (action: () => Promise<unknown>, okText?: string) => {
    setBusy(true);
    setFeedback(null);
    try {
      await action();
      if (okText) setFeedback({ kind: "ok", text: okText });
      return true;
    } catch (e) {
      setFeedback({ kind: "error", text: errorText(e) });
      return false;
    } finally {
      setBusy(false);
    }
  }, []);

  return { feedback, setFeedback, busy, run };
}

export function FeedbackLine({ feedback }: { feedback: Feedback }) {
  if (!feedback) return null;
  return (
    <p className={`msg ${feedback.kind}`} role={feedback.kind === "error" ? "alert" : "status"}>
      {feedback.text}
    </p>
  );
}

export function Chip({
  ok,
  on,
  off,
  warnWhenOff = false,
}: {
  ok: boolean;
  on: string;
  off: string;
  warnWhenOff?: boolean;
}) {
  const cls = ok ? "chip ok" : warnWhenOff ? "chip warn" : "chip";
  return <span className={cls}>{ok ? on : off}</span>;
}
