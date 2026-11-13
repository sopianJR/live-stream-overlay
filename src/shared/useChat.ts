import { useEffect, useState } from "react";
import { ipc, onChatStatus, onChatUpdate } from "./ipc";
import type { ChatMessage, ChatStatus, ChatUpdate } from "./types";

/** Sama dengan kapasitas buffer di Rust (youtube/buffer.rs). */
export const CHAT_CAPACITY = 200;

/** Menerapkan perubahan dari Rust ke daftar pesan (fungsi murni). */
export function applyChatUpdate(prev: ChatMessage[], update: ChatUpdate): ChatMessage[] {
  let next = update.reset ? [] : prev;
  if (update.removedIds.length || update.removedAuthors.length) {
    const ids = new Set(update.removedIds);
    const authors = new Set(update.removedAuthors);
    next = next.filter((m) => !ids.has(m.id) && !authors.has(m.author.channelId));
  }
  if (update.upserts.length) {
    next = [...next];
    const index = new Map(next.map((m, i) => [m.id, i]));
    for (const m of update.upserts) {
      const i = index.get(m.id);
      if (i === undefined) {
        index.set(m.id, next.length);
        next.push(m);
      } else {
        next[i] = m;
      }
    }
  }
  return next.length > CHAT_CAPACITY ? next.slice(-CHAT_CAPACITY) : next;
}

/** Pesan dari snapshot ditambah pesan dari event yang sempat datang lebih dulu (tanpa duplikat). */
function mergeSnapshot(snapshot: ChatMessage[], live: ChatMessage[]): ChatMessage[] {
  const ids = new Set(snapshot.map((m) => m.id));
  return [...snapshot, ...live.filter((m) => !ids.has(m.id))].slice(-CHAT_CAPACITY);
}

/** Status chat + pesan terbaru: snapshot di awal, lalu mengikuti event dari Rust. */
export function useChat(): { status: ChatStatus | null; messages: ChatMessage[] } {
  const [status, setStatus] = useState<ChatStatus | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);

  useEffect(() => {
    let disposed = false;
    const unStatus = onChatStatus((s) => {
      if (!disposed) setStatus(s);
    });
    const unUpdate = onChatUpdate((u) => {
      if (!disposed) setMessages((prev) => applyChatUpdate(prev, u));
    });
    ipc
      .chatGetSnapshot()
      .then((snap) => {
        if (disposed) return;
        setStatus((prev) => prev ?? snap.status);
        setMessages((prev) => mergeSnapshot(snap.messages, prev));
      })
      .catch((e) => console.error("chat_get_snapshot gagal", e));
    return () => {
      disposed = true;
      unStatus.then((fn) => fn());
      unUpdate.then((fn) => fn());
    };
  }, []);

  return { status, messages };
}

/** Hanya status chat (mis. jumlah penonton di bar OBS). */
export function useChatStatus(): ChatStatus | null {
  const [status, setStatus] = useState<ChatStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisten = onChatStatus((s) => {
      if (!disposed) setStatus(s);
    });
    ipc
      .chatGetSnapshot()
      .then((snap) => {
        if (!disposed) setStatus((prev) => prev ?? snap.status);
      })
      .catch((e) => console.error("chat_get_snapshot gagal", e));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  return status;
}
