import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "../../shared/ipc";
import type { HotkeyId, HotkeyInfo, HotkeySettings } from "../../shared/types";
import { FeedbackLine, useFeedback, type TabProps } from "../ui";

const DEFAULT_HOTKEYS: HotkeySettings = {
  toggleOverlay: "Ctrl+Shift+O",
  toggleEdit: "Ctrl+Shift+E",
};

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

function modifiersOf(e: KeyboardEvent): string[] {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Win");
  return mods;
}

/** "KeyO" → "O", "Digit1" → "1"; tombol lain memakai nama `KeyboardEvent.code`. */
function keyName(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return code;
}

export function HotkeyTab({ save }: TabProps) {
  const [infos, setInfos] = useState<HotkeyInfo[]>([]);
  const [recording, setRecording] = useState<HotkeyId | null>(null);
  const recordingRef = useRef<HotkeyId | null>(null);
  recordingRef.current = recording;
  const [preview, setPreview] = useState("");
  const { feedback, setFeedback, busy, run } = useFeedback();

  const refresh = useCallback(async () => setInfos(await ipc.hotkeysGet()), []);
  useEffect(() => {
    refresh().catch(console.error);
  }, [refresh]);

  const stopRecording = useCallback(async () => {
    setRecording(null);
    setPreview("");
    setInfos(await ipc.hotkeysSetSuspended(false));
  }, []);

  const startRecording = async (id: HotkeyId) => {
    setFeedback(null);
    // Hotkey dijeda agar kombinasi yang sedang terdaftar pun bisa direkam ulang.
    setInfos(await ipc.hotkeysSetSuspended(true));
    setRecording(id);
  };

  // Rekam kombinasi tombol berikutnya.
  useEffect(() => {
    if (!recording) return;

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const mods = modifiersOf(e);
      if (MODIFIER_CODES.has(e.code)) {
        setPreview(mods.join("+") + "+…");
        return;
      }
      if (e.code === "Escape" && mods.length === 0) {
        void stopRecording();
        return;
      }
      const combo = [...mods, keyName(e.code)].join("+");
      const id = recording;
      void (async () => {
        await stopRecording();
        await run(
          () => save((s) => ({ ...s, hotkeys: { ...s.hotkeys, [id]: combo } })),
          `Hotkey diubah menjadi ${combo}.`,
        );
        await refresh();
      })();
    };
    const onKeyUp = (e: KeyboardEvent) => {
      const mods = modifiersOf(e);
      setPreview(mods.length ? mods.join("+") + "+…" : "");
    };
    // Jika jendela kehilangan fokus, batalkan agar hotkey tidak tertinggal dalam keadaan dijeda.
    const onBlur = () => void stopRecording();

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [recording, run, save, stopRecording, refresh]);

  // Jangan tinggalkan hotkey dalam keadaan dijeda saat tab ditutup di tengah perekaman.
  useEffect(
    () => () => {
      if (recordingRef.current) ipc.hotkeysSetSuspended(false).catch(console.error);
    },
    [],
  );

  const onReset = () =>
    run(async () => {
      await save((s) => ({ ...s, hotkeys: DEFAULT_HOTKEYS }));
      await refresh();
    }, "Hotkey dikembalikan ke default.");

  return (
    <div className="grid">
      <section className="card wide">
        <h2>Hotkey global</h2>
        <p className="muted">
          Berfungsi walau game sedang fokus. Wajib memakai minimal satu modifier (Ctrl, Alt,
          Shift, atau Win) agar tidak bentrok dengan tombol game.
        </p>
        <ul className="hotkeys">
          {infos.map((h) => {
            const isRecording = recording === h.id;
            return (
              <li key={h.id} className={isRecording ? "recording" : ""}>
                <span>{h.label}</span>
                {isRecording ? (
                  <kbd className="rec">{preview || "Tekan kombinasi tombol… (Esc = batal)"}</kbd>
                ) : (
                  <kbd>{h.accelerator}</kbd>
                )}
                {recording ? (
                  <span className="status">dijeda</span>
                ) : (
                  <span
                    className={h.registered ? "status ok" : "status bad"}
                    title={h.error ?? ""}
                  >
                    {h.registered ? "aktif" : "gagal"}
                  </span>
                )}
                {isRecording ? (
                  <button className="secondary" onClick={() => void stopRecording()}>
                    Batal
                  </button>
                ) : (
                  <button
                    className="secondary"
                    disabled={busy || recording !== null}
                    onClick={() => void startRecording(h.id)}
                  >
                    Ubah
                  </button>
                )}
              </li>
            );
          })}
          <li>
            <span>Keluar dari mode edit (saat overlay fokus)</span>
            <kbd>Esc</kbd>
            <span className="status ok">aktif</span>
            <span />
          </li>
        </ul>
        {infos.some((h) => !h.registered && h.error) && !recording && (
          <p className="note warn">
            {infos
              .filter((h) => h.error)
              .map((h) => `${h.accelerator}: ${h.error}`)
              .join(" ")}
          </p>
        )}
        <div className="actions top-gap">
          <button className="secondary" disabled={busy || recording !== null} onClick={() => void onReset()}>
            Kembalikan default
          </button>
        </div>
        <FeedbackLine feedback={feedback} />
      </section>
    </div>
  );
}
