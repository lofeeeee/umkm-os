"use client";

// Isian kontak (telepon / email) dengan validasi langsung.
//
// Pola ini mengikuti `CoordField` (src/ui/coord-field.tsx): komponen **ter kendali**
// (controlled) — teks isian berasal dari state formulir di halaman induk, jadi tidak ada
// setState di dalam efek maupun akses ref saat render.
//
// Yang dijaga di sini:
//  1. **Aturan sama dengan backend & DB.** Telepon: hanya angka, diawali `0`/`62`, 9–15 digit
//     (`crates/domain/src/business.rs` → `normalize_phone_id`). Email: satu `@`, domain
//     bertitik, TLD ≥ 2 huruf (`is_valid_email`). Salinan peramban: `src/lib/contact.ts`.
//     DB: CHECK constraint di migrasi 0042.
//  2. **Pesan galat memakai KODE** (`invalid_phone` / `invalid_email`), bukan teks mentah,
//     supaya ikut diterjemahkan lewat kamus galat (`ERROR_TEXT` / `ERR`).
//  3. **Isian kosong bukan galat** — kolom kontak boleh dikosongkan.
//  4. **Tidak memblokir submit.** Galat tampil langsung, tapi tombol simpan tetap bisa
//     ditekan: server tetap jadi penentu (pesan lengkap & konsisten dari API).

import { useId } from "react";

import { t as tx } from "@/i18n";
import { emailErrorCode, phoneErrorCode, PHONE_PATTERN } from "@/lib/contact";

export type ContactFieldKind = "phone" | "email";

/** Teks galat per kode — dipakai bila halaman ini tidak punya kamus galat sendiri. */
const FALLBACK: Record<"invalid_phone" | "invalid_email", string> = {
  invalid_phone: "Nomor telepon harus hanya angka dan diawali 0 atau 62 (9–15 digit).",
  invalid_email: "Format email tidak valid.",
};

/**
 * Isian telepon atau email dengan galat langsung.
 *
 * `errorText` overriding kode → teks galat (mis. `ERROR_TEXT` dari `@/lib/api`).
 */
export function ContactField({
  label,
  kind,
  value,
  onChange,
  disabled,
  hint,
  errorText,
  required,
  autoComplete,
  maxLength,
  placeholder,
  className = "d-field",
  errorClassName = "d-field-error",
} : {
  label: string;
  kind: ContactFieldKind;
  /** Teks isian (bukan angka) — `null` = kosong. */
  value: string | null;
  /** Teks baru yang diketik pengguna. */
  onChange: (v: string) => void;
  disabled?: boolean;
  hint?: string;
  /** Pemetaan kode galat → teks (dari kamus galat yang dipakai halaman). */
  errorText?: Record<string, string>;
  required?: boolean;
  autoComplete?: string;
  maxLength?: number;
  placeholder?: string;
  /** Kelas pembungkus isian (`d-field` di dasbor, `shop-field` di situs toko). */
  className?: string;
  /** Kelas pesan galat. */
  errorClassName?: string;
}) {
  const id = useId();
  const errId = `${id}-err`;
  const hintId = `${id}-hint`;
  const code = kind === "phone" ? phoneErrorCode(value ?? "") : emailErrorCode(value ?? "");
  const err = code ? (errorText?.[code] ?? FALLBACK[code]) : null;

  return (
    <label className={className} htmlFor={id}>
      <span id={`${id}-label`}>{label}</span>
      <input
        id={id}
        type={kind === "phone" ? "tel" : "email"}
        inputMode={kind === "phone" ? "numeric" : "email"}
        pattern={kind === "phone" ? PHONE_PATTERN : undefined}
        autoComplete={autoComplete}
        maxLength={maxLength}
        placeholder={placeholder}
        required={required}
        value={value ?? ""}
        disabled={disabled}
        aria-invalid={err ? true : undefined}
        aria-describedby={err ? errId : hint ? hintId : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {hint && !err && <span className="d-hint" id={hintId}>{hint}</span>}
      {err && (
        <span className={errorClassName} id={errId} role="alert">
          {err}
        </span>
      )}
    </label>
  );
}
