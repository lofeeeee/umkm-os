// Validasi telepon & email untuk sisi peramban.
//
// Aturan ini **harus sama** dengan aturan di backend (`crates/domain/src/business.rs`)
// dan dengan CHECK constraint di DB (migrasi 0042). Kalau berbeda, pengguna mendapat
// pesan "tidak valid" padahal data ditolak di tempat lain (atau sebaliknya).
//
// Telepon:
//  - Hanya angka setelah spasi/`-`/`.`/`(`/`)` dibuang.
//  - `+` hanya boleh tepat sebelum `62`.
//  - Awalan `0` (lokal) atau `62` (internasional).
//  - Panjang 9–15 digit.
//
// Email:
//  - Satu `@`; bagian lokal & domain tak kosong; domain punya titik; TLD ≥ 2 huruf.
//
// Fungsi ini mengembalikan **kode galat** (bukan teks), supaya teksnya ikut diterjemahkan
// lewat kamus galat yang sudah ada (`ERROR_TEXT` di `@/lib/api`, `ERR` di `@/shop/views`).

/** Kode galat — sama dengan yang dipakai backend (`FieldError::code`). */
export type ContactErrorCode = "invalid_phone" | "invalid_email";

/** Batas jumlah digit nomor telepon (tanpa `+`). */
export const PHONE_MIN_DIGITS = 9;
export const PHONE_MAX_DIGITS = 15;

/** Buang pemisah visual: spasi, `-`, `.`, `(`, `)`. */
function stripSeparators(s: string): string {
  let out = "";
  for (const c of s) {
    if (c === " " || c === "-" || c === "." || c === "(" || c === ")") continue;
    out += c;
  }
  return out;
}

/**
 * Nomor telepon → digit kanonik (tanpa `+`), atau `null` bila tidak sah.
 *
 * `null` berarti "tidak valid", bukan "kosong" — pemanggil harus membedakan keduanya.
 */
export function normalizePhoneId(s: string): string | null {
  const t = stripSeparators(s);
  let digits: string;
  if (t.startsWith("+")) {
    const rest = t.slice(1);
    if (!rest.startsWith("62")) return null;
    digits = rest;
  } else {
    digits = t;
  }
  if (digits.length === 0) return null;
  for (const c of digits) {
    if (c < "0" || c > "9") return null;
  }
  if (!digits.startsWith("0") && !digits.startsWith("62")) return null;
  if (digits.length < PHONE_MIN_DIGITS || digits.length > PHONE_MAX_DIGITS) return null;
  return digits;
}

/** True bila nomor telepon sah (setara `normalize_phone_id` di Rust). */
export function isValidPhone(s: string): boolean {
  return normalizePhoneId(s) !== null;
}

/**
 * Email valid?
 *
 * Cerminan `is_valid_email` di Rust. Bedanya sengaja: versi Rust memakai `trim()`,
 * pemanggil di peramban sudah memangkas spasi sebelum memanggil.
 */
export function isValidEmail(s: string): boolean {
  if (s.length === 0 || s.length > 254) return false;
  for (const c of s) {
    // spasi & karakter kontrol ditolak
    if (/\s/.test(c)) return false;
    const code = c.charCodeAt(0);
    if (code < 32 || code === 127) return false;
  }
  const at = s.indexOf("@");
  if (at === -1 || at !== s.lastIndexOf("@")) return false;
  const local = s.slice(0, at);
  const domain = s.slice(at + 1);
  if (local.length === 0 || local.length > 64) return false;
  if (local.startsWith(".") || local.endsWith(".")) return false;
  if (local.includes("..")) return false;
  for (const c of local) {
    if (!/[A-Za-z0-9!#$%&'*+/=?^_`{|}~.-]/.test(c)) return false;
  }
  if (domain.length === 0 || domain.length > 253) return false;
  if (domain.startsWith(".") || domain.endsWith(".")) return false;
  const labels = domain.split(".");
  if (labels.length < 2) return false;
  for (const l of labels) {
    if (l.length === 0 || l.length > 63) return false;
    if (l.startsWith("-") || l.endsWith("-")) return false;
    for (const c of l) {
      if (!/[A-Za-z0-9-]/.test(c)) return false;
    }
  }
  const tld = labels[labels.length - 1]!;
  if (tld.length < 2) return false;
  for (const c of tld) {
    if (!/[A-Za-z]/.test(c)) return false;
  }
  return true;
}

/**
 * Kode galat untuk nomor telepon, atau `null` bila sah (atau kosong — kosong bukan galat).
 */
export function phoneErrorCode(value: string): ContactErrorCode | null {
  if (value.trim() === "") return null;
  return isValidPhone(value) ? null : "invalid_phone";
}

/**
 * Kode galat untuk email, atau `null` bila sah (atau kosong — kosong bukan galat).
 */
export function emailErrorCode(value: string): ContactErrorCode | null {
  const v = value.trim();
  if (v === "") return null;
  return isValidEmail(v) ? null : "invalid_email";
}

/**
 * Atribut `pattern` HTML untuk isian telepon.
 *
 * Hanya angka (opsional `+62` di depan), awalan `0`/`62`, total 9–15 digit.
 * Dipakai sebagai pagar terakhir di peramban; validasi tetap di JS + backend.
 */
export const PHONE_PATTERN = "^(?:\\+?62|0)[0-9]{8,14}$";
