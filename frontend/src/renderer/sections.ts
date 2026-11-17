// Spesifikasi field editor per tipe section — cermin ADR-0007 / crates/domain/src/website/schema.rs.
// Default section baru ada di section-defaults.json dan DIUJI oleh validator Rust
// (crates/domain/tests/frontend_contract.rs) agar editor tidak pernah membuat dokumen invalid.

import defaults from "./section-defaults.json";
import type { Json, Section, SectionType } from "./types";
import { t, tmap, m } from "@/i18n";

export type FieldKind = "text" | "textarea" | "image" | "number" | "bool" | "href" | "select";

export interface Field {
  key: string;
  label: string;
  kind: FieldKind | "cta" | "list";
  max?: number;
  min?: number;
  required?: boolean;
  options?: string[];
  /** untuk kind = "list" */
  item?: Field[];
  maxItems?: number;
  minItems?: number;
}

const title = (max = 120, required = false): Field => ({ key: "title", get label() { return t("Judul"); }, kind: "text", max, required });
const cta: Field = { key: "cta", get label() { return t("Tombol"); }, kind: "cta" };
const image = (key = "image", label: string = m("Gambar")): Field => ({ key, get label() { return t(label); }, kind: "image" });

export const SOCIAL_PLATFORMS = ["instagram", "facebook", "tiktok", "youtube", "x", "website"];
export const FONT_PRESETS = ["jakarta", "inter", "poppins", "nunito", "merriweather", "fraunces", "system"];
/** Nama tampilan preset font (pemilih font di Studio/editor). */
export const FONT_LABELS: Record<string, string> = tmap({
  jakarta: "Plus Jakarta Sans — modern, buatan Indonesia",
  inter: "Inter — netral & rapi",
  poppins: "Poppins — geometris & ramah",
  nunito: "Nunito — bulat & santai",
  merriweather: "Merriweather — klasik (serif)",
  fraunces: "Fraunces — elegan (serif)",
  system: "Font perangkat — paling ringan",
});

export const SECTION_LABELS: Record<SectionType, string> = tmap({
  header: "Kepala halaman & navigasi",
  hero: "Banner utama",
  about: "Tentang",
  product_grid: "Daftar produk",
  promotion: "Promo",
  gallery: "Galeri",
  testimonials: "Testimoni",
  business_hours: "Jam buka",
  location: "Lokasi",
  contact: "Kontak & media sosial",
  footer: "Kaki halaman",
});

export const FIELDS: Record<SectionType, Field[]> = {
  header: [
    title(80, true),
    image("logo", m("Logo (kosong = logo profil bisnis)")),
    { key: "nav", get label() { return t("Menu"); }, kind: "list", maxItems: 8, item: [
      { key: "label", get label() { return t("Teks"); }, kind: "text", max: 40, required: true },
      { key: "href", get label() { return t("Tautan"); }, kind: "href", required: true },
    ] },
  ],
  hero: [title(120, true), { key: "subtitle", get label() { return t("Subjudul"); }, kind: "textarea", max: 240 }, image(), cta],
  about: [title(), { key: "body", get label() { return t("Isi"); }, kind: "textarea", max: 2000, required: true }, image()],
  product_grid: [title(), { key: "limit", get label() { return t("Jumlah produk"); }, kind: "number", min: 1, max: 24, required: true }],
  promotion: [title(120, true), { key: "body", get label() { return t("Isi"); }, kind: "textarea", max: 500 }, image(), cta],
  gallery: [
    title(),
    { key: "images", get label() { return t("Gambar"); }, kind: "list", maxItems: 24, item: [
      { key: "asset", get label() { return t("Gambar"); }, kind: "image", required: true },
      { key: "caption", get label() { return t("Keterangan"); }, kind: "text", max: 120 },
    ] },
  ],
  testimonials: [
    title(),
    { key: "items", get label() { return t("Testimoni"); }, kind: "list", minItems: 1, maxItems: 12, item: [
      { key: "name", get label() { return t("Nama"); }, kind: "text", max: 80, required: true },
      { key: "quote", get label() { return t("Kutipan"); }, kind: "textarea", max: 500, required: true },
    ] },
  ],
  business_hours: [title()],
  location: [title(), { key: "show_map", get label() { return t("Tampilkan tautan peta"); }, kind: "bool" }],
  contact: [
    title(),
    { key: "show_whatsapp", get label() { return t("Tampilkan WhatsApp"); }, kind: "bool" },
    { key: "show_phone", get label() { return t("Tampilkan telepon"); }, kind: "bool" },
    { key: "show_email", get label() { return t("Tampilkan email"); }, kind: "bool" },
    { key: "socials", get label() { return t("Media sosial"); }, kind: "list", maxItems: 10, item: [
      { key: "platform", get label() { return t("Platform"); }, kind: "select", options: SOCIAL_PLATFORMS, required: true },
      { key: "url", get label() { return t("URL (https://)"); }, kind: "href", required: true },
    ] },
  ],
  footer: [{ key: "text", get label() { return t("Teks"); }, kind: "text", max: 240 }],
};

/** Kunci teks bawaan section (isi awal; ikut bahasa antarmuka saat blok dibuat). */
const DEFAULT_TEXT_KEYS = new Set(["title", "subtitle", "body", "text", "quote", "name"]);
const DEFAULT_TEXTS = [
  m("Nama Toko"), m("Judul utama"), m("Kalimat pendek tentang bisnis Anda"), m("Tentang Kami"), m("Ceritakan bisnis Anda di sini."),
  m("Produk"), m("Promo"), m("Tulis promo Anda"), m("Galeri"), m("Kata Pelanggan"), m("Pelanggan"), m("Pelayanannya ramah."),
  m("Jam Buka"), m("Lokasi"), m("Hubungi Kami"), m("Terima kasih telah berkunjung"),
];
const KNOWN_DEFAULTS = new Set<string>(DEFAULT_TEXTS);

function localizeDefaults(v: Json, key = ""): Json {
  if (typeof v === "string") return DEFAULT_TEXT_KEYS.has(key) && KNOWN_DEFAULTS.has(v) ? t(v) : v;
  if (Array.isArray(v)) return v.map((x) => localizeDefaults(x, key));
  if (v && typeof v === "object") return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, localizeDefaults(x as Json, k)]));
  return v;
}

/** Data awal section; teks contoh mengikuti bahasa antarmuka saat ini (konten milik pemilik toko setelahnya). */
export function defaultData(type: SectionType): { [k: string]: Json } {
  return localizeDefaults(structuredClone((defaults as Record<SectionType, { [k: string]: Json }>)[type])) as { [k: string]: Json };
}

/** Id unik `[a-z0-9-]` dalam halaman. */
export function newSectionId(type: SectionType, existing: Section[]): string {
  const base = type.replace(/_/g, "-");
  let n = 1;
  const ids = new Set(existing.map((s) => s.id));
  while (ids.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}

export function emptyItem(fields: Field[]): { [k: string]: Json } {
  const o: { [k: string]: Json } = {};
  for (const f of fields) {
    if (f.kind === "select") o[f.key] = f.options?.[0] ?? "";
    else if (f.required) o[f.key] = "";
  }
  return o;
}
