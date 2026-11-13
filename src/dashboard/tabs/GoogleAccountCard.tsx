import { useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";
import type { AccountStatus } from "../../shared/types";
import { useAccount } from "../../shared/useAccount";
import { FeedbackLine, useFeedback } from "../ui";

function statusChip(s: AccountStatus) {
  switch (s.state) {
    case "signedIn":
      return <span className="chip ok">Login sebagai {s.channelTitle ?? "akun Google"}</span>;
    case "signingIn":
      return <span className="chip warn">Menunggu login di browser…</span>;
    default:
      return <span className="chip">{s.hasClient ? "Belum login" : "Belum diatur"}</span>;
  }
}

function SetupSteps() {
  return (
    <ol className="steps">
      <li>
        Buka <strong>console.cloud.google.com</strong>, buat project baru (mis. "LiveStream Overlay").
      </li>
      <li>
        <strong>APIs &amp; Services → Library</strong> → aktifkan <strong>YouTube Data API v3</strong>.
      </li>
      <li>
        <strong>OAuth consent screen</strong> (Google Auth Platform) → tipe <em>External</em> → isi nama
        aplikasi &amp; email. Di <em>Audience / Test users</em>, tambahkan akun Google pemilik
        channel Anda.
      </li>
      <li>
        <strong>Credentials / Clients → Create OAuth client ID</strong> → tipe{" "}
        <strong>Desktop app</strong>.
      </li>
      <li>
        Salin <strong>Client ID</strong> dan <strong>Client secret</strong> ke kolom di bawah.
      </li>
    </ol>
  );
}

function ClientForm({ onSaved }: { onSaved?: () => void }) {
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const { feedback, busy, run } = useFeedback();

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      await ipc.accountSetClient(clientId, secret);
      setClientId("");
      setSecret("");
      onSaved?.();
    }, "OAuth client disimpan di Windows Credential Manager.");
  };

  return (
    <form className="form" onSubmit={onSubmit}>
      <label className="field">
        <span>Client ID</span>
        <input
          value={clientId}
          spellCheck={false}
          autoComplete="off"
          placeholder="1234-abcd.apps.googleusercontent.com"
          onChange={(e) => setClientId(e.target.value)}
        />
      </label>
      <label className="field">
        <span>Client secret</span>
        <input
          type="password"
          value={secret}
          autoComplete="off"
          placeholder="GOCSPX-…"
          onChange={(e) => setSecret(e.target.value)}
        />
      </label>
      <div className="actions">
        <button type="submit" disabled={busy || !clientId.trim() || !secret.trim()}>
          Simpan OAuth client
        </button>
      </div>
      <FeedbackLine feedback={feedback} />
    </form>
  );
}

/** Login Google opsional, hanya untuk membalas chat dari overlay. */
export function GoogleAccountCard() {
  const account = useAccount();
  const [editingClient, setEditingClient] = useState(false);
  const { feedback, busy, run } = useFeedback();

  if (!account) return null;
  const signedIn = account.state === "signedIn";
  const signingIn = account.state === "signingIn";
  const showForm = !account.hasClient || editingClient;

  return (
    <section className="card">
      <h2>Balas chat (login Google, opsional)</h2>
      <div className="chips">{statusChip(account)}</div>
      <p className="muted">
        Membaca chat tidak perlu login. Login Google hanya dibutuhkan untuk membalas chat dari
        overlay (mode edit). Pesan dikirim sebagai channel yang login.
      </p>

      {showForm ? (
        <>
          <SetupSteps />
          <ClientForm onSaved={() => setEditingClient(false)} />
          {editingClient && (
            <div className="actions">
              <button className="secondary" onClick={() => setEditingClient(false)}>
                Batal
              </button>
            </div>
          )}
        </>
      ) : (
        <div className="actions">
          {signingIn ? (
            <button className="secondary" onClick={() => void ipc.accountCancelLogin()}>
              Batalkan login
            </button>
          ) : signedIn ? (
            <button className="secondary" disabled={busy} onClick={() => void run(ipc.accountLogout, "Logout berhasil.")}>
              Logout
            </button>
          ) : (
            <button disabled={busy} onClick={() => void run(ipc.accountLogin)}>
              Login dengan Google
            </button>
          )}
          {!signingIn && (
            <button
              className="secondary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await ipc.accountClearClient();
                  setEditingClient(true);
                }, "OAuth client lama dihapus.")
              }
            >
              Ganti OAuth client
            </button>
          )}
        </div>
      )}

      {account.message && <p className="note warn">{account.message}</p>}
      <FeedbackLine feedback={feedback} />
      <p className="note info">
        Kuota gratis YouTube API: ±190 pesan/hari. Selama OAuth consent screen masih mode{" "}
        <em>Testing</em>, login berlaku 7 hari lalu perlu login ulang. Token disimpan di Windows
        Credential Manager.
      </p>
    </section>
  );
}
