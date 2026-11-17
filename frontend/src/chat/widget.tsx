"use client";

// Widget web chat situs toko (P6-2, ADR-0006). Same-origin ke /_api/public/chat/* (tenant dari Host).
// Sesi chat = cookie HttpOnly host-only (tidak terbaca JS); pemberitahuan AI (D-12) wajib disetujui
// sebelum pesan pertama. Isi pesan dirender sebagai teks biasa (React meng-escape), bukan HTML.
//
// SATU tampilan untuk situs publik dan Studio: `ChatShell` adalah markup murni (peluncur + panel),
// `ChatWidget` mengisinya dengan percakapan sungguhan, `ChatPreview` (Studio) mengisinya dengan contoh
// percakapan tanpa memanggil API. Dengan begitu bentuk chatbot di kanvas = bentuk di situs terbit.

import { useCallback, useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode } from "react";

import { ApiError } from "@/lib/api";
import { onColor } from "@/renderer/safe";
import { shopApi } from "@/shop/api";

import "./chat.css";
import { t, tmap } from "@/i18n";
import { consentText, serverMessage } from "@/i18n/server";

export interface ChatMessage { id: string; seq: number; sender: "CUSTOMER" | "AI" | "ADMIN" | "SYSTEM"; body: string; created_at: string }
interface Conversation { conversation_id: string; status: string; messages: ChatMessage[] }
interface Config { enabled: boolean; consent_version: string; consent_text: string }

export const SENDER_LABEL: Record<ChatMessage["sender"], string> = tmap({
  CUSTOMER: "Anda",
  AI: "Asisten AI",
  ADMIN: "Staf toko",
  SYSTEM: "Info",
});

/** Gabungkan pesan baru tanpa duplikat, urut seq. */
export function merge(list: ChatMessage[], add: ChatMessage[]): ChatMessage[] {
  const seen = new Set(list.map((m) => m.id));
  const out = [...list, ...add.filter((m) => !seen.has(m.id))];
  return out.sort((a, b) => a.seq - b.seq);
}

function errorText(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 429) return t("Terlalu banyak pesan. Coba lagi sebentar lagi.");
    if (e.status === 422) return t("Pesan tidak valid (1–4000 karakter).");
    return serverMessage(e.code, e.message);
  }
  return t("Tidak dapat menghubungi toko.");
}

/** Tampilan widget (plugin Chatbot Studio, ADR-0017): gelembung bulat, bilah bawah, atau menempel di halaman. */
export type ChatVariant = "bubble" | "bar" | "inline";
export interface ChatLook { variant?: ChatVariant; title?: string; welcome?: string; startOpen?: boolean; primary?: string }

/* ------------------------------------------------------------------ ikon (SVG sebaris, tanpa dependensi) */
const IconChat = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 11.5a8.4 8.4 0 0 1-12.2 7.5L3 21l2-5.6A8.4 8.4 0 1 1 21 11.5Z" />
  </svg>
);
const IconSend = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="currentColor"><path d="M3.4 20.4 21.5 12 3.4 3.6l-.1 6.5L15 12 3.3 13.9Z" /></svg>
);
const IconClose = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"><path d="M6 6l12 12M18 6 6 18" /></svg>
);
const IconMin = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>
);
const IconUp = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 15 6-6 6 6" /></svg>
);

/* ------------------------------------------------------------------ cangkang tampilan (dipakai bersama) */
interface ShellProps {
  variant: ChatVariant;
  heading: string;
  welcome?: string;
  primary?: string;
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  /** Baris status di kepala panel (mis. "Asisten AI · online"). */
  status: ReactNode;
  /** Isi panel di bawah kepala. */
  children: ReactNode;
  /** Studio: penanda pratinjau (tanpa API) + kelas tambahan. */
  preview?: boolean;
  /** Chatbot dimatikan di Asisten AI (hanya pratinjau Studio). */
  off?: boolean;
}

function themeStyle(primary?: string): CSSProperties | undefined {
  return primary ? ({ ["--chat-primary" as string]: primary, ["--chat-on-primary" as string]: onColor(primary) } as CSSProperties) : undefined;
}

function initial(s: string): string {
  return (s.trim()[0] ?? "C").toUpperCase();
}

