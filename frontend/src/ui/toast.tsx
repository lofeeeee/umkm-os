"use client";

// Toast global (gaya Sonner): panggil `toast.ok("Tersimpan.")` dari mana saja; <Toaster/> dipasang
// sekali di layout akar. Umpan balik sukses = toast; error validasi tetap inline di dekat isiannya.
// Setiap toast ber-role="status" (aria-live polite) supaya pembaca layar ikut membacakan.

import { useEffect, useState, useSyncExternalStore } from "react";

import { CheckCircle, Info, WarningCircle, X } from "@phosphor-icons/react/ssr";
import { t as tx } from "@/i18n";

type Kind = "ok" | "error" | "info";
interface Item { id: number; kind: Kind; text: string; leaving?: boolean }

let items: Item[] = [];
let seq = 0;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const EMPTY: Item[] = [];

function push(kind: Kind, text: string) {
  const id = ++seq;
  // Pesan identik berturut-turut cukup satu (mis. klik simpan dua kali).
  items = [...items.filter((t) => t.text !== text), { id, kind, text }].slice(-4);
  emit();
  return id;
}

export function dismissToast(id: number) {
  items = items.map((t) => (t.id === id ? { ...t, leaving: true } : t));
  emit();
  setTimeout(() => {
    items = items.filter((t) => t.id !== id);
    emit();
  }, 160);
}

export const toast = {
  ok: (text: string) => push("ok", text),
  error: (text: string) => push("error", text),
  info: (text: string) => push("info", text),
};

const ICON = { ok: CheckCircle, error: WarningCircle, info: Info } as const;

function ToastItem({ t }: { t: Item }) {
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (paused || t.leaving) return;
    // Error bertahan lebih lama; timer berhenti saat kursor di atas toast atau tab tersembunyi.
    const ms = t.kind === "error" ? 7000 : 4000;
    let timer = setTimeout(() => dismissToast(t.id), ms);
    const onVis = () => {
      clearTimeout(timer);
      if (document.visibilityState === "visible") timer = setTimeout(() => dismissToast(t.id), ms);
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [paused, t]);
  const Icon = ICON[t.kind];
  return (
    <div
      className="d-toast"
      data-kind={t.kind}
      data-leaving={t.leaving ? "true" : undefined}
      role={t.kind === "error" ? "alert" : "status"}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <Icon weight="fill" aria-hidden="true" />
      <span>{t.text}</span>
      <button type="button" className="d-icon-btn" aria-label={tx("Tutup notifikasi")} onClick={() => dismissToast(t.id)}>
        <X weight="bold" />
      </button>
    </div>
  );
}

export function Toaster() {
  const list = useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    () => items,
    () => EMPTY,
  );
  return (
    <div className="d-toaster" aria-label={tx("Notifikasi singkat")}>
      {list.map((t) => <ToastItem key={t.id} t={t} />)}
    </div>
  );
}
