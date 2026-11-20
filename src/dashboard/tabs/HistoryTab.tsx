import { useCallback, useEffect, useState } from "react";
import { formatBitrate, formatViewers } from "../../shared/format";
import { errorText, ipc, onSessionSummary } from "../../shared/ipc";
import type { SessionEndReason, SessionState, SessionSummary } from "../../shared/types";
import { ALERT_ICON, ALERT_LEVEL_LABEL } from "../../shared/useAlerts";
import { MinuteChart } from "../MinuteChart";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

const LIVE_REFRESH_MS = 5_000;
const CURRENT = "current";

const DATE = new Intl.DateTimeFormat("id-ID", {
  weekday: "long",
  day: "numeric",
  month: "long",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});
const SHORT_DATE = new Intl.DateTimeFormat("id-ID", {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});
const RUPIAH = new Intl.NumberFormat("id-ID");

const REASON: Record<SessionEndReason, { text: string; cls: string }> = {
  stopped: { text: "Selesai", cls: "chip ok" },
  lost: { text: "Stream putus", cls: "chip bad" },
  appClosed: { text: "Aplikasi ditutup", cls: "chip warn" },
};

/** 5520000 → "1 jam 32 menit" (sama dengan teks di Rust). */
export function durationText(ms: number): string {
  const minutes = Math.floor(ms / 60_000);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0 && m === 0) return `${Math.floor(ms / 1000)} detik`;
  if (h === 0) return `${m} menit`;
  return m === 0 ? `${h} jam` : `${h} jam ${m} menit`;
}

function rupiah(n: number): string {
  return `Rp ${RUPIAH.format(n)}`;
}

