"use client";

// Widget web chat situs toko (P6-2, ADR-0006). Same-origin ke /_api/public/chat/* (tenant dari Host).
// Sesi chat = cookie HttpOnly host-only (tidak terbaca JS); pemberitahuan AI (D-12) wajib disetujui
// sebelum pesan pertama. Isi pesan dirender sebagai teks biasa (React meng-escape), bukan HTML.
//
// SATU tampilan untuk situs publik dan Studio: `ChatShell` adalah markup murni (peluncur + panel),
// `ChatWidget` mengisinya dengan percakapan sungguhan, `ChatPreview` (Studio) mengisinya dengan contoh
// percakapan tanpa memanggil API. Dengan begitu bentuk chatbot di kanvas = bentuk di situs terbit.

import { useCallback, useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode, type Ref } from "react";

import { ApiError } from "@/lib/api";
import { onColor } from "@/renderer/safe";
import { shopApi } from "@/shop/api";

import "./chat.css";
import { intlLocale, t, tmap } from "@/i18n";
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
export interface ChatLook {
  variant?: ChatVariant; title?: string; welcome?: string; startOpen?: boolean; primary?: string;
  /** Gaya blok dari Studio (Polos/Kartu/Warna lembut/Gelap/3D lembut/2D datar) — berlaku di semua varian. */
  look?: string;
}

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

const IconPerson = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="8" r="4" /><path d="M4 21a8 8 0 0 1 16 0" /></svg>
);
const IconEnd = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9" /></svg>
);
const IconShield = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3 4 6v6c0 4.4 3.4 8.3 8 9 4.6-.7 8-4.6 8-9V6Z" /><path d="m9 12 2 2 4-4" /></svg>
);

/** Pertanyaan cepat: pembeli tidak perlu mengetik untuk memulai. */
export const SUGGESTIONS = (): string[] => [t("Jam buka toko?"), t("Cara memesan?"), t("Info ongkos kirim")];

/** Label pemisah hari untuk percakapan yang dipulihkan: "Hari ini" atau tanggalnya. */
function dayLabel(iso?: string): string | undefined {
  if (!iso) return undefined;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime()) || d.toDateString() === new Date().toDateString()) return undefined;
  return d.toLocaleDateString(intlLocale(), { day: "numeric", month: "long" });
}

function clock(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleTimeString(intlLocale(), { hour: "2-digit", minute: "2-digit" });
}

/** Satu bentuk daftar pesan untuk situs & Studio: pesan beruntun dari pengirim yang sama dikelompokkan
 *  (label di pesan pertama, avatar & jam di pesan terakhir) — seperti aplikasi chat yang dikenal pembeli. */
function ChatLog({ messages, heading, typing, label, logRef, preview, day }: {
  messages: (Pick<ChatMessage, "id" | "sender" | "body"> & { time?: string })[];
  heading: string; typing?: boolean; label: string; logRef?: Ref<HTMLOListElement>; preview?: boolean; day?: string;
}) {
  return (
    <ol className="chat-log" ref={logRef} aria-live={preview ? undefined : "polite"} aria-label={label}>
      <li className="chat-day" aria-hidden="true"><span>{day ?? t("Hari ini")}</span></li>
      {messages.map((m, i) => {
        if (m.sender === "SYSTEM") return <li key={m.id} className="chat-msg chat-system"><span className="chat-body">{m.body}</span></li>;
        const first = messages[i - 1]?.sender !== m.sender;
        const last = messages[i + 1]?.sender !== m.sender;
        const them = m.sender !== "CUSTOMER";
        return (
          <li key={m.id} className={`chat-msg chat-${m.sender.toLowerCase()}${first ? " chat-first" : ""}${last ? " chat-last" : ""}`}>
            {them && <span className="chat-msg-avatar" aria-hidden="true">{last && (m.sender === "ADMIN" ? <IconPerson /> : initial(heading))}</span>}
            <span className="chat-bubble">
              <span className={first ? "chat-who" : "chat-sr"}>{SENDER_LABEL[m.sender]}</span>
              <span className="chat-body">{m.body}</span>
              {last && m.time && <time className="chat-time">{m.time}</time>}
            </span>
          </li>
        );
      })}
      {typing && (
        <li className="chat-msg chat-ai chat-first chat-last chat-typing-row" aria-label={t("Mengetik")}>
          <span className="chat-msg-avatar" aria-hidden="true">{initial(heading)}</span>
          <span className="chat-typing"><i /><i /><i /></span>
        </li>
      )}
    </ol>
  );
}

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
  look?: string;
  /** Balasan yang masuk saat panel tertutup (lencana di peluncur). */
  unread?: number;
}

