// Spesifikasi field editor per tipe section — cermin ADR-0007 / crates/domain/src/website/schema.rs.
// Default section baru ada di section-defaults.json dan DIUJI oleh validator Rust
// (crates/domain/tests/frontend_contract.rs) agar editor tidak pernah membuat dokumen invalid.

import defaults from "./section-defaults.json";
import type { Json, Section, SectionType } from "./types";

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

const title = (max = 120, required = false): Field => ({ key: "title", label: "Judul", kind: "text", max, required });
const cta: Field = { key: "cta", label: "Tombol", kind: "cta" };
const image = (key = "image", label = "Gambar"): Field => ({ key, label, kind: "image" });

export const SOCIAL_PLATFORMS = ["instagram", "facebook", "tiktok", "youtube", "x", "website"];
export const FONT_PRESETS = ["inter", "poppins", "nunito", "merriweather", "system"];

export const SECTION_LABELS: Record<SectionType, string> = {
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
};

export const FIELDS: Record<SectionType, Field[]> = {
  header: [
    title(80, true),
    image("logo", "Logo (kosong = logo profil bisnis)"),
    { key: "nav", label: "Menu", kind: "list", maxItems: 8, item: [
      { key: "label", label: "Teks", kind: "text", max: 40, required: true },
      { key: "href", label: "Tautan", kind: "href", required: true },
    ] },
  ],
  hero: [title(120, true), { key: "subtitle", label: "Subjudul", kind: "textarea", max: 240 }, image(), cta],
  about: [title(), { key: "body", label: "Isi", kind: "textarea", max: 2000, required: true }, image()],
  product_grid: [title(), { key: "limit", label: "Jumlah produk", kind: "number", min: 1, max: 24, required: true }],
  promotion: [title(120, true), { key: "body", label: "Isi", kind: "textarea", max: 500 }, image(), cta],
  gallery: [
    title(),
    { key: "images", label: "Gambar", kind: "list", maxItems: 24, item: [
      { key: "asset", label: "Gambar", kind: "image", required: true },
      { key: "caption", label: "Keterangan", kind: "text", max: 120 },
    ] },
  ],
  testimonials: [
    title(),
    { key: "items", label: "Testimoni", kind: "list", minItems: 1, maxItems: 12, item: [
      { key: "name", label: "Nama", kind: "text", max: 80, required: true },
      { key: "quote", label: "Kutipan", kind: "textarea", max: 500, required: true },
    ] },
  ],
  business_hours: [title()],
  location: [title(), { key: "show_map", label: "Tampilkan tautan peta", kind: "bool" }],
  contact: [
    title(),
    { key: "show_whatsapp", label: "Tampilkan WhatsApp", kind: "bool" },
    { key: "show_phone", label: "Tampilkan telepon", kind: "bool" },
    { key: "show_email", label: "Tampilkan email", kind: "bool" },
    { key: "socials", label: "Media sosial", kind: "list", maxItems: 10, item: [
      { key: "platform", label: "Platform", kind: "select", options: SOCIAL_PLATFORMS, required: true },
      { key: "url", label: "URL (https://)", kind: "href", required: true },
    ] },
  ],
  footer: [{ key: "text", label: "Teks", kind: "text", max: 240 }],
};

export function defaultData(type: SectionType): { [k: string]: Json } {
  return structuredClone((defaults as Record<SectionType, { [k: string]: Json }>)[type]);
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
