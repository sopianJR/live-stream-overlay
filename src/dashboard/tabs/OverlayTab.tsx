import { useCallback, useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { ipc } from "../../shared/ipc";
import { LAYOUT_RESET_EVENT, resetLayout } from "../../shared/layout";
import type { MonitorInfo, OverlaySettings } from "../../shared/types";
import { useOverlayStatus } from "../../shared/useOverlayStatus";
import { Chip, FeedbackLine, useFeedback, type TabProps } from "../ui";

const FADE_OPTIONS = [
  { value: 0, label: "Tidak memudar" },
  { value: 10, label: "10 detik" },
  { value: 15, label: "15 detik" },
  { value: 30, label: "30 detik" },
  { value: 60, label: "1 menit" },
  { value: 120, label: "2 menit" },
];

export function OverlayTab({ settings, save }: TabProps) {
  return (
    <div className="grid">
      <StatusCard />
      <MonitorCard settings={settings} save={save} />
      <AppearanceCard settings={settings} save={save} />
    </div>
  );
}

function StatusCard() {
  const status = useOverlayStatus();
  const { feedback, run } = useFeedback();

  const onResetLayout = () =>
    run(async () => {
      await resetLayout();
      await emit(LAYOUT_RESET_EVENT);
    }, "Layout widget dikembalikan ke posisi awal.");

  return (
    <section className="card">
      <h2>Status overlay</h2>
      {status ? (
        <div className="chips">
          <Chip ok={status.visible} on="Tampil" off="Tersembunyi" />
          <Chip ok={!status.editMode} on="Mode pasif (tembus klik)" off="Mode edit" warnWhenOff />
          <Chip
            ok={status.contentProtected}
            on="Tersembunyi dari capture"
            off="Terlihat di capture"
            warnWhenOff
          />
        </div>
      ) : (
        <p className="muted">Memuat status…</p>
      )}

      <div className="actions">
        <button onClick={() => run(ipc.overlayToggleVisible)}>
          {status?.visible ? "Sembunyikan overlay" : "Tampilkan overlay"}
        </button>
        <button
          className="secondary"
          onClick={() => run(() => ipc.overlaySetEditMode(!status?.editMode))}
        >
          {status?.editMode ? "Selesai edit" : "Atur posisi widget"}
        </button>
        <button className="secondary" onClick={onResetLayout}>
          Reset layout
        </button>
      </div>

      <label className="toggle">
        <input
          type="checkbox"
          checked={status?.contentProtected ?? true}
          onChange={(e) => run(() => ipc.overlaySetContentProtected(e.target.checked))}
        />
        <span>
          Sembunyikan overlay dari capture (OBS Display Capture, screenshot)
          <small>Selalu aktif lagi setiap aplikasi dibuka. Matikan hanya untuk pengujian.</small>
        </span>
      </label>
      <FeedbackLine feedback={feedback} />
    </section>
  );
}

function MonitorCard({ settings, save }: TabProps) {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const { feedback, busy, run } = useFeedback();

  const refresh = useCallback(() => run(async () => setMonitors(await ipc.monitorsList())), [run]);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const selected = settings.overlay.monitor ?? "";
  const missing = selected !== "" && monitors.length > 0 && !monitors.some((m) => m.id === selected);

  const onChange = (value: string) =>
    run(
      () => save((s) => ({ ...s, overlay: { ...s.overlay, monitor: value || null } })),
      "Overlay dipindahkan.",
    );

  return (
    <section className="card">
      <h2>Monitor</h2>
      <p className="muted">Pilih monitor tempat game berjalan. Overlay menutupi seluruh monitor itu.</p>
      <div className="field-row">
        <select value={selected} disabled={busy} onChange={(e) => onChange(e.target.value)}>
          <option value="">Ikuti monitor utama</option>
          {monitors.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
          {missing && <option value={selected}>Monitor tersimpan (tidak terhubung)</option>}
        </select>
        <button className="secondary" disabled={busy} onClick={() => void refresh()}>
          Muat ulang
        </button>
      </div>
      {missing && (
        <p className="note warn">
          Monitor yang dipilih sedang tidak terhubung; overlay sementara memakai monitor utama.
        </p>
      )}
      <FeedbackLine feedback={feedback} />
    </section>
  );
}

function AppearanceCard({ settings, save }: TabProps) {
  // Draft lokal agar slider responsif; disimpan dengan jeda singkat.
  const [draft, setDraft] = useState<OverlaySettings>(settings.overlay);
  const { feedback, run } = useFeedback();
  const timer = useRef<number>(0);

  // Ikuti nilai tersimpan, kecuali sedang ada perubahan yang menunggu disimpan
  // (agar slider tidak melompat balik saat digeser).
  useEffect(() => {
    if (timer.current === 0) setDraft(settings.overlay);
  }, [settings.overlay]);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const update = (patch: Partial<OverlaySettings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = 0;
      void run(() =>
        save((s) => ({
          ...s,
          overlay: {
            ...s.overlay,
            widgetOpacity: next.widgetOpacity,
            fontScale: next.fontScale,
            chatFadeSeconds: next.chatFadeSeconds,
          },
        })),
      );
    }, 250);
  };

  const pct = (v: number) => `${Math.round(v * 100)}%`;

  return (
    <section className="card">
      <h2>Tampilan</h2>
      <p className="muted">Perubahan langsung terlihat di overlay.</p>

      <label className="range">
        <span>
          Kepekatan latar widget <strong>{pct(draft.widgetOpacity)}</strong>
        </span>
        <input
          type="range"
          min={30}
          max={100}
          step={5}
          value={Math.round(draft.widgetOpacity * 100)}
          onChange={(e) => update({ widgetOpacity: Number(e.target.value) / 100 })}
        />
      </label>

      <label className="range">
        <span>
          Ukuran huruf <strong>{pct(draft.fontScale)}</strong>
        </span>
        <input
          type="range"
          min={80}
          max={160}
          step={5}
          value={Math.round(draft.fontScale * 100)}
          onChange={(e) => update({ fontScale: Number(e.target.value) / 100 })}
        />
      </label>

      <label className="field">
        <span>Pesan chat memudar setelah</span>
        <select
          value={draft.chatFadeSeconds}
          onChange={(e) => update({ chatFadeSeconds: Number(e.target.value) })}
        >
          {FADE_OPTIONS.some((o) => o.value === draft.chatFadeSeconds) ? null : (
            <option value={draft.chatFadeSeconds}>{draft.chatFadeSeconds} detik</option>
          )}
          {FADE_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
      <FeedbackLine feedback={feedback} />
    </section>
  );
}
