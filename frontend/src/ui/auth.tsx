"use client";

// Kerangka halaman masuk/daftar/undangan: panel merek di kiri (desktop), form di kanan.

import { useState, type ReactNode } from "react";

import { ChatCircleDots, Eye, EyeSlash, Globe, Storefront, Wallet } from "@phosphor-icons/react/ssr";

import { ThemeToggle } from "@/dashboard/shell";
import { t } from "@/i18n";
import { Tr } from "@/i18n/react";
import { LanguageSwitch } from "@/i18n/react";

const FEATS = [
  { icon: Globe, get title() { return t("Website siap pakai"); }, get desc() { return t("Pilih templat, ubah teks & foto, terbitkan dalam menit."); } },
  { icon: Wallet, get title() { return t("Pesanan & pembayaran QRIS"); }, get desc() { return t("Pembeli bayar langsung ke rekening toko, Anda cukup konfirmasi."); } },
  { icon: ChatCircleDots, get title() { return t("Asisten AI 24 jam"); }, get desc() { return t("Jawab pertanyaan pelanggan otomatis, serahkan ke staf bila perlu."); } },
];

export function AuthShell({ pitch, children }: { pitch: string; children: ReactNode }) {
  return (
    <div className="d-auth">
      <div className="d-auth-brand" aria-hidden="true">
        <div className="d-auth-logo"><span><Storefront size={18} weight="bold" /></span>{t("UMKM OS")}</div>
        <div className="d-auth-pitch">
          <h2>{pitch}</h2>
          <p>{t("Website, katalog, pesanan, pelanggan, dan asisten AI, dikelola dari satu dasbor.")}</p>
        </div>
        <ul className="d-auth-feats">
          {FEATS.map((f) => (
            <li key={f.title}><f.icon weight="bold" /><div><strong>{f.title}</strong><span>{f.desc}</span></div></li>
          ))}
        </ul>
        <p className="d-auth-foot"><Tr s="© {0} UMKM OS" a={[new Date().getFullYear()]} /></p>
      </div>
      <div className="d-auth-form">
        <div className="d-auth-theme"><LanguageSwitch /><ThemeToggle /></div>
        <main id="main" className="d-narrow" tabIndex={-1}>
          <div className="d-auth-mark"><span><Storefront size={16} weight="bold" /></span>{t("UMKM OS")}</div>
          {children}
        </main>
      </div>
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
