import { useEffect, useRef, useState } from "react";

// DEMO Fase 0: pesan disimulasikan. Fase 3 menggantinya dengan chat YouTube asli.

type Role = "owner" | "moderator" | "member" | "viewer";

interface DemoMessage {
  id: number;
  author: string;
  role: Role;
  text: string;
  superChat?: { amount: string; tier: "blue" | "green" | "yellow" | "red" };
}

const MAX_MESSAGES = 50;

const AUTHORS: Array<[string, Role]> = [
  ["Andi", "viewer"],
  ["Budi Santoso", "member"],
  ["Citra", "viewer"],
  ["Dewi_gaming", "moderator"],
  ["Eko", "viewer"],
  ["Fajar R", "member"],
  ["Gita", "viewer"],
  ["Hendra", "viewer"],
];

const TEXTS = [
  "halo bang!",
  "gg wp",
  "mantap clutch-nya 🔥",
  "bang suaranya kecilan dikit",
  "main ranked ga nanti?",
  "first time nonton, seru juga",
  "wkwkwk",
  "itu musuhnya di belakang!!",
  "semangat streamingnya",
  "settingan grafiknya apa bang?",
];

const SUPER_CHATS: Array<NonNullable<DemoMessage["superChat"]>> = [
  { amount: "Rp10.000", tier: "blue" },
  { amount: "Rp25.000", tier: "green" },
  { amount: "Rp50.000", tier: "yellow" },
  { amount: "Rp100.000", tier: "red" },
];

function pick<T>(items: readonly T[]): T {
  return items[Math.floor(Math.random() * items.length)];
}

function makeMessage(id: number): DemoMessage {
  const [author, role] = pick(AUTHORS);
  const msg: DemoMessage = { id, author, role, text: pick(TEXTS) };
  if (id % 9 === 0) msg.superChat = pick(SUPER_CHATS);
  return msg;
}

function useDemoChat(): DemoMessage[] {
  const [messages, setMessages] = useState<DemoMessage[]>(() =>
    [1, 2, 3].map(makeMessage),
  );
  const nextId = useRef(4);

  useEffect(() => {
    let timer = 0;
    const schedule = () => {
      timer = window.setTimeout(() => {
        const msg = makeMessage(nextId.current++);
        setMessages((prev) => [...prev, msg].slice(-MAX_MESSAGES));
        schedule();
      }, 1200 + Math.random() * 2600);
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, []);

  return messages;
}

export function ChatWidget({ editMode }: { editMode: boolean }) {
  const messages = useDemoChat();
  const listRef = useRef<HTMLDivElement>(null);

  // Di mode edit chat bisa di-scroll; tetap ikuti pesan terbaru.
  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, editMode]);

  return (
    <div className="chat-wrap">
    <div className="chat" ref={listRef}>
      {messages.map((m) =>
        m.superChat ? (
          <div key={m.id} className={`chat-msg super tier-${m.superChat.tier}`}>
            <div className="super-head">
              <span className="chat-author">{m.author}</span>
              <span className="super-amount">{m.superChat.amount}</span>
            </div>
            <div className="chat-text">{m.text}</div>
          </div>
        ) : (
          <div key={m.id} className="chat-msg">
            <span className={`chat-author role-${m.role}`}>
              {m.role === "moderator" && <span className="badge">🔧</span>}
              {m.role === "member" && <span className="badge">⭐</span>}
              {m.author}
            </span>{" "}
            <span className="chat-text">{m.text}</span>
          </div>
        ),
      )}
    </div>
      <span className="demo-tag chat-demo-tag">DEMO</span>
    </div>
  );
}
