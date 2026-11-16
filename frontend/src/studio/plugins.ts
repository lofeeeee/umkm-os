// Katalog plugin resmi (prototipe ADR-0017). Setiap plugin = komponen whitelist dengan data tertutup;
// sebagian besar memakai ulang renderer section ADR-0007 sehingga tampilan editor = situs publik.

import type { SectionType } from "@/renderer/types";

import type { Json } from "@/renderer/types";

import manifest from "./plugin-manifest.json";

export type PluginKind = SectionType | "chatbot" | "text" | "whatsapp";
export type PluginCategory = "Konten" | "Jualan" | "Layanan" | "Info toko";

export interface PluginDef {
  kind: PluginKind;
  name: string;
  desc: string;
  category: PluginCategory;
  /** Ukuran awal (kolom × baris) dan minimum. */
  w: number; h: number; minW: number; minH: number;
  /** Varian tata letak khusus plugin (opsional). */
  layouts?: { value: string; label: string }[];
  keywords: string;
}

export const PLUGINS: PluginDef[] = [
  { kind: "hero", name: "Banner", desc: "Judul besar, slogan, tombol ajakan, dan foto utama.", category: "Konten", w: 12, h: 9, minW: 4, minH: 2, keywords: "hero header utama judul" },
  { kind: "product_grid", name: "Galeri produk", desc: "Tampilkan produk yang dijual — grid, carousel, atau daftar.", category: "Jualan", w: 12, h: 11, minW: 4, minH: 2,
    layouts: [{ value: "grid", label: "Grid" }, { value: "carousel", label: "Carousel" }, { value: "list", label: "Daftar" }], keywords: "katalog item jual toko etalase" },
  { kind: "chatbot", name: "Chatbot AI", desc: "Asisten AI yang menjawab pembeli. Bisa menempel di halaman atau melayang.", category: "Layanan", w: 4, h: 9, minW: 3, minH: 2,
    layouts: [{ value: "inline", label: "Menempel" }, { value: "floating", label: "Gelembung" }, { value: "bar", label: "Bilah" }], keywords: "chat bot ai asisten bantuan cs" },
  { kind: "promotion", name: "Promo", desc: "Kotak promo mencolok dengan tombol.", category: "Jualan", w: 6, h: 6, minW: 3, minH: 2, keywords: "diskon penawaran sale" },
  { kind: "about", name: "Tentang kami", desc: "Cerita usaha dengan foto.", category: "Konten", w: 6, h: 7, minW: 3, minH: 2, keywords: "profil cerita teks" },
  { kind: "gallery", name: "Galeri foto", desc: "Kumpulan foto suasana atau hasil karya.", category: "Konten", w: 12, h: 8, minW: 4, minH: 2, keywords: "foto gambar portofolio" },
  { kind: "testimonials", name: "Testimoni", desc: "Ulasan dari pelanggan.", category: "Konten", w: 12, h: 7, minW: 4, minH: 2, keywords: "review ulasan" },
  { kind: "business_hours", name: "Jam buka", desc: "Jadwal buka dan status buka sekarang.", category: "Info toko", w: 4, h: 8, minW: 3, minH: 2, keywords: "jadwal waktu" },
  { kind: "location", name: "Lokasi", desc: "Alamat toko dan petunjuk arah.", category: "Info toko", w: 4, h: 5, minW: 3, minH: 2, keywords: "alamat peta map" },
  { kind: "text", name: "Teks", desc: "Paragraf bebas: pengumuman, cerita, atau info penting.", category: "Konten", w: 6, h: 4, minW: 2, minH: 2, keywords: "paragraf tulisan artikel pengumuman" },
  { kind: "whatsapp", name: "Tombol WhatsApp", desc: "Tombol chat ke nomor WhatsApp di Profil bisnis — melayang di pojok atau di halaman.", category: "Jualan", w: 4, h: 3, minW: 2, minH: 2,
    layouts: [{ value: "floating", label: "Melayang" }, { value: "inline", label: "Di halaman" }], keywords: "wa chat pesan kontak order" },
  { kind: "contact", name: "Kontak", desc: "Tombol WhatsApp, telepon, dan media sosial.", category: "Info toko", w: 4, h: 5, minW: 3, minH: 2, keywords: "whatsapp telepon sosial" },
];

export const pluginDef = (k: PluginKind) => PLUGINS.find((p) => p.kind === k)!;

/** ADR-0019: paket minimum plugin dari manifest (sama dengan `plugin_plan` server; tak dikenal = pro). */
export function pluginPlan(k: string): "starter" | "pro" {
  const p = (manifest.plugins as Record<string, { plan?: string }>)[k]?.plan;
  return p === "starter" ? "starter" : "pro";
}
/** Blok tidak tampil di situs publik untuk paket ini. `planCode` null = belum diketahui (jangan menandai). */
export const needsUpgrade = (plan: "starter" | "pro", planCode: string | null) => plan === "pro" && planCode !== null && planCode !== "pro";

/** Data awal plugin di luar renderer section v1 (bentuk = validator server schema_v2.rs). */
export const EXTRA_DATA: Partial<Record<PluginKind, { [k: string]: Json }>> = {
  chatbot: { title: "Tanya kami", welcome: "Halo! Ada yang bisa kami bantu?" },
  text: { title: "Pengumuman", body: "Tulis informasi penting untuk pembeli di sini." },
  whatsapp: { label: "Pesan via WhatsApp", message: "Halo, saya mau tanya produk di toko Anda." },
};

export type CellStyle = "plain" | "card" | "tint" | "dark";
export const CELL_STYLES: { value: CellStyle; label: string }[] = [
  { value: "plain", label: "Polos" },
  { value: "card", label: "Kartu" },
  { value: "tint", label: "Warna lembut" },
  { value: "dark", label: "Gelap" },
];

/** Palet siap pakai: warna utama + warna teks di atas warna utama + permukaan halaman.
 *  Wajib: contrast(primary, onPrimary) ≥ 4.5 (WCAG AA) — dijaga lewat plugins.test.ts. */
export interface Palette { name: string; primary: string; onPrimary: string; surface: "light" | "cream" | "dark" }
export const PALETTES: Palette[] = [
  { name: "Senja",  primary: "#C2410C", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Indigo", primary: "#4F46E5", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Daun",   primary: "#15803D", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Laut",   primary: "#0369A1", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Mawar",  primary: "#BE185D", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Kopi",   primary: "#78350F", onPrimary: "#FEF3C7", surface: "cream" },
  { name: "Malam",  primary: "#7C3AED", onPrimary: "#FFFFFF", surface: "dark" },
  { name: "Emas",   primary: "#CA8A04", onPrimary: "#1C1917", surface: "dark" },
  { name: "Sawi",   primary: "#4D7C0F", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Terong", primary: "#7E22CE", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Bata",   primary: "#9F1239", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Sian",   primary: "#0E7490", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Kuningan", primary: "#B45309", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Lumut",  primary: "#3F6212", onPrimary: "#F7FEE7", surface: "dark" },
  { name: "Abu",    primary: "#3F3F46", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Merah",  primary: "#DC2626", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Toska",  primary: "#0F766E", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Koral",  primary: "#E11D48", onPrimary: "#FFFFFF", surface: "light" },
];
