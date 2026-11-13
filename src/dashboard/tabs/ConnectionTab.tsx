import { useCallback, useEffect, useState, type FormEvent } from "react";
import {
  CHAT_STATE_LABEL,
  CONNECTION_LABEL,
  formatDuration,
  formatViewers,
  streamLabel,
} from "../../shared/format";
import { errorText, ipc } from "../../shared/ipc";
import type { ChatStatus, ObsStatus } from "../../shared/types";
import { useChatStatus } from "../../shared/useChat";
import { useObsStatus } from "../../shared/useObsStatus";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

export function ConnectionTab({ settings, save }: TabProps) {
  return (
    <div className="grid">
      <ObsCard settings={settings} save={save} />
      <YoutubeCard settings={settings} save={save} />
    </div>
  );
}

function connectionTone(status: ObsStatus): "ok" | "warn" | "bad" {
  if (status.connection === "connected") return "ok";
  if (status.connection === "connecting" || status.connection === "retrying") return "warn";
  return "bad";
}

function ObsConnectionStatus({ status }: { status: ObsStatus | null }) {
  const { busy, run } = useFeedback();
  if (!status) return null;
  const tone = connectionTone(status);
  const { stream, record } = status;

  return (
    <div className={`conn-box ${tone}`} role="status">
      <div className="conn-head">
        <span className={`chip ${tone}`}>{CONNECTION_LABEL[status.connection]}</span>
        <span className="muted">
          {status.endpoint}
          {status.obsVersion && ` · OBS ${status.obsVersion}`}
        </span>
        <button
          type="button"
          className="secondary small"
          disabled={busy}
          onClick={() => void run(ipc.obsReconnect)}
        >
          Hubungkan ulang
        </button>
      </div>
      {status.message && <p className="conn-msg">{status.message}</p>}
      {status.connection === "retrying" && status.attempt > 1 && (
        <small className="muted">Dicoba ulang otomatis (percobaan ke-{status.attempt}).</small>
      )}
      {status.connection === "connected" && (
        <p className="conn-msg">
          Stream: <strong>{streamLabel(status).text}</strong>
          {stream.active && ` ${formatDuration(stream.durationMs)}`} · Rekaman:{" "}
          <strong>{record.active ? (record.paused ? "dijeda" : "berjalan") : "tidak"}</strong> ·
          Scene: <strong>{status.scene ?? "–"}</strong>
        </p>
      )}
    </div>
  );
}

function ObsCard({ settings, save }: TabProps) {
  const status = useObsStatus();
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
      () => save((s) => ({ ...s, obs: { ...s.obs, host, port: portNum } })),
      "Alamat OBS disimpan. Menghubungkan ulang…",
    );
  };

  const dirty = host.trim() !== settings.obs.host || port !== String(settings.obs.port);

  return (
    <section className="card">
      <h2>OBS WebSocket</h2>
      <ObsConnectionStatus status={status} />
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
      <MicField settings={settings} save={save} status={status} />
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
    }, "Password disimpan di Windows Credential Manager. Menghubungkan ulang…");
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
        Kosongkan jika autentikasi di OBS dimatikan.
      </small>
      <FeedbackLine feedback={feedback} />
    </form>
  );
}