export function ChatShell({ variant, heading, welcome, primary, open, onOpen, onClose, status, children, preview, off }: ShellProps) {
  const cls = `chat chat-v-${variant}${open ? " chat-is-open" : ""}${preview ? " chat-preview" : ""}${off ? " chat-off" : ""}`;
  return (
    <div className={cls} style={themeStyle(primary)} data-editor-preview={preview ? "" : undefined}>
      {!open && variant === "bubble" && (
        <button type="button" className="chat-fab" data-interactive onClick={onOpen} aria-expanded="false">
          <IconChat />
          <span>{t("Chat")}</span>
        </button>
      )}
      {!open && variant === "bar" && (
        <button type="button" className="chat-bar" data-interactive onClick={onOpen} aria-expanded="false" aria-label={t("Chat")}>
          <span className="chat-avatar chat-avatar-sm" aria-hidden="true">{initial(heading)}</span>
          <span className="chat-bar-who">{heading}</span>
          <span className="chat-bar-input">{welcome?.trim() || t("Tulis pertanyaan…")}</span>
          <span className="chat-bar-go" aria-hidden="true"><IconSend /></span>
        </button>
      )}
      {!open && variant === "inline" && (
        <button type="button" className="chat-inline-min" data-interactive onClick={onOpen} aria-expanded="false" aria-label={t("Chat")}>
          <span className="chat-avatar chat-avatar-sm" aria-hidden="true">{initial(heading)}</span>
          <span className="chat-inline-min-text"><strong>{heading}</strong><span>{welcome?.trim() || t("Ketuk untuk mulai chat")}</span></span>
          <IconUp />
        </button>
      )}
      {open && (
        <section className="chat-panel" role="dialog" aria-label={t("Chat dengan toko")}>
          <header className="chat-head">
            <span className="chat-avatar" aria-hidden="true">{initial(heading)}<i className="chat-presence" /></span>
            <span className="chat-head-text">
              <strong>{heading}</strong>
              <span className="chat-status">{status}</span>
            </span>
            <button type="button" className="chat-x" data-interactive onClick={onClose} aria-label={variant === "inline" ? t("Minimalkan chat") : t("Tutup chat")}>
              {variant === "inline" ? <IconMin /> : <IconClose />}
            </button>
          </header>
          {children}
        </section>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ widget sungguhan (situs publik) */
export function ChatWidget({ variant = "bubble", title, welcome, startOpen = false, primary }: ChatLook = {}) {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [open, setOpen] = useState(startOpen);
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
  const heading = title?.trim() || t("Chat dengan toko");
  const status = waiting ? t("Menunggu staf toko…") : conv?.status === "HUMAN" ? t("Staf toko membalas") : t("Online · biasanya membalas cepat");
  return (
    <ChatShell variant={variant} heading={heading} welcome={welcome} primary={primary} open={open} onOpen={() => setOpen(true)} onClose={() => setOpen(false)} status={status}>
      {err && <p className="chat-err" role="alert">{err}</p>}
      {!conv ? (
        <div className="chat-consent">
          {welcome?.trim() && <p className="chat-msg chat-ai chat-welcome"><span className="chat-body">{welcome}</span></p>}
          <div className="chat-consent-card">
            <p>{consentText(cfg.consent_text)}</p>
            <button type="button" className="chat-btn" disabled={busy} onClick={begin}>{t("Setuju & mulai chat")}</button>
          </div>
        </div>
      ) : (
        <>
          <ol className="chat-log" ref={logRef} aria-live="polite" aria-label={t("Pesan")}>
            {messages.map((m) => (
              <li key={m.id} className={`chat-msg chat-${m.sender.toLowerCase()}`}>
                <span className="chat-who">{SENDER_LABEL[m.sender]}</span>
                <span className="chat-body">{m.body}</span>
              </li>
            ))}
            {busy && <li className="chat-typing" aria-label={t("Mengetik")}><i /><i /><i /></li>}
          </ol>
          {waiting && <p className="chat-note" role="status">{t("Menunggu staf toko…")}</p>}
          <form className="chat-form" onSubmit={send}>
            <label className="chat-sr" htmlFor="chat-input">{t("Tulis pesan")}</label>
            <textarea
              id="chat-input" name="body" rows={1} maxLength={4000} required placeholder={t("Tulis pesan…")}
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); e.currentTarget.form?.requestSubmit(); } }}
            />
            <button type="submit" className="chat-send" disabled={busy} aria-label={t("Kirim")}><IconSend /></button>
          </form>
          <div className="chat-actions">
            {conv.status === "AI" && <button type="button" className="chat-link" disabled={busy} onClick={handover}>{t("Bicara dengan admin")}</button>}
            <button type="button" className="chat-link" disabled={busy} onClick={end}>{t("Akhiri chat")}</button>
          </div>
        </>
      )}
    </ChatShell>
  );
}

/* ------------------------------------------------------------------ pratinjau Studio (tanpa API) */
export interface ChatPreviewProps extends ChatLook {
  /** Keadaan buka/tutup dikendalikan editor (disimpan sebagai `chat_open`). */
  open: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Chatbot aktif di Asisten AI? false = tandai "tidak tampil di situs". null = belum diketahui. */
  ready?: boolean | null;
}

/** Contoh percakapan dengan markup & CSS yang sama persis dengan widget di situs terbit. */
export function ChatPreview({ variant = "bubble", title, welcome, primary, open, onOpenChange, ready = null }: ChatPreviewProps) {
  const heading = title?.trim() || t("Chat dengan toko");
  const off = ready === false;
  return (
    <ChatShell
      preview off={off}
      variant={variant} heading={heading} welcome={welcome} primary={primary}
      open={open} onOpen={() => onOpenChange?.(true)} onClose={() => onOpenChange?.(false)}
      status={off ? t("Nonaktif — tidak tampil di situs") : t("Online · biasanya membalas cepat")}
    >
      {off && <p className="chat-note chat-note-warn">{t("Aktifkan chatbot di menu Asisten AI agar tampil untuk pembeli.")}</p>}
      <ol className="chat-log" aria-label={t("Contoh percakapan")}>
        <li className="chat-msg chat-ai"><span className="chat-who">{SENDER_LABEL.AI}</span><span className="chat-body">{welcome?.trim() || t("Halo! Ada yang bisa kami bantu?")}</span></li>
        <li className="chat-msg chat-customer"><span className="chat-who">{SENDER_LABEL.CUSTOMER}</span><span className="chat-body">{t("Kopi susunya masih ada?")}</span></li>
        <li className="chat-msg chat-ai"><span className="chat-who">{SENDER_LABEL.AI}</span><span className="chat-body">{t("Masih, kak! Tersedia ukuran Regular dan Large 😊")}</span></li>
      </ol>
      <div className="chat-form" aria-hidden="true">
        <span className="chat-fake-input">{t("Tulis pesan…")}</span>
        <span className="chat-send"><IconSend /></span>
      </div>
    </ChatShell>
  );
}
