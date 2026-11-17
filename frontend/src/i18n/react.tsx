"use client";

import { Fragment, useEffect, useSyncExternalStore, type ReactNode } from "react";

import { getLocale, LOCALES, setLocale, storedLocale, subscribeLocale, t, type Locale } from "./index";

/**
 * Pembungkus akar. HTML statis selalu dirender dalam bahasa Indonesia (hidrasi aman); setelah mount
 * bahasa tersimpan diterapkan. Mengganti bahasa memasang ulang subtree (key) sehingga semua teks —
 * termasuk yang dihitung sekali per pemasangan — ikut berganti.
 */
export function I18nRoot({ children }: { children: ReactNode }) {
  const locale = useLocale();
  useEffect(() => { setLocale(storedLocale(), false); }, []);
  return <Fragment key={locale}>{children}</Fragment>;
}

export function useLocale(): Locale {
  return useSyncExternalStore(subscribeLocale, getLocale, () => "id" as Locale);
}

/**
 * Kalimat yang memuat elemen React di tengahnya: <Tr s="Buka {0} lalu simpan." a={[<Link …/>]} />.
 * Urutan kata mengikuti terjemahan, jadi placeholder boleh berpindah posisi di bahasa lain.
 */
export function Tr({ s, a = [] }: { s: string; a?: ReactNode[] }) {
  const parts = t(s).split(/(\{\d+\})/);
  return (
    <>
      {parts.map((p, i) => {
        const m = /^\{(\d+)\}$/.exec(p);
        return <Fragment key={i}>{m ? a[Number(m[1])] : p}</Fragment>;
      })}
    </>
  );
}

/** Pemilih bahasa ringkas (ID | EN). */
export function LanguageSwitch({ className = "" }: { className?: string }) {
  const locale = useLocale();
  return (
    <div className={`lang-switch ${className}`.trim()} role="group" aria-label={t("Bahasa")}>
      {LOCALES.map((l) => (
        <button key={l.value} type="button" lang={l.value} aria-pressed={locale === l.value} title={l.label} onClick={() => setLocale(l.value)}>
          {l.short}
        </button>
      ))}
    </div>
  );
}
