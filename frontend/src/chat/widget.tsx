"use client";

// Widget web chat situs toko (P6-2, ADR-0006). Same-origin ke /_api/public/chat/* (tenant dari Host).
// Sesi chat = cookie HttpOnly host-only (tidak terbaca JS); pemberitahuan AI (D-12) wajib disetujui
// sebelum pesan pertama. Isi pesan dirender sebagai teks biasa (React meng-escape), bukan HTML.

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";

import { ApiError } from "@/lib/api";
import { shopApi } from "@/shop/api";

import "./chat.css";

export interface ChatMessage { id: string; seq: number; sender: "CUSTOMER" | "AI" | "ADMIN" | "SYSTEM"; body: string; created_at: string }
interface Conversation { conversation_id: string; status: string; messages: ChatMessage[] }
interface Config { enabled: boolean; consent_version: string; consent_text: string }

export const SENDER_LABEL: Record<ChatMessage["sender"], string> = {
  CUSTOMER: "Anda",
  AI: "Asisten AI",
  ADMIN: "Staf toko",
  SYSTEM: "Info",
};

/** Gabungkan pesan baru tanpa duplikat, urut seq. */
export function merge(list: ChatMessage[], add: ChatMessage[]): ChatMessage[] {
  const seen = new Set(list.map((m) => m.id));
  const out = [...list, ...add.filter((m) => !seen.has(m.id))];
  return out.sort((a, b) => a.seq - b.seq);
}

function errorText(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 429) return "Terlalu banyak pesan. Coba lagi sebentar lagi.";
    if (e.status === 422) return "Pesan tidak valid (1–4000 karakter).";
    return e.message;
  }
  return "Tidak dapat menghubungi toko.";
}

export function ChatWidget() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [open, setOpen] = useState(false);
  const [conv, setConv] = useState<{ id: string; status: string } | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const logRef = useRef<HTMLOListElement>(null);

  useEffect(() => {
    shopApi<Config>("GET", "/chat/config")
      .then(async (c) => {
        setCfg(c);
        if (!c.enabled) return;
        try {
          const r = await shopApi<Conversation>("GET", "/chat/conversation");
          if (r.status !== "CLOSED") {
            setConv({ id: r.conversation_id, status: r.status });
            setMessages(r.messages);
          }
        } catch {
          // 401 = belum ada percakapan
        }
      })
      .catch(() => setCfg(null));
  }, []);

  // Balasan staf / AI lewat SSE (cookie same-origin).
  const convId = conv?.id;
  useEffect(() => {
    if (!convId) return;
    const es = new EventSource("/_api/public/chat/stream");
    es.addEventListener("chat.message", (ev) => {
      try {
        const m = JSON.parse((ev as MessageEvent).data) as ChatMessage;
        setMessages((l) => merge(l, [m]));
      } catch { /* abaikan event rusak */ }
    });
    es.addEventListener("chat.status", (ev) => {
      try {
        const s = JSON.parse((ev as MessageEvent).data) as { status: string };
        setConv((c) => (c ? { ...c, status: s.status } : c));
      } catch { /* abaikan */ }
    });
    return () => es.close();
  }, [convId]);

  useEffect(() => {
    logRef.current?.lastElementChild?.scrollIntoView({ block: "end" });
  }, [messages, open]);

  const run = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    setErr(null);
    try {
      await fn();
    } catch (e) {
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  if (!cfg?.enabled) return null;

  const begin = () => run(async () => {
    const r = await shopApi<Conversation>("POST", "/chat/conversations", {
      json: { consent_version: cfg.consent_version, accept: true },
    });
    setConv({ id: r.conversation_id, status: r.status });
    setMessages(r.messages);
  });

  function send(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    return run(async () => {
      const r = await shopApi<Conversation>("POST", "/chat/messages", { json: { body } });
      form.reset();
      setMessages((l) => merge(l, r.messages));
      setConv((c) => (c ? { ...c, status: r.status } : c));
    });
  }

  const handover = () => run(async () => {
    const r = await shopApi<Conversation>("POST", "/chat/handover", { json: {} });
    setMessages((l) => merge(l, r.messages));
    setConv((c) => (c ? { ...c, status: r.status } : c));
  });

  const end = () => run(async () => {
    await shopApi("POST", "/chat/end", { json: {} });
    setConv(null);
    setMessages([]);
  });

  const waiting = conv?.status === "WAITING_HUMAN";
  return (
    <div className="chat">
      {!open && (
        <button type="button" className="chat-fab" onClick={() => setOpen(true)} aria-expanded="false">
          Chat
        </button>
      )}
      {open && (
        <section className="chat-panel" role="dialog" aria-label="Chat dengan toko">
          <header className="chat-head">
            <strong>Chat dengan toko</strong>
            <button type="button" className="chat-x" onClick={() => setOpen(false)} aria-label="Tutup chat">×</button>
          </header>
          {err && <p className="chat-err" role="alert">{err}</p>}
          {!conv ? (
            <div className="chat-consent">
              <p>{cfg.consent_text}</p>
              <button type="button" className="chat-btn" disabled={busy} onClick={begin}>Setuju &amp; mulai chat</button>
            </div>
          ) : (
            <>
              <ol className="chat-log" ref={logRef} aria-live="polite" aria-label="Pesan">
                {messages.map((m) => (
                  <li key={m.id} className={`chat-msg chat-${m.sender.toLowerCase()}`}>
                    <span className="chat-who">{SENDER_LABEL[m.sender]}</span>
                    <span className="chat-body">{m.body}</span>
                  </li>
                ))}
              </ol>
              {waiting && <p className="chat-note" role="status">Menunggu staf toko…</p>}
              <form className="chat-form" onSubmit={send}>
                <label className="chat-sr" htmlFor="chat-input">Tulis pesan</label>
                <textarea id="chat-input" name="body" rows={2} maxLength={4000} required placeholder="Tulis pesan…" />
                <button className="chat-btn" disabled={busy}>Kirim</button>
              </form>
              <div className="chat-actions">
                {conv.status === "AI" && <button type="button" className="chat-link" disabled={busy} onClick={handover}>Bicara dengan admin</button>}
                <button type="button" className="chat-link" disabled={busy} onClick={end}>Akhiri chat</button>
              </div>
            </>
          )}
        </section>
      )}
    </div>
  );
}
