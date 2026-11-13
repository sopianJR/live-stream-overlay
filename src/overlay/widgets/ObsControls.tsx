import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { CONNECTION_LABEL } from "../../shared/format";
import { errorText, ipc } from "../../shared/ipc";
import type { ObsStatus } from "../../shared/types";

/**
 * Tombol yang harus ditahan beberapa saat sebelum aksinya berjalan.
 * Dipakai untuk mulai/stop stream agar tidak terpicu karena salah klik.
 */
function HoldButton({
  holdMs,
  disabled,
  className,
  onConfirm,
  children,
}: {
  holdMs: number;
  disabled: boolean;
  className: string;
  onConfirm: () => void;
  children: ReactNode;
}) {
  const [progress, setProgress] = useState(0);
  const frame = useRef(0);

  const cancel = () => {
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    setProgress(0);
  };

  const begin = () => {
    if (disabled || frame.current) return;
    const started = performance.now();
    const step = (now: number) => {
      const p = Math.min(1, (now - started) / holdMs);
      setProgress(p);
      if (p >= 1) {
        cancel();
        onConfirm();
      } else {
        frame.current = requestAnimationFrame(step);
      }
    };
    frame.current = requestAnimationFrame(step);
  };

  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  return (
    <button
      type="button"
      className={`hold-btn ${className}`}
      disabled={disabled}
      style={{ "--hold": progress } as CSSProperties}
      onPointerDown={begin}
      onPointerUp={cancel}
      onPointerLeave={cancel}
      onPointerCancel={cancel}
      onKeyDown={(e) => {
        if ((e.key === " " || e.key === "Enter") && !e.repeat) begin();
      }}
      onKeyUp={cancel}
      onBlur={cancel}
    >
      <span className="hold-fill" />
      <span className="hold-text">{children}</span>
    </button>
  );
}

/** Kontrol OBS; hanya tampil di mode edit (overlay sedang bisa diklik). */
export function ObsControls({ status }: { status: ObsStatus }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (status.connection !== "connected") {
    return (
      <div className="obs-controls no-drag">
        <p className="obs-msg first">{status.message ?? CONNECTION_LABEL[status.connection]}</p>
        <div className="obs-controls-row">
          <button type="button" disabled={busy} onClick={() => void run(ipc.obsReconnect)}>
            Hubungkan ulang
          </button>
        </div>
      </div>
    );
  }

  const { stream, record, mic } = status;
  const streamBusy = busy || stream.transition !== null;

  return (
    <div className="obs-controls no-drag">
      <div className="obs-controls-row">
        {stream.active ? (
          <HoldButton
            holdMs={1500}
            className="danger"
            disabled={streamBusy}
            onConfirm={() => void run(ipc.obsStopStream)}
          >
            Tahan untuk STOP stream
          </HoldButton>
        ) : (
          <HoldButton
            holdMs={1000}
            className="go"
            disabled={streamBusy}
            onConfirm={() => void run(ipc.obsStartStream)}
          >
            Tahan untuk mulai stream
          </HoldButton>
        )}

        <button
          type="button"
          disabled={busy || record.transition !== null}
          onClick={() => void run(ipc.obsToggleRecord)}
        >
          {record.active ? "■ Stop rekam" : "● Rekam"}
        </button>

        <button
          type="button"
          disabled={busy || !mic}
          title={mic ? `Input: ${mic.name}` : (status.micNote ?? "")}
          onClick={() => void run(ipc.obsToggleMic)}
        >
          {mic?.muted ? "🎙 Unmute mic" : "🔇 Mute mic"}
        </button>

        <select
          aria-label="Scene aktif"
          value={status.scene ?? ""}
          disabled={busy || status.scenes.length === 0}
          onChange={(e) => {
            const name = e.target.value;
            void run(() => ipc.obsSetScene(name));
          }}
        >
          {status.scene === null && <option value="">Pilih scene</option>}
          {status.scenes.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </div>
      {!mic && status.micNote && <p className="obs-msg">{status.micNote}</p>}
      {error && <p className="obs-msg bad">{error}</p>}
    </div>
  );
}
