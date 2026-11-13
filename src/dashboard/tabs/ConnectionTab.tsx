import { useEffect, useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

export function ConnectionTab({ settings, save }: TabProps) {
  return (
    <div className="grid">
      <ObsCard settings={settings} save={save} />
      <YoutubeCard settings={settings} save={save} />
    </div>
  );
}

function ObsCard({ settings, save }: TabProps) {
  const [host, setHost] = useState(settings.obs.host);
  const [port, setPort] = useState(String(settings.obs.port));
  const server = useFeedback();

  useEffect(() => {
    setHost(settings.obs.host);
    setPort(String(settings.obs.port));
  }, [settings.obs]);

  const onSaveServer = (e: FormEvent) => {
    e.preventDefault();
    const portNum = Number(port);
    if (!Number.isInteger(portNum) || portNum < 1 || portNum > 65535) {
      server.setFeedback({ kind: "error", text: "Port OBS harus angka 1–65535." });
      return;
    }
    void server.run(
      () => save((s) => ({ ...s, obs: { host, port: portNum } })),
      "Alamat OBS disimpan.",
    );
  };

  const dirty = host !== settings.obs.host || port !== String(settings.obs.port);

  return (
    <section className="card">
      <h2>OBS WebSocket</h2>
      <p className="muted">
        Di OBS: <strong>Tools → WebSocket Server Settings</strong> → centang{" "}
        <em>Enable WebSocket server</em>, lalu klik <em>Show Connect Info</em> untuk melihat port
        dan password.
      </p>

      <form className="form" onSubmit={onSaveServer}>
        <div className="field-row">
          <label className="field grow">
            <span>Host</span>
            <input value={host} onChange={(e) => setHost(e.target.value)} spellCheck={false} />
          </label>
          <label className="field port">
            <span>Port</span>
            <input
              value={port}
              inputMode="numeric"
              onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
            />
          </label>
        </div>
        <div className="actions">
          <button type="submit" disabled={!dirty || server.busy}>
            Simpan alamat
          </button>
        </div>
        <FeedbackLine feedback={server.feedback} />
      </form>

      <PasswordField />
      <p className="note info">Koneksi ke OBS diaktifkan di Fase 2.</p>
    </section>
  );
}

function PasswordField() {
  const [hasPassword, setHasPassword] = useState<boolean | null>(null);
  const [value, setValue] = useState("");
  const { feedback, busy, run } = useFeedback();

  useEffect(() => {
    void run(async () => setHasPassword(await ipc.obsPasswordStatus()));
  }, [run]);

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      await ipc.obsPasswordSet(value);
      setValue("");
      setHasPassword(true);
    }, "Password disimpan di Windows Credential Manager.");
  };

  const onClear = () =>
    run(async () => {
      await ipc.obsPasswordClear();
      setHasPassword(false);
    }, "Password dihapus.");

  return (
    <form className="form password" onSubmit={onSave}>
      <label className="field">
        <span>
          Password{" "}
          {hasPassword === null ? null : hasPassword ? (
            <span className="status ok">tersimpan</span>
          ) : (
            <span className="status">belum ada</span>
          )}
        </span>
        <input
          type="password"
          value={value}
          autoComplete="off"
          placeholder={hasPassword ? "•••••••• (isi untuk mengganti)" : "Password dari OBS"}
          onChange={(e) => setValue(e.target.value)}
        />
      </label>
      <div className="actions">
        <button type="submit" disabled={!value || busy}>
          Simpan password
        </button>
        {hasPassword && (
          <button type="button" className="secondary" disabled={busy} onClick={() => void onClear()}>
            Hapus password
          </button>
        )}
      </div>
      <small className="muted">
        Password disimpan terenkripsi di Windows Credential Manager, bukan di file pengaturan.
      </small>
      <FeedbackLine feedback={feedback} />
    </form>
  );
}

function YoutubeCard({ settings, save }: TabProps) {
  const [source, setSource] = useState(settings.youtube.source);
  const { feedback, busy, run } = useFeedback();

  useEffect(() => setSource(settings.youtube.source), [settings.youtube]);

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    void run(
      () => save((s) => ({ ...s, youtube: { source } })),
      source.trim() ? "Sumber chat disimpan." : "Sumber chat dikosongkan.",
    );
  };

  return (
    <section className="card">
      <h2>YouTube Live Chat</h2>
      <p className="muted">
        Isi dengan <strong>URL video live</strong>, <strong>URL channel</strong>, atau{" "}
        <strong>@handle</strong> channel Anda. Tidak perlu login untuk membaca chat.
      </p>
      <form className="form" onSubmit={onSave}>
        <label className="field">
          <span>Sumber chat</span>
          <input
            value={source}
            spellCheck={false}
            placeholder="mis. @namachannel atau https://youtube.com/watch?v=…"
            onChange={(e) => setSource(e.target.value)}
          />
        </label>
        <div className="actions">
          <button type="submit" disabled={source === settings.youtube.source || busy}>
            Simpan
          </button>
        </div>
        <FeedbackLine feedback={feedback} />
      </form>
      <p className="note info">Chat asli mulai tampil di Fase 3; saat ini overlay masih DEMO.</p>
    </section>
  );
}