/** Detik sejak mulai → "1:05:09" / "5:09". */
function offset(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function HistoryTab({ settings, save }: TabProps) {
  const [state, setState] = useState<SessionState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (takeFocus: boolean) => {
    try {
      const next = await ipc.sessionGetState(takeFocus);
      setState(next);
      setError(null);
      if (next.focusId) setSelected(next.focusId);
      return next;
    } catch (e) {
      setError(errorText(e));
      return null;
    }
  }, []);

  useEffect(() => {
    void load(true);
    const unlisten = onSessionSummary((s) => {
      setSelected(s.id);
      void load(true);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  // Angka sesi yang sedang live diperbarui berkala selama tab ini terbuka.
  const live = state?.current !== null && state?.current !== undefined;
  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => void load(false), LIVE_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [live, load]);

  const history = state?.history ?? [];
  const current = state?.current ?? null;
  const pick = selected ?? (current ? CURRENT : history[0]?.id ?? null);
  // Sesi yang dipilih sudah dihapus / sudah selesai: tampilkan yang sedang live atau yang terbaru.
  const shown =
    (pick === CURRENT ? current : history.find((s) => s.id === pick)) ?? current ?? history[0] ?? null;

  return (
    <div className="grid">
      <section className="card wide">
        {error && <p className="msg error">{error}</p>}
        {!state ? (
          <p className="muted">Memuat riwayat…</p>
        ) : shown ? (
          <SummaryView summary={shown} />
        ) : (
          <>
            <h2>Ringkasan sesi</h2>
            <p className="muted">
              Belum ada sesi. Ringkasan dibuat otomatis setiap kali Anda live lewat OBS: durasi, penonton, chat,
              donasi Trakteer, dan masalah stream.
            </p>
          </>
        )}
      </section>

      <SessionListCard
        current={current}
        history={history}
        selected={shown ? (shown === current ? CURRENT : shown.id) : null}
        onSelect={setSelected}
        onChange={setState}
      />
      <SessionSettingsCard settings={settings} save={save} />
    </div>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="stat">
      <span className="stat-label">{label}</span>
      <strong className="stat-value">{value}</strong>
      {hint && <small className="muted">{hint}</small>}
    </div>
  );
}

function SummaryView({ summary: s }: { summary: SessionSummary }) {
  const reason = s.endReason ? REASON[s.endReason] : { text: "Sedang live", cls: "chip live" };
  const viewers = s.timeline.map((m) => ({ minute: m.minute, value: m.viewers }));
  const bitrate = s.timeline.map((m) => ({ minute: m.minute, value: m.bitrateKbps }));
  const hasViewers = viewers.some((v) => v.value !== null);

  return (
    <div className="summary">
      <div className="summary-head">
        <div>
          <h2>{DATE.format(new Date(s.startedAt))}</h2>
          <p className="muted">
            Durasi live {durationText(s.durationMs)}
            {s.drops > 0 && ` · putus ${s.drops}× lalu lanjut`}
          </p>
        </div>
        <span className={reason.cls}>{reason.text}</span>
      </div>

      <div className="stats">
        <Stat label="Durasi" value={durationText(s.durationMs)} />
        <Stat
          label="Puncak penonton"
          value={s.viewers.peak === null ? "–" : formatViewers(s.viewers.peak)}
          hint={s.viewers.average === null ? "Chat YouTube tidak tersambung" : `rata-rata ${formatViewers(s.viewers.average)}`}
        />
        <Stat label="Pesan chat" value={formatViewers(s.chat.messages)} hint={`${formatViewers(s.chat.chatters)} chatter unik`} />
        <Stat
          label="Donasi Trakteer"
          value={rupiah(s.trakteer.totalRp)}
          hint={`${s.trakteer.count} donasi (tanpa test)`}
        />
        <Stat
          label="Super Chat"
          value={String(s.chat.superChatCount)}
          hint={`${s.chat.memberships} member baru · ${s.chat.gifts} hadiah`}
        />
        <Stat
          label="Frame drop"
          value={s.stream.droppedPct === null ? "–" : `${s.stream.droppedPct.toFixed(2)}%`}
          hint={`${formatViewers(s.stream.droppedFrames)} dari ${formatViewers(s.stream.totalFrames)} frame`}
        />
      </div>

      <div className="charts">
        {hasViewers && <MinuteChart title="Penonton" points={viewers} format={(v) => formatViewers(v)} />}
        <MinuteChart title="Bitrate" points={bitrate} format={(v) => formatBitrate(v)} />
      </div>

      <div className="summary-cols">
        <div>
          <h3 className="subhead">Kesehatan stream</h3>
          <ul className="kv">
            <li>
              <span>Rata-rata bitrate</span>
              <strong>{formatBitrate(s.stream.avgBitrateKbps)}</strong>
            </li>
            <li>
              <span>OBS menyambung ulang</span>
              <strong>{s.reconnects}×</strong>
            </li>
            <li>
              <span>Stream putus lalu lanjut</span>
              <strong>{s.drops}×</strong>
            </li>
            <li>
              <span>Alert masalah</span>
              <strong>{s.alerts.length}</strong>
            </li>
          </ul>

          <h3 className="subhead">Top donatur Trakteer</h3>
          {s.trakteer.top.length === 0 ? (
            <p className="muted">Belum ada donasi di sesi ini.</p>
          ) : (
            <ol className="kv">
              {s.trakteer.top.map((d) => (
                <li key={d.name}>
                  <span>
                    {d.name} <small className="muted">({d.count}×)</small>
                  </span>
                  <strong>{rupiah(d.totalRp)}</strong>
                </li>
              ))}
            </ol>
          )}

          {s.chat.superChats.length > 0 && (
            <>
              <h3 className="subhead">Super Chat & Super Sticker</h3>
              <ul className="kv">
                {s.chat.superChats.map((p, i) => (
                  <li key={`${p.name}-${i}`}>
                    <span>{p.name}</span>
                    <strong>{p.amount || "–"}</strong>
                  </li>
                ))}
              </ul>
              {s.chat.superChatCount > s.chat.superChats.length && (
                <small className="muted">+{s.chat.superChatCount - s.chat.superChats.length} lainnya</small>
              )}
            </>
          )}
        </div>

        <div>
          <h3 className="subhead">Timeline alert</h3>
          {s.alerts.length === 0 ? (
            <p className="muted">Tidak ada masalah stream. Mantap!</p>
          ) : (
            <ul className="alert-list">
              {s.alerts.map((a, i) => (
                <li key={`${a.atS}-${i}`} className={`level-${a.level}`}>
                  <span className="alert-level">
                    {ALERT_ICON[a.level]} {offset(a.atS)}
                  </span>
                  <span>
                    <strong>{a.title}</strong>
                    <small>{ALERT_LEVEL_LABEL[a.level]}</small>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

function SessionListCard({
  current,
  history,
  selected,
  onSelect,
  onChange,
}: {
  current: SessionSummary | null;
  history: SessionSummary[];
  selected: string | null;
  onSelect: (id: string) => void;
  onChange: (state: SessionState) => void;
}) {
  const [confirmClear, setConfirmClear] = useState(false);
  const { feedback, busy, run } = useFeedback();

  useEffect(() => {
    if (!confirmClear) return;
    const timer = window.setTimeout(() => setConfirmClear(false), 4000);
    return () => window.clearTimeout(timer);
  }, [confirmClear]);

  return (
    <section className="card">
      <h2>Sesi sebelumnya</h2>
      {current === null && history.length === 0 ? (
        <p className="muted">Belum ada.</p>
      ) : (
        <ul className="session-list">
          {current && (
            <li className={selected === CURRENT ? "active" : ""}>
              <button className="link" onClick={() => onSelect(CURRENT)}>
                <span className="chip live">LIVE</span> {SHORT_DATE.format(new Date(current.startedAt))}
              </button>
              <span className="muted">{durationText(current.durationMs)}</span>
            </li>
          )}
          {history.map((s) => (
            <li key={s.id} className={selected === s.id ? "active" : ""}>
              <button className="link" onClick={() => onSelect(s.id)}>
                {SHORT_DATE.format(new Date(s.startedAt))}
              </button>
              <span className="muted">{durationText(s.durationMs)}</span>
              <button
                className="secondary small"
                disabled={busy}
                aria-label={`Hapus sesi ${SHORT_DATE.format(new Date(s.startedAt))}`}
                onClick={() => void run(async () => onChange(await ipc.sessionDelete(s.id)))}
              >
                Hapus
              </button>
            </li>
          ))}
        </ul>
      )}
      {history.length > 0 && (
        <div className="actions top-gap">
          <button
            className={confirmClear ? "danger" : "secondary"}
            disabled={busy}
            onClick={() => {
              if (!confirmClear) {
                setConfirmClear(true);
                return;
              }
              setConfirmClear(false);
              void run(async () => onChange(await ipc.sessionClear()), "Riwayat sesi dihapus.");
            }}
          >
            {confirmClear ? "Yakin? Klik lagi untuk hapus semua" : "Hapus semua"}
          </button>
        </div>
      )}
      <FeedbackLine feedback={feedback} />
      <small className="muted">Menyimpan 30 sesi terakhir, hanya di komputer ini.</small>
    </section>
  );
}

function SessionSettingsCard({ settings, save }: TabProps) {
  const { feedback, busy, run } = useFeedback();
  const openOnEnd = settings.session.openOnEnd;

  return (
    <section className="card">
      <h2>Setelah stream selesai</h2>
      <label className="toggle">
        <input
          type="checkbox"
          checked={openOnEnd}
          disabled={busy}
          onChange={(e) => {
            const value = e.target.checked;
            void run(
              () => save((s) => ({ ...s, session: { ...s.session, openOnEnd: value } })),
              value ? "Dashboard akan terbuka setelah stream selesai." : "Dashboard tidak dibuka otomatis.",
            );
          }}
        />
        <span>
          Buka ringkasan di dashboard
          <small>
            Saat stream dihentikan (atau putus dan tidak lanjut dalam 3 menit). Sesi di bawah 2 menit tetap disimpan,
            tapi dashboard tidak dibuka.
          </small>
        </span>
      </label>
      <FeedbackLine feedback={feedback} />
    </section>
  );
}