function MicField({ settings, save, status }: TabProps & { status: ObsStatus | null }) {
  const [inputs, setInputs] = useState<string[] | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const { feedback, busy, run } = useFeedback();
  const connected = status?.connection === "connected";

  const loadInputs = useCallback(async () => {
    setListError(null);
    try {
      setInputs(await ipc.obsListAudioInputs());
    } catch (e) {
      setListError(errorText(e));
    }
  }, []);

  useEffect(() => {
    if (connected) void loadInputs();
    else setInputs(null);
  }, [connected, loadInputs]);

  const current = settings.obs.micInput ?? "";
  const options = inputs ?? [];
  // Pilihan tersimpan tetap ditampilkan walau daftar belum dimuat / input sudah tidak ada.
  const showCurrentSeparately = current !== "" && !options.includes(current);
  const missing = showCurrentSeparately && inputs !== null;

  const onChange = (value: string) =>
    run(
      () => save((s) => ({ ...s, obs: { ...s.obs, micInput: value || null } })),
      value ? `Mic dipantau dari input "${value}".` : "Mic dipantau dari Mic/Aux global OBS.",
    );

  return (
    <div className="form mic">
      <div className="field-row">
        <label className="field grow">
          <span>
            Input mic yang dipantau{" "}
            {status?.mic && (
              <span className={status.mic.muted ? "status bad" : "status ok"}>
                {status.mic.muted ? "mute" : "aktif"}
              </span>
            )}
          </span>
          <select value={current} disabled={busy} onChange={(e) => void onChange(e.target.value)}>
            <option value="">Mic/Aux global OBS (default)</option>
            {showCurrentSeparately && (
              <option value={current}>
                {current}
                {missing ? " (tidak ada di OBS)" : ""}
              </option>
            )}
            {options.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="secondary"
          disabled={!connected || busy}
          onClick={() => void loadInputs()}
        >
          Muat ulang
        </button>
      </div>
      <small className="muted">
        {connected
          ? "Daftar berisi input audio di OBS."
          : "Hubungkan ke OBS untuk melihat daftar input audio."}{" "}
        Dipakai untuk ikon mic di overlay, tombol mute, dan hotkey {settings.hotkeys.toggleMic}.
      </small>
      {status?.micNote && <p className="note warn">{status.micNote}</p>}
      {listError && <p className="msg error">{listError}</p>}
      <FeedbackLine feedback={feedback} />
    </div>
  );
}

function chatTone(status: ChatStatus): "ok" | "warn" | "bad" | "" {
  switch (status.state) {
    case "connected":
      return "ok";
    case "failed":
      return "bad";
    case "idle":
      return "";
    default:
      return "warn";
  }
}

function ChatConnectionStatus({ status }: { status: ChatStatus | null }) {
  const { busy, run } = useFeedback();
  if (!status) return null;
  const tone = chatTone(status);
  const connected = status.state === "connected";

  return (
    <div className={`conn-box ${tone}`} role="status">
      <div className="conn-head">
        <span className={`chip ${tone}`}>Chat {CHAT_STATE_LABEL[status.state]}</span>
        <span className="muted">
          {connected && status.viewers !== null
            ? `👁 ${formatViewers(status.viewers)} penonton`
            : status.source || "Sumber belum diisi"}
        </span>
        {status.state !== "idle" && (
          <button
            type="button"
            className="secondary small"
            disabled={busy}
            onClick={() => void run(ipc.chatReconnect)}
          >
            Hubungkan ulang
          </button>
        )}
      </div>
      {connected && (
        <p className="conn-msg">
          <strong>{status.title ?? "Live"}</strong>
          {status.channel && ` · ${status.channel}`}
        </p>
      )}
      {status.message && <p className="conn-msg">{status.message}</p>}
    </div>
  );
}

function YoutubeCard({ settings, save }: TabProps) {
  const status = useChatStatus();
  const [source, setSource] = useState(settings.youtube.source);
  const { feedback, busy, run } = useFeedback();

  useEffect(() => setSource(settings.youtube.source), [settings.youtube]);

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    void run(
      () => save((s) => ({ ...s, youtube: { source } })),
      source.trim() ? "Sumber chat disimpan. Menghubungkan…" : "Sumber chat dikosongkan.",
    );
  };

  return (
    <section className="card">
      <h2>YouTube Live Chat</h2>
      <ChatConnectionStatus status={status} />
      <p className="muted">
        Isi dengan <strong>@handle</strong> channel Anda (live baru terdeteksi otomatis), atau{" "}
        <strong>URL video live</strong>. Tidak perlu login untuk membaca chat.
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
      <p className="note info">
        Live <strong>unlisted</strong> tidak muncul di halaman channel, jadi pakai URL videonya.
        Chat dibaca dalam mode <em>Live chat</em> (semua pesan, bukan hanya Top chat).
      </p>
    </section>
  );
}
