// Koordinat geografis untuk isian manual (Profil bisnis → Alamat).
//
// Dua aturan yang berbeda:
//  1. **Nilai** boleh negatif — latitude Indonesia ada di belahan bumi selatan (mis. -6.2°),
//     bujur bisa negatif di belahan barat. Jadi `-6.2`/`-106.8` harus bisa diketik & disimpan.
//  2. **Rentang** tetap dijaga: latitude -90..90, bujur -180..180 (sama dengan
//     `validate_coordinates` di Rust dan CHECK constraint PostgreSQL).
//
// Catatan: angka negatif tidak boleh ditolak oleh pengurai. `Number("-")` menghasilkan NaN,
// jadi saat pengguna baru mengetik tanda minus, isian harus tetap bisa menyimpan teks "-"
// (bukan NaN) sampai digit berikutnya masuk. Karena itu teks diketik disimpan apa adanya.

/** Rentang latitude (bujur) yang sah. */
export const LAT_RANGE = { min: -90, max: 90 } as const;
export const LNG_RANGE = { min: -180, max: 180 } as const;

/**
 * Bersihkan teks koordinat yang diketik pengguna.
 *
 * Mengembalikan teks yang sudah dibersihkan **apa adanya** (bukan angka), supaya:
 *  - tanda minus tak lengkap ("-") tetap tampil dan tidak jadi "NaN";
 *  - titik desimal di tengah pengetikan ("6.", "-6.") tidak hilang sebelum digit berikutnya.
 *
 * Mengembalikan `null` bila isian kosong atau bukan Awalan koordinat yang masuk akal
 * (mis. "abc" atau "1e5") — isian seperti itu tidak pernah sampai ke server.
 */
export function parseCoord(raw: string | null): string | null {
  const s = (raw ?? "").trim();
  if (s === "") return null;
  // Awalan koordinat yang sah: tanda (+/-) opsional, lalu digit dan/atau satu titik desimal.
  if (!/^[+-]?(\d+(\.\d*)?|\.\d+)?$/.test(s)) return null;
  // Hanya tanda (mis. "-") atau angka desimal belum selesai → tetap dikembalikan sebagai teks.
  return s;
}

/** Nilai numerik koordinat dari teks atau angka; `null` bila kosong atau belum valid. */
export function coordNumber(raw: string | number | null | undefined): number | null {
  if (raw === null || raw === undefined || raw === "") return null;
  if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
  const s = raw.trim();
  if (s === "" || !/^[+-]?(\d+(\.\d*)?|\.\d+)$/.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

/** Apakah nilai koordinat berada dalam rentang yang sah? */
export function coordInRange(v: number | null, range: { min: number; max: number }): boolean {
  if (v === null) return true; // opsional = boleh kosong
  return Number.isFinite(v) && v >= range.min && v <= range.max;
}