function themeStyle(primary?: string): CSSProperties | undefined {
  return primary ? ({ ["--chat-primary" as string]: primary, ["--chat-on-primary" as string]: onColor(primary) } as CSSProperties) : undefined;
}

function initial(s: string): string {
  return (s.trim()[0] ?? "C").toUpperCase();
}

export function ChatShell({ variant, heading, welcome, primary, open, onOpen, onClose, status, children, preview, off, look, unread = 0 }: ShellProps) {
  const badge = unread > 0 && <span className="chat-unread" aria-label={t("{0} pesan baru", unread)}>{unread > 9 ? "9+" : unread}</span>;
  const cls = `chat chat-v-${variant}${open ? " chat-is-open" : ""}${preview ? " chat-preview" : ""}${off ? " chat-off" : ""}`;
  return (
    <div className={cls} style={themeStyle(primary)} data-look={look || undefined} data-editor-preview={preview ? "" : undefined}>
      {!open && variant === "bubble" && (
        <button type="button" className="chat-fab" data-interactive onClick={onOpen} aria-expanded="false">
          <IconChat />
          <span>{t("Chat")}</span>
          {badge}
        </button>
      )}
      {!open && variant === "bar" && (
        <button type="button" className="chat-bar" data-interactive onClick={onOpen} aria-expanded="false" aria-label={t("Chat")}>
          <span className="chat-avatar chat-avatar-sm" aria-hidden="true">{initial(heading)}</span>
          <span className="chat-bar-who">{heading}</span>
          <span className="chat-bar-input">{welcome?.trim() || t("Tulis pertanyaan…")}</span>
          <span className="chat-bar-go" aria-hidden="true"><IconSend /></span>
          {badge}
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
export function ChatWidget({ variant = "bubble", title, welcome, startOpen = false, primary, look }: ChatLook = {}) {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [open, setOpen] = useState(startOpen);
  const [conv, setConv] = useState<{ id: string; status: string } | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState("");
  const [unread, setUnread] = useState(0);
  const logRef = useRef<HTMLOListElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const openRef = useRef(open);
  useEffect(() => { openRef.current = open; }, [open]);

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
        if (!openRef.current && m.sender !== "CUSTOMER") setUnread((n) => n + 1);
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
  }, [messages, open, busy]);

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

  async function post(body: string) {
    const r = await shopApi<Conversation>("POST", "/chat/messages", { json: { body } });
    setMessages((l) => merge(l, r.messages));
    setConv((c) => (c ? { ...c, status: r.status } : c));
  }

  /** Setujui pemberitahuan AI (D-12); `first` = pertanyaan cepat yang langsung dikirim. */
  const begin = (first?: string) => run(async () => {
    const r = await shopApi<Conversation>("POST", "/chat/conversations", {
      json: { consent_version: cfg.consent_version, accept: true },
    });
    setConv({ id: r.conversation_id, status: r.status });
    setMessages(r.messages);
    if (first) await post(first);
    requestAnimationFrame(() => inputRef.current?.focus());
  });

  function send(e?: FormEvent<HTMLFormElement>) {
    e?.preventDefault();
    const body = draft.trim();
    if (!body || busy) return;
    return run(async () => {
      await post(body);
      setDraft("");
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
    setDraft("");
  });

  const waiting = conv?.status === "WAITING_HUMAN";
  const heading = title?.trim() || t("Chat dengan toko");
  const status = waiting ? t("Menunggu staf toko…") : conv?.status === "HUMAN" ? t("Staf toko membalas") : t("Online · biasanya membalas cepat");
  const asked = messages.some((m) => m.sender === "CUSTOMER");
  return (
    <ChatShell
      variant={variant} look={look} heading={heading} welcome={welcome} primary={primary} open={open} unread={unread}
      onOpen={() => { setOpen(true); setUnread(0); }} onClose={() => setOpen(false)} status={status}
    >
      {err && <p className="chat-err" role="alert">{err}</p>}
      {!conv ? (
        <div className="chat-consent">
          <div className="chat-intro">
            <span className="chat-avatar chat-avatar-lg" aria-hidden="true">{initial(heading)}</span>
            <p className="chat-intro-title">{t("Halo! 👋")}</p>
            <p className="chat-intro-text">{welcome?.trim() || t("Ada yang bisa kami bantu? Tanyakan produk, harga, atau pesanan Anda.")}</p>
          </div>
          <div className="chat-chips" role="group" aria-label={t("Pertanyaan cepat")}>
            {SUGGESTIONS().map((q) => <button key={q} type="button" className="chat-chip" disabled={busy} onClick={() => void begin(q)}>{q}</button>)}
          </div>
          <div className="chat-consent-card">
            <p><span className="chat-consent-icon"><IconShield /></span>{consentText(cfg.consent_text)}</p>
            <button type="button" className="chat-btn" disabled={busy} onClick={() => void begin()}>{t("Setuju & mulai chat")}</button>
          </div>
        </div>
      ) : (
        <>
          <ChatLog
            logRef={logRef} heading={heading} typing={busy} label={t("Pesan")} day={dayLabel(messages[0]?.created_at)}
            messages={messages.map((m) => ({ ...m, time: clock(m.created_at) }))}
          />
          {waiting && <p className="chat-note" role="status">{t("Menunggu staf toko…")}</p>}
          {!asked && !busy && (
            <div className="chat-chips chat-chips-dock" role="group" aria-label={t("Pertanyaan cepat")}>
              {SUGGESTIONS().map((q) => <button key={q} type="button" className="chat-chip" onClick={() => void run(() => post(q))}>{q}</button>)}
            </div>
          )}
          <form className="chat-form" onSubmit={send}>
            <div className="chat-compose">
              <label className="chat-sr" htmlFor="chat-input">{t("Tulis pesan")}</label>
              <textarea
                id="chat-input" ref={inputRef} name="body" rows={1} maxLength={4000} placeholder={t("Tulis pesan…")}
                value={draft} onChange={(e) => setDraft(e.target.value)} enterKeyHint="send"
                // Enter = kirim; Shift+Enter = baris baru. Abaikan Enter saat IME masih menyusun kata.
                onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); } }}
              />
              <button type="submit" className="chat-send" disabled={busy || !draft.trim()} aria-label={t("Kirim")}><IconSend /></button>
            </div>
          </form>
          <div className="chat-actions">
            {conv.status === "AI" && <button type="button" className="chat-pill" disabled={busy} onClick={handover}><IconPerson />{t("Bicara dengan admin")}</button>}
            <button type="button" className="chat-pill chat-pill-quiet" disabled={busy} onClick={end}><IconEnd />{t("Akhiri chat")}</button>
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
export function ChatPreview({ variant = "bubble", title, welcome, primary, look, open, onOpenChange, ready = null }: ChatPreviewProps) {
  const heading = title?.trim() || t("Chat dengan toko");
  const off = ready === false;
  return (
    <ChatShell
      preview off={off}
      variant={variant} look={look} heading={heading} welcome={welcome} primary={primary}
      open={open} onOpen={() => onOpenChange?.(true)} onClose={() => onOpenChange?.(false)}
      status={off ? t("Nonaktif — tidak tampil di situs") : t("Online · biasanya membalas cepat")}
    >
      {off && <p className="chat-note chat-note-warn">{t("Aktifkan chatbot di menu Asisten AI agar tampil untuk pembeli.")}</p>}
      <ChatLog
        preview heading={heading} label={t("Contoh percakapan")}
        messages={[
          { id: "1", sender: "AI", body: welcome?.trim() || t("Halo! Ada yang bisa kami bantu?"), time: "09.41" },
          { id: "2", sender: "CUSTOMER", body: t("Kopi susunya masih ada?"), time: "09.42" },
          { id: "3", sender: "AI", body: t("Masih, kak! Tersedia ukuran Regular dan Large 😊"), time: "09.42" },
        ]}
      />
      <div className="chat-form" aria-hidden="true">
        <div className="chat-compose">
          <span className="chat-fake-input">{t("Tulis pesan…")}</span>
          <span className="chat-send"><IconSend /></span>
        </div>
      </div>
      <div className="chat-actions" aria-hidden="true">
        <span className="chat-pill"><IconPerson />{t("Bicara dengan admin")}</span>
        <span className="chat-pill chat-pill-quiet"><IconEnd />{t("Akhiri chat")}</span>
      </div>
    </ChatShell>
  );
}
