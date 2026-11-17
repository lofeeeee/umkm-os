// Pesan dari API (Rust) → bahasa antarmuka. Server menulis pesan dalam bahasa Indonesia (atau teks
// generik berbahasa Inggris dari ErrorCode); frontend menerjemahkannya di sini sehingga server tetap
// satu bahasa. Daftar kunci = kamus src/i18n/en/server-6.ts; server.test.ts memeriksa bahwa setiap
// pesan di kode Rust punya padanan (gagal bila server menambah pesan tanpa terjemahan).

import { CONSENT_EN, SERVER_EN } from "./en/server-6";
import { getLocale, tmap } from "./index";

/** Pesan pengganti untuk teks generik ErrorCode (server mengirim bahasa Inggris tanpa konteks). */
const GENERIC: Record<string, string> = tmap({
  BAD_REQUEST: "Permintaan tidak dapat diproses.",
  UNAUTHORIZED: "Sesi Anda berakhir. Silakan masuk lagi.",
  FORBIDDEN: "Anda tidak punya izin untuk aksi ini.",
  RESOURCE_NOT_FOUND: "Data tidak ditemukan.",
  METHOD_NOT_ALLOWED: "Aksi ini tidak didukung.",
  REQUEST_TIMEOUT: "Permintaan terlalu lama. Coba lagi.",
  CONFLICT: "Data sudah berubah. Muat ulang lalu coba lagi.",
  PAYLOAD_TOO_LARGE: "File atau data terlalu besar.",
  UNSUPPORTED_MEDIA_TYPE: "Format file tidak didukung.",
  VALIDATION_FAILED: "Periksa kembali isian Anda.",
  RATE_LIMITED: "Terlalu banyak percobaan. Coba lagi sebentar lagi.",
  INTERNAL_ERROR: "Terjadi kesalahan di server. Coba lagi nanti.",
  SERVICE_UNAVAILABLE: "Layanan sedang tidak tersedia. Coba lagi nanti.",
});

/** Teks bawaan ErrorCode di server (crates/shared/src/http.rs `default_message`). */
export const ERRORCODE_DEFAULTS = [
  "Bad request", "Authentication required", "Permission denied", "Resource not found", "Method not allowed",
  "Request timed out", "Conflict", "Payload too large", "Unsupported media type", "Validation failed",
  "Too many requests", "Internal server error", "Service unavailable",
];
const DEFAULTS = new Set(ERRORCODE_DEFAULTS);

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/** Templat berparameter ("stok tidak cukup (tersedia {0}, diminta {1})") → regex pencocok. */
const PATTERNS = Object.keys(SERVER_EN)
  .filter((k) => /\{\d+\}/.test(k))
  .map((k) => ({ key: k, re: new RegExp(`^${esc(k).replace(/\\\{(\d+)\\\}/g, "(.+?)")}$`) }));

const capitalize = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);
const fill = (s: string, args: string[]) => s.replace(/\{(\d+)\}/g, (m, i: string) => args[Number(i)] ?? m);

/**
 * Pesan server dalam bahasa antarmuka. Pesan tak dikenal ditampilkan apa adanya; pesan generik
 * ErrorCode diganti kalimat yang bisa ditindaklanjuti. Huruf pertama dikapitalkan.
 */
export function serverMessage(code: string, message: string): string {
  const en = getLocale() === "en";
  if (!message || DEFAULTS.has(message)) return GENERIC[code] ?? capitalize(message);
  if (message in SERVER_EN) return capitalize(en ? SERVER_EN[message]! : message);
  for (const p of PATTERNS) {
    const m = p.re.exec(message);
    if (m) return capitalize(en ? fill(SERVER_EN[p.key]!, m.slice(1)) : message);
  }
  return capitalize(message);
}

/** Teks persetujuan AI (D-12) dari server, dalam bahasa antarmuka. Versi persetujuan tidak berubah. */
export function consentText(text: string): string {
  return getLocale() === "en" && CONSENT_EN[text] ? CONSENT_EN[text]! : text;
}
