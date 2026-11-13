// Katalog plugin resmi (prototipe ADR-0017). Setiap plugin = komponen whitelist dengan data tertutup;
// sebagian besar memakai ulang renderer section ADR-0007 sehingga tampilan editor = situs publik.

import type { SectionType } from "@/renderer/types";

export type PluginKind = SectionType | "chatbot";
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
  { kind: "contact", name: "Kontak", desc: "Tombol WhatsApp, telepon, dan media sosial.", category: "Info toko", w: 4, h: 5, minW: 3, minH: 2, keywords: "whatsapp telepon sosial" },
];

export const pluginDef = (k: PluginKind) => PLUGINS.find((p) => p.kind === k)!;

export type CellStyle = "plain" | "card" | "tint" | "dark";
export const CELL_STYLES: { value: CellStyle; label: string }[] = [
  { value: "plain", label: "Polos" },
  { value: "card", label: "Kartu" },
  { value: "tint", label: "Warna lembut" },
  { value: "dark", label: "Gelap" },
];

/** Palet siap pakai: warna utama + permukaan halaman. */
export interface Palette { name: string; primary: string; surface: "light" | "cream" | "dark" }
export const PALETTES: Palette[] = [
  { name: "Senja", primary: "#C2410C", surface: "cream" },
  { name: "Indigo", primary: "#4F46E5", surface: "light" },
  { name: "Daun", primary: "#15803D", surface: "light" },
  { name: "Laut", primary: "#0369A1", surface: "light" },
  { name: "Mawar", primary: "#BE185D", surface: "cream" },
  { name: "Kopi", primary: "#92400E", surface: "cream" },
  { name: "Malam", primary: "#A78BFA", surface: "dark" },
  { name: "Emas", primary: "#CA8A04", surface: "dark" },
];
