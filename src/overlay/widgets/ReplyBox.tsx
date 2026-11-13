import { useEffect, useRef, useState, type FormEvent } from "react";
import { errorText, ipc } from "../../shared/ipc";
import type { AccountStatus } from "../../shared/types";

/** Sama dengan batas YouTube (youtube/api.rs). */
const MAX_CHARS = 200;

interface Props {
  account: AccountStatus | null;
  /** Teks yang disisipkan dari luar (mis. "@nama " saat nama penonton diklik). */
  mention: { text: string; seq: number } | null;
}

/** Kotak balas chat; hanya tampil di mode edit (overlay sedang bisa diklik). */
export function ReplyBox({ account, mention }: Props) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{ ok: boolean; text: string } | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!mention) return;
    setText((prev) => (prev.startsWith(mention.text) ? prev : mention.text + prev).slice(0, MAX_CHARS));
    input.current?.focus();
  }, [mention]);

  useEffect(() => {
    if (!feedback?.ok) return;
    const t = window.setTimeout(() => setFeedback(null), 2500);
    return () => window.clearTimeout(t);
  }, [feedback]);

  if (account?.state !== "signedIn") {
    return (
      <div className="reply-hint no-drag">
        {account?.state === "signingIn"
          ? "Menunggu login Google di browser…"
          : "Login Google di dashboard → Koneksi untuk membalas chat dari sini."}
      </div>
    );
  }

  const length = [...text].length;

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!text.trim() || busy) return;
    setBusy(true);
    setFeedback(null);
    try {
      await ipc.chatSend(text);
      setText("");
      setFeedback({ ok: true, text: "Terkirim ✓" });
    } catch (err) {
      setFeedback({ ok: false, text: errorText(err) });
    } finally {
      setBusy(false);
      input.current?.focus();
    }
  };

  return (
    <div className="reply-wrap no-drag">
      <form className="reply" onSubmit={(e) => void onSubmit(e)}>
        <input
          ref={input}
          value={text}
          maxLength={MAX_CHARS}
          placeholder={`Balas sebagai ${account.channelTitle ?? "akun Anda"}…`}
          aria-label="Pesan balasan"
          spellCheck={false}
          onChange={(e) => setText(e.target.value)}
        />
        <span className={length > MAX_CHARS - 20 ? "reply-count near" : "reply-count"}>
          {length}/{MAX_CHARS}
        </span>
        <button type="submit" disabled={busy || !text.trim()}>
          {busy ? "…" : "Kirim"}
        </button>
      </form>
      {feedback && <p className={feedback.ok ? "reply-msg ok" : "reply-msg bad"}>{feedback.text}</p>}
    </div>
  );
}
