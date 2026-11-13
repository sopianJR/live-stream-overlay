import {
  CONNECTION_LABEL,
  dropLevel,
  formatBitrate,
  formatDuration,
  formatPct,
  formatViewers,
  streamLabel,
} from "../../shared/format";
import type { ObsStatus } from "../../shared/types";
import { useChatStatus } from "../../shared/useChat";
import { useObsNotice, useObsStatus } from "../../shared/useObsStatus";
import { ObsControls } from "./ObsControls";

/** Teks singkat untuk bar saat belum terhubung; penjelasan lengkap ada di mode edit & dashboard. */
const OFFLINE_TEXT = {
  connecting: "Menghubungkan ke OBS…",
  retrying: "OBS belum terhubung",
  authFailed: "Password OBS salah",
  failed: "Koneksi OBS gagal",
} as const;

function ConnectedBar({ status }: { status: ObsStatus }) {
  const { stream, record, perf, mic } = status;
  const label = streamLabel(status);

  return (
    <>
      {label.tone === "live" ? (
        <span className="obs-live">
          <span className="live-dot" /> LIVE <span className="mono">{formatDuration(stream.durationMs)}</span>
        </span>
      ) : (
        <span className={`obs-state ${label.tone}`}>
          {label.text}
          {stream.active && <span className="mono"> {formatDuration(stream.durationMs)}</span>}
        </span>
      )}

      {stream.active && (
        <>
          <span className="obs-item mono">{formatBitrate(stream.bitrateKbps)}</span>
          <span
            className={`obs-item mono ${dropLevel(stream.droppedPct)}`}
            title={`${stream.droppedFrames} dari ${stream.totalFrames} frame di-drop sejak mulai`}
          >
            drop {formatPct(stream.droppedPct)}
          </span>
        </>
      )}

      {perf && <span className="obs-item mono">{Math.round(perf.fps)} fps</span>}

      {record.active && (
        <span className={record.paused ? "obs-item warn" : "obs-item rec"}>
          {record.paused ? "⏸ REC" : "● REC"} <span className="mono">{formatDuration(record.durationMs)}</span>
        </span>
      )}

      {mic ? (
        <span className={mic.muted ? "obs-item bad" : "obs-item"} title={mic.name}>
          {mic.muted ? "🔇 mic mute" : "🎙 mic"}
        </span>
      ) : (
        <span className="obs-item dim" title={status.micNote ?? ""}>
          🎙 ?
        </span>
      )}

      {status.scene && (
        <span className="obs-item scene" title="Scene aktif">
          {status.scene}
        </span>
      )}
    </>
  );
}

export function ObsStatusWidget({ editMode }: { editMode: boolean }) {
  const status = useObsStatus();
  const notice = useObsNotice();
  const chat = useChatStatus();
  const connected = status?.connection === "connected";
  const viewers = chat?.state === "connected" ? chat.viewers : null;

  return (
    <div className="obs-widget">
      <div className="obs-bar">
        {!status ? (
          <span className="obs-item dim">Memuat status OBS…</span>
        ) : connected ? (
          <ConnectedBar status={status} />
        ) : (
          <>
            <span className={`obs-conn ${status.connection}`} title={CONNECTION_LABEL[status.connection]}>
              ● OBS
            </span>
            <span className="obs-item">{OFFLINE_TEXT[status.connection as keyof typeof OFFLINE_TEXT]}</span>
          </>
        )}
        {viewers !== null && (
          <span className="obs-item mono" title="Penonton YouTube saat ini">
            👁 {formatViewers(viewers)}
          </span>
        )}
      </div>

      {(notice || (editMode && status)) && (
        <div className="obs-below">
          {notice && <div className={notice.error ? "obs-notice bad" : "obs-notice"}>{notice.text}</div>}
          {editMode && status && <ObsControls status={status} />}
        </div>
      )}
    </div>
  );
}
