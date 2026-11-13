import { useState, type ReactNode } from "react";
import { CHAT_STATE_LABEL } from "../../shared/format";
import { ipc } from "../../shared/ipc";
import { useAccount } from "../../shared/useAccount";
import { useChatStatus } from "../../shared/useChat";
import { useObsStatus } from "../../shared/useObsStatus";
import type { TabProps } from "../ui";

export const ONBOARDING_DONE_KEY = "onboarding-done";
const MANUAL_KEY = "onboarding-manual";

type ManualStep = "borderless" | "hotkeys" | "layout";

function readManual(): Record<ManualStep, boolean> {
  try {
    return { borderless: false, hotkeys: false, layout: false, ...JSON.parse(localStorage.getItem(MANUAL_KEY) ?? "{}") };
  } catch {
    return { borderless: false, hotkeys: false, layout: false };
  }
}

function Step({
  n,
  done,
  optional,
  title,
  status,
  children,
}: {
  n: number;
  done: boolean;
  optional?: boolean;
  title: string;
  status?: ReactNode;
  children: ReactNode;
}) {
  return (
    <li className={done ? "step done" : "step"}>
      <span className="step-mark" aria-hidden>
        {done ? "✓" : n}
      </span>
      <div className="step-body">
        <h3>
          {title}
          {optional && <span className="status">opsional</span>}
        </h3>
        {status && <p className="step-status">{status}</p>}
        {!done && <div className="step-how">{children}</div>}
      </div>
    </li>
  );
}

interface Props extends TabProps {
  goTo: (tab: "connection" | "overlay" | "alerts" | "hotkeys") => void;
  onFinish: () => void;
}

/** Panduan awal: langkah otomatis tercentang saat kondisinya terpenuhi. */
export function StartTab({ settings, goTo, onFinish }: Props) {
  const obs = useObsStatus();
  const chat = useChatStatus();
  const account = useAccount();
  const [manual, setManual] = useState(readManual);

  const tick = (key: ManualStep, value: boolean) =>
    setManual((prev) => {
      const next = { ...prev, [key]: value };
      try {
        localStorage.setItem(MANUAL_KEY, JSON.stringify(next));
      } catch {
        // abaikan; centang hanya berlaku selama sesi ini
      }
      return next;
    });

  const obsDone = obs?.connection === "connected";
  const chatDone = !!chat && chat.state !== "idle" && chat.state !== "failed";
  const required = [obsDone, chatDone, manual.borderless, manual.hotkeys, manual.layout];
  const doneCount = required.filter(Boolean).length;
  const hk = settings.hotkeys;

  const manualCheck = (key: ManualStep, label: string) => (
    <label className="toggle">
      <input type="checkbox" checked={manual[key]} onChange={(e) => tick(key, e.target.checked)} />
      <span>{label}</span>
    </label>
  );

  return (
    <div className="grid">
      <section className="card wide">
        <h2>Mulai di sini</h2>
        <p className="muted">
          {doneCount} dari {required.length} langkah selesai. Langkah 1 dan 2 tercentang sendiri saat
          koneksinya berhasil.
        </p>

        <ol className="onboarding">
          <Step
            n={1}
            done={obsDone}
            title="Hubungkan OBS"
            status={obsDone ? `Terhubung ke OBS ${obs?.obsVersion ?? ""} di ${obs?.endpoint}.` : obs?.message}
          >
            <p>
              Di OBS: <strong>Tools → WebSocket Server Settings</strong> → centang{" "}
              <em>Enable WebSocket server</em> → <em>Show Connect Info</em>. Salin port dan password ke
              tab Koneksi. Jika OBS ada di komputer lain, isi IP komputer itu sebagai host.
            </p>
            <button className="secondary small" onClick={() => goTo("connection")}>
              Buka tab Koneksi
            </button>
          </Step>

          <Step
            n={2}
            done={chatDone}
            title="Sambungkan live chat YouTube"
            status={
              chat && chat.state !== "idle"
                ? `Chat: ${CHAT_STATE_LABEL[chat.state]}${chat.message ? ` (${chat.message})` : ""}`
                : undefined
            }
          >
            <p>
              Isi <strong>@handle</strong> channel Anda di tab Koneksi. Live baru akan terdeteksi
              otomatis. Untuk live <em>unlisted</em>, pakai URL videonya.
            </p>
            <button className="secondary small" onClick={() => goTo("connection")}>
              Buka tab Koneksi
            </button>
          </Step>

          <Step n={3} done={manual.borderless} title="Jalankan game dalam mode Borderless">
            <p>
              Overlay hanya bisa tampil di atas game <strong>Borderless / Windowed Fullscreen</strong>,
              bukan <em>exclusive fullscreen</em>. Ubah di pengaturan grafis game.
            </p>
            {manualCheck("borderless", "Game saya sudah memakai mode borderless")}
          </Step>

          <Step n={4} done={manual.hotkeys} title="Coba hotkey di dalam game">
            <ul className="hotkey-summary">
              <li>
                <kbd>{hk.toggleOverlay}</kbd> tampilkan / sembunyikan overlay
              </li>
              <li>
                <kbd>{hk.toggleEdit}</kbd> mode edit: geser widget, kontrol OBS, balas chat
              </li>
              <li>
                <kbd>{hk.toggleMic}</kbd> mute mic · <kbd>{hk.toggleRecord}</kbd> rekam
              </li>
            </ul>
            <p className="muted">
              Overlay tidak ikut tertangkap di stream. Hotkey bentrok dengan game? Ubah di tab{" "}
              <button className="link" onClick={() => goTo("hotkeys")}>
                Hotkey
              </button>
              .
            </p>
            {manualCheck("hotkeys", "Hotkey berfungsi saat game fokus")}
          </Step>

          <Step n={5} done={manual.layout} title="Atur posisi widget & coba alert">
            <p>
              Tekan <kbd>{hk.toggleEdit}</kbd>, seret widget status OBS, chat, dan alert ke tempat
              yang tidak menutupi HUD game. Tekan <kbd>Esc</kbd> untuk kembali ke game.
            </p>
            <div className="actions">
              <button className="secondary small" onClick={() => void ipc.alertsTest()}>
                Tes alert
              </button>
              <button className="secondary small" onClick={() => goTo("overlay")}>
                Atur tampilan overlay
              </button>
            </div>
            {manualCheck("layout", "Posisi widget sudah pas")}
          </Step>

          <Step
            n={6}
            optional
            done={account?.state === "signedIn"}
            title="Balas chat dari overlay"
            status={account?.state === "signedIn" ? `Login sebagai ${account.channelTitle}.` : undefined}
          >
            <p>Butuh OAuth client Google milik Anda sendiri. Panduannya ada di tab Koneksi.</p>
            <button className="secondary small" onClick={() => goTo("connection")}>
              Buka tab Koneksi
            </button>
          </Step>
        </ol>

        <div className="actions top-gap">
          <button onClick={onFinish}>
            {doneCount === required.length ? "Selesai" : "Lewati untuk sekarang"}
          </button>
          <span className="muted">Panduan ini selalu bisa dibuka lagi dari tab Mulai.</span>
        </div>
      </section>
    </div>
  );
}
