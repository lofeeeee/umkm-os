// Bahasa antarmuka (ID/EN). Gaya "gettext": teks Indonesia di kode adalah KUNCI sekaligus teks
// bawaan; `t()` mengembalikan terjemahan dari kamus saat bahasa = EN.
//
// - Bukan hook: aman dipakai di komponen, handler, modul, dan komponen server.
// - Bahasa disimpan di localStorage; mengganti bahasa memasang ulang pohon aplikasi (I18nRoot) agar
//   semua teks — termasuk konstanta modul yang memakai getter — dirender ulang.
// - Kelengkapan kamus dijaga tes (src/i18n/catalog.test.ts): setiap t("…") di kode wajib punya
//   terjemahan EN dengan placeholder {0}, {1}, … yang sama.
// - Kamus EN adalah chunk terpisah (dynamic import): pengguna bahasa Indonesia — mayoritas, termasuk
//   pengunjung situs toko — tidak mengunduhnya. Bahasa baru berlaku setelah kamus termuat.
// Konten buatan pemilik toko (nama produk, teks situs) TIDAK diterjemahkan.

export interface EnglishDict {
  EN: Record<string, string>;
  SERVER_EN: Record<string, string>;
  CONSENT_EN: Record<string, string>;
}
let dict: EnglishDict | null = null;
let loading: Promise<EnglishDict> | null = null;

/** Muat kamus English (sekali; dipakai juga oleh tes sebelum setLocale("en")). */
export function loadEnglish(): Promise<EnglishDict> {
  loading ??= import("./en").then(
    (m) => (dict = { EN: m.EN, SERVER_EN: m.SERVER_EN, CONSENT_EN: m.CONSENT_EN }),
    (e: unknown) => { loading = null; throw e; },
  );
  return loading;
}
/** Kamus English bila sudah termuat. */
export const englishDict = (): EnglishDict | null => dict;

export type Locale = "id" | "en";
export const LOCALES: { value: Locale; label: string; short: string }[] = [
  { value: "id", label: "Bahasa Indonesia", short: "ID" },
  { value: "en", label: "English", short: "EN" },
];
const STORAGE_KEY = "umkm-lang";

let current: Locale = "id";
/** Bahasa yang terakhir diminta (kamus EN mungkin masih diunduh). */
let wanted: Locale = "id";
const listeners = new Set<() => void>();

export const getLocale = (): Locale => current;

/** Bahasa yang tersimpan (default Indonesia; bukan dari bahasa browser supaya konsisten untuk UMKM). */
export function storedLocale(): Locale {
  try {
    return localStorage.getItem(STORAGE_KEY) === "en" ? "en" : "id";
  } catch {
    return "id";
  }
}

export function setLocale(l: Locale, persist = true) {
  if (persist) {
    try { localStorage.setItem(STORAGE_KEY, l); } catch { /* mode privat: tetap berlaku di sesi ini */ }
  }
  wanted = l;
  if (l === "en" && !dict) {
    void loadEnglish().then(() => { if (wanted === "en") apply("en"); }, () => { /* jaringan gagal: tetap Indonesia */ });
    return;
  }
  apply(l);
}

function apply(l: Locale) {
  if (l === current) return;
  current = l;
  if (typeof document !== "undefined") document.documentElement.lang = l;
  for (const f of listeners) f();
}

export function subscribeLocale(f: () => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

/** Locale Intl untuk tanggal/angka. Rupiah tetap "Rp" di kedua bahasa. */
export const intlLocale = (): string => (current === "en" ? "en-GB" : "id-ID");

function fill(s: string, args: readonly unknown[]): string {
  return args.length === 0 ? s : s.replace(/\{(\d+)\}/g, (m, i: string) => (Number(i) < args.length ? String(args[Number(i)]) : m));
}

/** Terjemahkan teks antarmuka. Placeholder posisi: t("Halaman {0} dibuat.", judul). */
export function t(id: string, ...args: unknown[]): string {
  const s = current === "en" && dict ? (dict.EN[id] ?? id) : id;
  return fill(s, args);
}

/**
 * Peta kode → teks (mis. status, izin) yang diterjemahkan SAAT DIBACA, sehingga konstanta modul ikut
 * berganti bahasa. Nilainya tetap kunci Indonesia (diekstrak oleh catalog.test.ts).
 */
export function tmap<T extends Record<string, string>>(o: T): T {
  return new Proxy(o, {
    get: (target, k) => {
      const v = Reflect.get(target, k);
      return typeof v === "string" ? t(v) : v;
    },
  });
}

/** Penanda kunci terjemahan tanpa menerjemahkan (untuk larik modul yang diterjemahkan saat tampil: t(x)). */
export const m = <S extends string>(s: S): S => s;
