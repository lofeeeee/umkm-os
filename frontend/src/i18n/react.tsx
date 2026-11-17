"use client";

import { Fragment, useEffect, useSyncExternalStore, type ReactNode } from "react";

import { getLocale, LOCALES, setLocale, storedLocale, subscribeLocale, t, type Locale } from "./index";
import { translateTitle } from "./titles";

/** Judul asli (bahasa Indonesia, dari metadata Next) dan judul terakhir yang kita pasang. */
const titleState = { src: "", applied: "" };

/**
 * Terjemahkan judul tab browser. Next memasang judul berbahasa Indonesia dari metadata (juga saat
 * navigasi); observer menangkap setiap perubahan itu sebagai judul sumber lalu memasang terjemahannya.
 */
function useTranslatedTitle(locale: Locale) {
  useEffect(() => {
    const apply = () => {
      titleState.applied = translateTitle(titleState.src);
      if (document.title !== titleState.applied) document.title = titleState.applied;
    };
    // Judul sekarang adalah sumber kecuali itu hasil terjemahan kita sendiri (ganti bahasa).
    if (document.title !== titleState.applied) titleState.src = document.title;
    apply();
    const obs = new MutationObserver(() => {
      if (document.title === titleState.applied) return;
      titleState.src = document.title;
      apply();
    });
    obs.observe(document.head, { subtree: true, childList: true, characterData: true });
    return () => obs.disconnect();
  }, [locale]);
}

/**
 * Pembungkus akar. HTML statis selalu dirender dalam bahasa Indonesia (hidrasi aman); setelah mount
 * bahasa tersimpan diterapkan. Mengganti bahasa memasang ulang subtree (key) sehingga semua teks —
 * termasuk yang dihitung sekali per pemasangan — ikut berganti.
 */
export function I18nRoot({ children }: { children: ReactNode }) {
  const locale = useLocale();
  useEffect(() => { setLocale(storedLocale(), false); }, []);
  useTranslatedTitle(locale);
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
