import { useEffect, useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";
import type { AlertSettings } from "../../shared/types";
import { ALERT_ICON, ALERT_LEVEL_LABEL, useAlerts } from "../../shared/useAlerts";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

export function AlertTab({ settings, save }: TabProps) {
  return (
    <div className="grid">
      <ActiveAlertsCard />
      <AlertSettingsCard settings={settings} save={save} />
      <RulesCard />
    </div>
  );
}

function ActiveAlertsCard() {
  const alerts = useAlerts();
  const { feedback, busy, run } = useFeedback();

  return (
    <section className="card wide">
      <h2>Alert aktif</h2>
      {alerts.length === 0 ? (
        <p className="muted">Tidak ada alert. Semua aman.</p>
      ) : (
        <ul className="alert-list">
          {alerts.map((a) => (
            <li key={a.id} className={`level-${a.level}`}>
              <span className="alert-level">
                {ALERT_ICON[a.level]} {ALERT_LEVEL_LABEL[a.level]}
              </span>
              <span>
                <strong>{a.title}</strong>
                {a.detail && <small>{a.detail}</small>}
              </span>
              <button className="secondary small" onClick={() => void ipc.alertsDismiss(a.id)}>
                Tutup
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="actions top-gap">
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void run(ipc.alertsTest, "Contoh alert tampil di overlay selama 10 detik.")}
        >
          Tes alert
        </button>
      </div>
      <FeedbackLine feedback={feedback} />
      <small className="muted">
        Contoh alert berguna untuk mengatur posisi widget Alert di mode edit (
        <kbd>{"Ctrl+Shift+E"}</kbd>). Semua alert juga tercatat di file log.
      </small>
    </section>
  );
}

type Draft = Record<keyof AlertSettings, string | boolean>;

function toDraft(a: AlertSettings): Draft {
  return {
    enabled: a.enabled,
    micReminder: a.micReminder,
    droppedWarnPct: String(a.droppedWarnPct),
    droppedCriticalPct: String(a.droppedCriticalPct),
    lowBitratePct: String(a.lowBitratePct),
    cpuPct: String(a.cpuPct),
  };
}

function AlertSettingsCard({ settings, save }: TabProps) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(settings.alerts));
  const { feedback, setFeedback, busy, run } = useFeedback();

  useEffect(() => setDraft(toDraft(settings.alerts)), [settings.alerts]);

  const set = (key: keyof AlertSettings, value: string | boolean) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const dirty = JSON.stringify(draft) !== JSON.stringify(toDraft(settings.alerts));

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    const num = (key: keyof AlertSettings) => Number(String(draft[key]).replace(",", "."));
    const next: AlertSettings = {
      enabled: draft.enabled === true,
      micReminder: draft.micReminder === true,
      droppedWarnPct: num("droppedWarnPct"),
      droppedCriticalPct: num("droppedCriticalPct"),
      lowBitratePct: Math.round(num("lowBitratePct")),
      cpuPct: Math.round(num("cpuPct")),
    };
    if (Object.values(next).some((v) => typeof v === "number" && !Number.isFinite(v))) {
      setFeedback({ kind: "error", text: "Isi semua ambang dengan angka." });
      return;
    }
    void run(() => save((s) => ({ ...s, alerts: next })), "Pengaturan alert disimpan.");
  };

  const numberField = (key: keyof AlertSettings, label: string, hint: string, step = "1") => (
    <label className="field">
      <span>{label}</span>
      <input
        type="number"
        inputMode="decimal"
        step={step}
        min="0"
        value={String(draft[key])}
        disabled={draft.enabled !== true}
        onChange={(e) => set(key, e.target.value)}
      />
      <small className="muted">{hint}</small>
    </label>
  );

  return (
    <section className="card">
      <h2>Ambang alert</h2>
      <form className="form" onSubmit={onSave}>
        <label className="toggle">
          <input
            type="checkbox"
            checked={draft.enabled === true}
            onChange={(e) => set("enabled", e.target.checked)}
          />
          <span>
            Aktifkan alert masalah stream
            <small>Jika dimatikan, overlay tidak menampilkan alert apa pun (kecuali contoh).</small>
          </span>
        </label>

        <div className="field-row">
          {numberField("droppedWarnPct", "Frame drop: peringatan (%)", "Default 1", "0.1")}
          {numberField("droppedCriticalPct", "Frame drop: kritis (%)", "Default 5", "0.1")}
        </div>
        <div className="field-row">
          {numberField(
            "lowBitratePct",
            "Bitrate rendah (% dari normal)",
            "10–90, default 50. 0 = mati.",
          )}
          {numberField("cpuPct", "CPU OBS tinggi (%)", "50–100, default 90. 0 = mati.")}
        </div>

        <label className="toggle">
          <input
            type="checkbox"
            checked={draft.micReminder === true}
            disabled={draft.enabled !== true}
            onChange={(e) => set("micReminder", e.target.checked)}
          />
          <span>
            Ingatkan jika mic mute saat live
            <small>Muncul setelah mic mute 10 detik, lalu diulang tiap 2 menit.</small>
          </span>
        </label>

        <div className="actions">
          <button type="submit" disabled={!dirty || busy}>
            Simpan
          </button>
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={() =>
              void run(
                () =>
                  save((s) => ({
                    ...s,
                    alerts: {
                      enabled: true,
                      droppedWarnPct: 1,
                      droppedCriticalPct: 5,
                      lowBitratePct: 50,
                      cpuPct: 90,
                      micReminder: true,
                    },
                  })),
                "Ambang alert dikembalikan ke default.",
              )
            }
          >
            Kembalikan default
          </button>
        </div>
        <FeedbackLine feedback={feedback} />
      </form>
    </section>
  );
}

const RULES: Array<[string, string, string]> = [
  ["⛔", "Stream terputus / menyambung ulang", "Menetap sampai stream pulih."],
  ["⛔", "Stream berhenti mendadak", "Stop karena error. Stop yang disengaja tidak dianggap masalah."],
  ["⛔", "Koneksi ke OBS putus saat live", "Menetap sampai OBS tersambung lagi."],
  ["⚠️", "Frame drop, bitrate turun, jaringan padat", "Tampil 10 detik, diulang tiap 1–2 menit jika berlanjut."],
  ["⚠️", "CPU OBS tinggi, frame terlewat saat render/encoding", "Tampil 10 detik, diulang tiap 2 menit."],
  ["🔔", "Mic mute saat live", "Diulang tiap 2 menit."],
  ["🔔", "Chat YouTube bermasalah / live belum terdeteksi", "Saat OBS live; diulang tiap 5 menit."],
];

function RulesCard() {
  return (
    <section className="card">
      <h2>Yang dipantau</h2>
      <ul className="rules">
        {RULES.map(([icon, title, how]) => (
          <li key={title}>
            <span aria-hidden>{icon}</span>
            <span>
              <strong>{title}</strong>
              <small>{how}</small>
            </span>
          </li>
        ))}
      </ul>
      <p className="note info">
        Bitrate "normal" diukur otomatis dari menit pertama setiap live. Alert kecil diberi jeda
        beberapa detik agar lonjakan sesaat tidak memicu alert.
      </p>
    </section>
  );
}
