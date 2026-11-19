"use client";

// Kerangka halaman masuk/daftar/undangan: satu kolom berpusat pada form. Tanpa panel merek
// setengah layar dan daftar fitur berikon (pola template SaaS; audit anti-slop Nov 2026).

import { useState, type ReactNode } from "react";

import { Eye, EyeSlash, Storefront } from "@phosphor-icons/react/ssr";

import { ThemeToggle } from "@/dashboard/shell";
import { t } from "@/i18n";
import { Tr } from "@/i18n/react";
import { LanguageSwitch } from "@/i18n/react";

export function AuthShell({ pitch, children }: { pitch: string; children: ReactNode }) {
  return (
    <div className="d-auth">
      <header className="d-auth-top">
        <div className="d-auth-mark"><span aria-hidden="true"><Storefront size={16} weight="bold" /></span>{t("UMKM OS")}</div>
        <div className="d-auth-theme"><LanguageSwitch /><ThemeToggle /></div>
      </header>
      <div className="d-auth-form">
        <main id="main" className="d-narrow" tabIndex={-1}>
          {children}
        </main>
      </div>
      <footer className="d-auth-foot">
        <p>{pitch}. {t("Website, katalog, pesanan, pelanggan, dan asisten AI, dikelola dari satu dasbor.")}</p>
        <p><Tr s="© {0} UMKM OS" a={[new Date().getFullYear()]} /></p>
      </footer>
    </div>
  );
}

/** Input password dengan tombol tampilkan/sembunyikan. */
export function PasswordInput(props: { name: string; autoComplete: string; minLength?: number; label?: string; value?: string; onChange?: (v: string) => void; hint?: ReactNode }) {
  const [show, setShow] = useState(false);
  return (
    <label className="d-field">
      <span>{props.label ?? t("Password")}</span>
      <span className="d-password">
        <input
          name={props.name}
          // Label membungkus tombol "Tampilkan" juga; nama aksesibel input = label saja.
          aria-label={props.label ?? t("Password")}
          type={show ? "text" : "password"}
          autoComplete={props.autoComplete}
          required={!props.onChange}
          minLength={props.minLength}
          value={props.value}
          onChange={props.onChange ? (e) => props.onChange?.(e.target.value) : undefined}
        />
        <button type="button" className="d-icon-btn" onClick={() => setShow(!show)} aria-label={show ? t("Sembunyikan karakter") : t("Tampilkan karakter")} title={show ? t("Sembunyikan") : t("Tampilkan")} aria-pressed={show}>
          {show ? <EyeSlash weight="bold" /> : <Eye weight="bold" />}
        </button>
      </span>
      {props.hint && <span className="d-hint">{props.hint}</span>}
    </label>
  );
}
