import { useEffect, useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";
import type { TrakteerKeyStatus, TrakteerStatus } from "../../shared/types";
import { useTrakteer } from "../../shared/useTrakteer";
import { FeedbackLine, useFeedback } from "../ui";

const KIND_LABEL = { live: "", test: "TEST", sample: "CONTOH" } as const;

function statusChip(s: TrakteerStatus | null) {
  if (!s) return null;
  const proto = s.protocol ? ` (${s.protocol})` : "";
  switch (s.state) {
    case "connected":
      return <span className="chip ok">Tersambung{proto}</span>;
    case "connecting":
      return <span className="chip warn">Menghubungkan…</span>;
    case "retrying":
      return <span className="chip warn">Terputus, mencoba ulang{s.attempt > 1 ? ` (ke-${s.attempt})` : ""}</span>;
    case "failed":
      return <span className="chip bad">Gagal</span>;
    default:
      return <span className="chip">Belum diatur</span>;
  }
}

function detectType(value: string): string | null {
  const v = value.trim();
  if (!v) return null;
  return v.startsWith("trv2-") ? "Stream Key (V2) terdeteksi" : "Channel ID (V1)";
}

function timeOf(ms: number): string {
  return new Date(ms).toLocaleTimeString("id-ID", { hour: "2-digit", minute: "2-digit" });
}

/** Koneksi ke Trakteer untuk alert donasi di overlay. */
export function TrakteerCard() {
  const { status, recent } = useTrakteer();
  const [keyStatus, setKeyStatus] = useState<TrakteerKeyStatus | null>(null);
  const [value, setValue] = useState("");
  const { feedback, busy, run } = useFeedback();

  useEffect(() => {
    ipc.trakteerKeyStatus().then(setKeyStatus).catch(console.error);
  }, []);

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      setKeyStatus(await ipc.trakteerSetKey(value));
      setValue("");
    }, "Kunci disimpan di Windows Credential Manager. Menghubungkan…");
  };

  const onClear = () =>
    run(async () => {
      setKeyStatus(await ipc.trakteerClearKey());
    }, "Kunci Trakteer dihapus.");

  const detected = detectType(value);

  return (
    <section className="card">
      <h2>Trakteer (alert donasi)</h2>
      <div className="conn-head">
        {statusChip(status)}
        {keyStatus?.hasKey && (
          <button
            type="button"
            className="secondary small"
            disabled={busy}
            onClick={() => void run(ipc.trakteerReconnect)}
          >
            Hubungkan ulang
          </button>
        )}
      </div>
      {status?.message && <p className="conn-msg">{status.message}</p>}
      <p className="muted top-gap">
        Donasi yang masuk tampil sebagai kartu di overlay (hanya terlihat oleh Anda, tidak ikut
        stream). Ambil kunci dari dashboard Trakteer di pengaturan <strong>widget Alert</strong>:
        Stream Key ada di URL widget (bagian <code>token=…</code>, diawali <code>trv2-</code>).
        Channel ID versi lama juga didukung.
      </p>

      <form className="form" onSubmit={onSave}>
        <label className="field">
          <span>
            Channel ID / Stream Key{" "}
            {keyStatus &&
              (keyStatus.hasKey ? (
                <span className="status ok">tersimpan{keyStatus.protocol ? ` · ${keyStatus.protocol}` : ""}</span>
              ) : (
                <span className="status">belum ada</span>
              ))}
          </span>
          <input
            type="password"
            value={value}
            autoComplete="off"
            spellCheck={false}
            placeholder={keyStatus?.hasKey ? "•••••••• (isi untuk mengganti)" : "trv2-… atau Channel ID"}
            onChange={(e) => setValue(e.target.value)}
          />
          {detected && <small className="muted">{detected}</small>}
        </label>
        <div className="actions">
          <button type="submit" disabled={!value.trim() || busy}>
            Simpan kunci
          </button>
          {keyStatus?.hasKey && (
            <button type="button" className="secondary" disabled={busy} onClick={() => void onClear()}>
              Hapus kunci
            </button>
          )}
        </div>
        <FeedbackLine feedback={feedback} />
      </form>

      <h3 className="subhead">Donasi terakhir</h3>
      {recent.length === 0 ? (
        <p className="muted">
          Belum ada donasi sejak aplikasi dibuka. Coba tombol <em>Test alert</em> di dashboard
          Trakteer.
        </p>
      ) : (
        <ul className="donation-list">
          {recent.slice(0, 10).map((ev) => (
            <li key={ev.id}>
              <span className="muted">{timeOf(ev.receivedAtMs)}</span>
              <span className="donation-list-name">
                {ev.donation.supporterName}
                {KIND_LABEL[ev.kind] && <span className="status">{KIND_LABEL[ev.kind]}</span>}
              </span>
              <strong>{ev.donation.priceText}</strong>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
