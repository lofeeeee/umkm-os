"use client";

// Kerangka halaman masuk/daftar/undangan: panel merek di kiri (desktop), form di kanan.

import { useState, type ReactNode } from "react";

import { ChatCircleDots, Eye, EyeSlash, Globe, Storefront, Wallet } from "@phosphor-icons/react/ssr";

import { ThemeToggle } from "@/dashboard/shell";

const FEATS = [
  { icon: Globe, title: "Website siap pakai", desc: "Pilih template, ubah teks & foto, terbitkan dalam menit." },
  { icon: Wallet, title: "Pesanan & pembayaran QRIS", desc: "Pembeli bayar langsung ke rekening toko, Anda cukup konfirmasi." },
  { icon: ChatCircleDots, title: "Asisten AI 24 jam", desc: "Jawab pertanyaan pelanggan otomatis, serahkan ke staf bila perlu." },
];

export function AuthShell({ pitch, children }: { pitch: string; children: ReactNode }) {
  return (
    <div className="d-auth">
      <div className="d-auth-brand" aria-hidden="true">
        <div className="d-auth-logo"><span><Storefront size={18} weight="bold" /></span>UMKM OS</div>
        <div className="d-auth-pitch">
          <h2>{pitch}</h2>
          <p>Website, katalog, pesanan, pelanggan, dan asisten AI — dikelola dari satu dasbor.</p>
        </div>
        <ul className="d-auth-feats">
          {FEATS.map((f) => (
            <li key={f.title}><f.icon weight="bold" /><div><strong>{f.title}</strong><span>{f.desc}</span></div></li>
          ))}
        </ul>
        <p className="d-auth-foot">© {new Date().getFullYear()} UMKM OS</p>
      </div>
      <div className="d-auth-form">
        <div className="d-auth-theme"><ThemeToggle /></div>
        <main id="main" className="d-narrow" tabIndex={-1}>
          <div className="d-auth-mark"><span><Storefront size={16} weight="bold" /></span>UMKM OS</div>
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
      <span>{props.label ?? "Password"}</span>
      <span className="d-password">
        <input
          name={props.name}
          type={show ? "text" : "password"}
          autoComplete={props.autoComplete}
          required={!props.onChange}
          minLength={props.minLength}
          value={props.value}
          onChange={props.onChange ? (e) => props.onChange?.(e.target.value) : undefined}
        />
        <button type="button" className="d-icon-btn" onClick={() => setShow(!show)} aria-label={show ? "Sembunyikan karakter" : "Tampilkan karakter"} title={show ? "Sembunyikan" : "Tampilkan"} aria-pressed={show}>
          {show ? <EyeSlash weight="bold" /> : <Eye weight="bold" />}
        </button>
      </span>
      {props.hint && <span className="d-hint">{props.hint}</span>}
    </label>
  );
}
