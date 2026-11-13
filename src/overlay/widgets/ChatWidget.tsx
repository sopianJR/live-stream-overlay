import { useEffect, useRef, type CSSProperties } from "react";
import { CHAT_STATE_LABEL, formatViewers } from "../../shared/format";
import { ipc } from "../../shared/ipc";
import type { ChatAuthor, ChatMessage, ChatRun, ChatStatus } from "../../shared/types";
import { useChat } from "../../shared/useChat";

/** Pesan yang dirender; buffer menyimpan lebih banyak untuk dibuka ulang. */
const RENDER_LIMIT = 50;

function Runs({ runs }: { runs: ChatRun[] }) {
  return (
    <>
      {runs.map((r, i) =>
        r.type === "text" ? (
          <span key={i}>{r.text}</span>
        ) : (
          <img key={i} className="emoji" src={r.url} alt={r.alt} title={r.alt} draggable={false} />
        ),
      )}
    </>
  );
}

function roleClass(a: ChatAuthor): string {
  if (a.owner) return "role-owner";
  if (a.moderator) return "role-moderator";
  if (a.member) return "role-member";
  return "";
}

function AuthorName({ author }: { author: ChatAuthor }) {
  return (
    <span className={`chat-author ${roleClass(author)}`}>
      {author.moderator && (
        <span className="badge" title="Moderator">
          🔧
        </span>
      )}
      {author.member &&
        (author.memberBadge ? (
          <img
            className="badge-img"
            src={author.memberBadge}
            alt=""
            title={author.memberLabel ?? "Member"}
            draggable={false}
          />
        ) : (
          <span className="badge" title={author.memberLabel ?? "Member"}>
            ⭐
          </span>
        ))}
      {author.name}
      {author.verified && (
        <span className="verified" title="Terverifikasi">
          ✓
        </span>
      )}
    </span>
  );
}

function ChatItem({ m }: { m: ChatMessage }) {
  switch (m.kind) {
    case "superChat":
    case "superSticker": {
      const head: CSSProperties = m.colors
        ? { background: m.colors.headerBg, color: m.colors.headerText }
        : {};
      const body: CSSProperties = m.colors
        ? { background: m.colors.bodyBg, color: m.colors.bodyText }
        : {};
      return (
        <div className="chat-msg super" style={body}>
          <div className="super-head" style={head}>
            <AuthorName author={m.author} />
            <span className="super-amount">{m.amount}</span>
          </div>
          {m.kind === "superSticker" && m.sticker ? (
            <div className="super-body">
              <img className="sticker" src={m.sticker} alt={m.header ?? "Super Sticker"} draggable={false} />
            </div>
          ) : (
            m.runs.length > 0 && (
              <div className="super-body chat-text">
                <Runs runs={m.runs} />
              </div>
            )
          )}
        </div>
      );
    }
    case "membership":
      return (
        <div className="chat-msg member-event">
          <div className="member-head">
            <AuthorName author={m.author} />
            {m.header && <span className="member-title">{m.header}</span>}
          </div>
          {m.runs.length > 0 && (
            <div className="chat-text">
              <Runs runs={m.runs} />
            </div>
          )}
        </div>
      );
    case "giftPurchase":
      return (
        <div className="chat-msg gift">
          🎁 <AuthorName author={m.author} /> <span className="chat-text">{m.header}</span>
        </div>
      );
    case "giftRedemption":
      return (
        <div className="chat-msg gift small">
          🎁 <AuthorName author={m.author} />{" "}
          <span className="chat-text">
            <Runs runs={m.runs} />
          </span>
        </div>
      );
    default:
      return (
        <div className={m.author.owner ? "chat-msg owner" : "chat-msg"}>
          <AuthorName author={m.author} />{" "}
          <span className="chat-text">
            <Runs runs={m.runs} />
          </span>
        </div>
      );
  }
}

/** Baris status di atas chat: selalu di mode edit, di mode pasif hanya saat belum tersambung. */
function StatusLine({ status, editMode }: { status: ChatStatus; editMode: boolean }) {
  const connected = status.state === "connected";
  if (!editMode && (connected || status.state === "idle")) return null;

  return (
    <div className={`chat-status ${status.state}`}>
      <span className="chat-status-text">
        {connected ? (
          <>
            <strong>{status.channel ?? "Live chat"}</strong>
            {status.viewers !== null && <> · 👁 {formatViewers(status.viewers)}</>}
            {status.message && <> · {status.message}</>}
          </>
        ) : (
          <>
            <strong>Chat: {CHAT_STATE_LABEL[status.state]}</strong>
            {status.message && <> · {status.message}</>}
            {status.state === "idle" && " · Isi sumber chat di dashboard → Koneksi."}
          </>
        )}
      </span>
      {editMode && status.state !== "idle" && (
        <button type="button" className="no-drag" onClick={() => void ipc.chatReconnect()}>
          Hubungkan ulang
        </button>
      )}
    </div>
  );
}

export function ChatWidget({ editMode }: { editMode: boolean }) {
  const { status, messages } = useChat();
  const listRef = useRef<HTMLDivElement>(null);
  const shown = messages.length > RENDER_LIMIT ? messages.slice(-RENDER_LIMIT) : messages;

  // Selalu ikuti pesan terbaru (di mode edit chat bisa di-scroll).
  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [shown, editMode]);

  return (
    <div className="chat-wrap">
      {status && <StatusLine status={status} editMode={editMode} />}
      <div className="chat" ref={listRef}>
        {shown.map((m) => (
          <ChatItem key={m.id} m={m} />
        ))}
      </div>
    </div>
  );
}
