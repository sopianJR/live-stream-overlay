import { useEffect, useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";
import type { AlertSettings, MicAudio } from "../../shared/types";
import { ALERT_ICON, ALERT_LEVEL_LABEL, useAlerts } from "../../shared/useAlerts";
import { useObsStatus } from "../../shared/useObsStatus";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

const DEFAULT_ALERTS: AlertSettings = {
  enabled: true,
  droppedWarnPct: 1,
  droppedCriticalPct: 5,
  lowBitratePct: 50,
  cpuPct: 90,
  micReminder: true,
  micSilenceS: 120,
  micClipping: true,
};

/** Ambang deteksi di Rust (`obs/audio.rs`). */
const SOUND_DB = -50;
const CLIP_DB = -0.5;
const METER_MIN_DB = -60;

export function AlertTab({ settings, save }: TabProps) {
  const micLevels = settings.alerts.enabled && (settings.alerts.micSilenceS > 0 || settings.alerts.micClipping);
  return (
    <div className="grid">
      <ActiveAlertsCard editHotkey={settings.hotkeys.toggleEdit} />
      <AlertSettingsCard settings={settings} save={save} />
      <MicTestCard enabled={micLevels} />
      <RulesCard />
    </div>
  );
}

function ActiveAlertsCard({ editHotkey }: { editHotkey: string }) {
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
        <kbd>{editHotkey}</kbd>). Semua alert juga tercatat di file log.
      </small>
    </section>
  );
}

type Draft = Record<keyof AlertSettings, string | boolean>;

function toDraft(a: AlertSettings): Draft {
  return {
    enabled: a.enabled,
    micReminder: a.micReminder,
    micClipping: a.micClipping,
    micSilenceS: String(a.micSilenceS),
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
      micClipping: draft.micClipping === true,
      micSilenceS: Math.round(num("micSilenceS")),
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

        <div className="field-row">
          {numberField(
            "micSilenceS",
            "Mic tidak terdengar (detik)",
            "30–900, default 120. 0 = mati. Mic tidak mute, tapi tanpa suara sama sekali.",
          )}
        </div>

        <label className="toggle">
          <input
            type="checkbox"
            checked={draft.micClipping === true}
            disabled={draft.enabled !== true}
            onChange={(e) => set("micClipping", e.target.checked)}
          />
          <span>
            Peringatkan jika suara mic pecah
            <small>Suara menyentuh batas maksimum (0 dB) berulang kali dalam 10 detik.</small>
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
                  save((s) => ({ ...s, alerts: DEFAULT_ALERTS })),
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
  ["⚠️", "Mic tidak terdengar", "Mic tidak mute tapi tanpa suara selama 2 menit (bisa diatur); diulang tiap 5 menit."],
  ["⚠️", "Suara mic pecah", "Terlalu keras ≥ 3 detik dalam 10 detik; diulang tiap 2 menit."],
  ["🔔", "Mic mute saat live", "Diulang tiap 2 menit."],
  ["🔔", "Chat YouTube bermasalah / live belum terdeteksi", "Saat OBS live; diulang tiap 5 menit."],
  ["🔔", "Koneksi Trakteer terputus", "Jika kunci terisi dan OBS live; diulang tiap 5 menit."],
];

/** Posisi (0–100%) level dB pada meter. */
function meterPct(db: number | null): number {
  if (db === null) return 0;
  return Math.min(100, Math.max(0, ((db - METER_MIN_DB) / -METER_MIN_DB) * 100));
}

function MicTestCard({ enabled }: { enabled: boolean }) {
  const status = useObsStatus();
  const [audio, setAudio] = useState<MicAudio | null>(null);
  const connected = status?.connection === "connected";

  useEffect(() => {
    if (!enabled || !connected) {
      setAudio(null);
      return;
    }
    let alive = true;
    const poll = () =>
      ipc
        .obsMicLevel()
        .then((a) => alive && setAudio(a))
        .catch(() => alive && setAudio(null));
    poll();
    const timer = window.setInterval(poll, 250);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [enabled, connected]);

  const micName = status?.mic?.name ?? null;
  const known = audio?.known === true;
  const peak = known ? audio.peakDb : null;
  const tone = !known ? "" : peak !== null && peak >= CLIP_DB ? "bad" : peak !== null && peak >= SOUND_DB ? "ok" : "warn";

  let text: string;
  if (!enabled) text = "Aktifkan \"Mic tidak terdengar\" atau \"suara mic pecah\" untuk memakai meter ini.";
  else if (!connected) text = "Hubungkan OBS dulu (tab Koneksi).";
  else if (!micName) text = status?.micNote ?? "Input mic belum tersedia.";
  else if (!known) text = "Menunggu level dari OBS…";
  else if (status?.mic?.muted) text = "Mic sedang mute di OBS.";
  else if (peak !== null && peak >= CLIP_DB) text = "Terlalu keras: suara pecah.";
  else if (peak !== null && peak >= SOUND_DB) text = "Suara terdeteksi.";
  else text = `Senyap ${Math.floor(audio.silentForMs / 1000)} detik.`;

  return (
    <section className="card">
      <h2>Tes mic</h2>
      <p className="muted">
        Bicara ke mic: bar harus melewati garis {SOUND_DB} dB. Jika tetap kosong, kemungkinan device mic di
        OBS salah. {micName && <>Input dipantau: <strong>{micName}</strong>.</>}
      </p>
      <div className={`mic-meter ${tone}`} role="meter" aria-valuemin={METER_MIN_DB} aria-valuemax={0} aria-valuenow={peak ?? METER_MIN_DB} aria-label="Level mic">
        <div className="mic-meter-fill" style={{ width: `${meterPct(peak)}%` }} />
        <span className="mic-meter-mark" style={{ left: `${meterPct(SOUND_DB)}%` }} title={`Ambang suara ${SOUND_DB} dB`} />
        <span className="mic-meter-mark clip" style={{ left: `${meterPct(CLIP_DB)}%` }} title="Batas pecah" />
      </div>
      <div className="mic-meter-scale muted">
        <span>{METER_MIN_DB} dB</span>
        <span>{peak === null ? "–∞" : `${peak.toFixed(1)}`} dB</span>
        <span>0 dB</span>
      </div>
      <p className={`status ${tone === "ok" ? "ok" : tone === "bad" ? "bad" : ""}`}>{text}</p>
      <small className="muted">Level dibaca dari OBS (setelah fader & filter), sama seperti yang didengar penonton.</small>
    </section>
  );
}

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
