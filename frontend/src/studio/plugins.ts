// Katalog plugin resmi (prototipe ADR-0017). Setiap plugin = komponen whitelist dengan data tertutup;
// sebagian besar memakai ulang renderer section ADR-0007 sehingga tampilan editor = situs publik.

import type { SectionType } from "@/renderer/types";

import type { Json } from "@/renderer/types";

import manifest from "./plugin-manifest.json";
import { t, m } from "@/i18n";

export type CoreKind = "shop_cart" | "shop_orders" | "shop_account";
export type PluginKind = SectionType | "chatbot" | "text" | "whatsapp" | "background" | "image" | "shape" | "logo" | "article_body" | "blog_list" | CoreKind;
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
  /** Blok inti halaman sistem: tidak ada di katalog, tidak bisa dihapus/diduplikat (server: `check_core_blocks`). */
  core?: true;
}

export const PLUGINS: PluginDef[] = [
  { kind: "hero", get name() { return t("Banner"); }, get desc() { return t("Judul besar, slogan, tombol ajakan, dan foto utama."); }, category: "Konten", w: 12, h: 9, minW: 4, minH: 2, keywords: m("hero header utama judul") },
  { kind: "product_grid", get name() { return t("Galeri produk"); }, get desc() { return t("Tampilkan produk yang dijual: grid, carousel, atau daftar."); }, category: "Jualan", w: 12, h: 11, minW: 4, minH: 2,
    layouts: [{ value: "grid", get label() { return t("Grid"); } }, { value: "carousel", get label() { return t("Carousel"); } }, { value: "list", get label() { return t("Daftar"); } }], keywords: m("katalog item jual toko etalase") },
  { kind: "chatbot", get name() { return t("Chatbot AI"); }, get desc() { return t("Asisten AI yang menjawab pembeli. Bisa menempel di halaman atau melayang."); }, category: "Layanan", w: 4, h: 9, minW: 3, minH: 2,
    layouts: [{ value: "inline", get label() { return t("Menempel"); } }, { value: "floating", get label() { return t("Gelembung"); } }, { value: "bar", get label() { return t("Bilah"); } }], keywords: m("chat bot ai asisten bantuan cs") },
  { kind: "promotion", get name() { return t("Promo"); }, get desc() { return t("Kotak promo mencolok dengan tombol."); }, category: "Jualan", w: 6, h: 6, minW: 3, minH: 2, keywords: m("diskon penawaran sale") },
  { kind: "about", get name() { return t("Tentang kami"); }, get desc() { return t("Cerita usaha dengan foto."); }, category: "Konten", w: 6, h: 7, minW: 3, minH: 2, keywords: m("profil cerita teks") },
  { kind: "gallery", get name() { return t("Galeri foto"); }, get desc() { return t("Kumpulan foto suasana atau hasil karya."); }, category: "Konten", w: 12, h: 8, minW: 4, minH: 2, keywords: m("foto gambar portofolio") },
  { kind: "testimonials", get name() { return t("Testimoni"); }, get desc() { return t("Ulasan dari pelanggan."); }, category: "Konten", w: 12, h: 7, minW: 4, minH: 2, keywords: m("review ulasan") },
  { kind: "business_hours", get name() { return t("Jam buka"); }, get desc() { return t("Jadwal buka dan status buka sekarang."); }, category: "Info toko", w: 4, h: 8, minW: 3, minH: 2, keywords: m("jadwal waktu") },
  { kind: "location", get name() { return t("Lokasi"); }, get desc() { return t("Alamat toko dan petunjuk arah."); }, category: "Info toko", w: 4, h: 5, minW: 3, minH: 2, keywords: m("alamat peta map") },
  { kind: "text", get name() { return t("Teks"); }, get desc() { return t("Paragraf bebas: pengumuman, cerita, atau info penting."); }, category: "Konten", w: 6, h: 4, minW: 2, minH: 2, keywords: m("paragraf tulisan artikel pengumuman") },
  { kind: "whatsapp", get name() { return t("Tombol WhatsApp"); }, get desc() { return t("Tombol chat ke nomor WhatsApp di Profil bisnis, melayang di pojok atau di halaman."); }, category: "Jualan", w: 4, h: 3, minW: 2, minH: 2,
    layouts: [{ value: "floating", get label() { return t("Melayang"); } }, { value: "inline", get label() { return t("Di halaman"); } }], keywords: m("wa chat pesan kontak order") },
  { kind: "background", get name() { return t("Latar"); }, get desc() { return t("Gambar latar tembus: selalu di belakang blok lain. Seret, pindah, dan ubah ukurannya bebas."); }, category: "Konten", w: 12, h: 12, minW: 2, minH: 2, keywords: m("background latar belakang wallpaper gambar tembus") },
  { kind: "image", get name() { return t("Gambar"); }, get desc() { return t("Gambar bebas (PNG transparan, stiker, logo) yang bisa ditaruh di mana saja: di atas atau di belakang blok, menggantung saat digulir, atau popup di layar."); }, category: "Konten", w: 3, h: 6, minW: 1, minH: 2,
    layouts: [{ value: "above", get label() { return t("Di atas blok"); } }, { value: "below", get label() { return t("Di belakang blok"); } }, { value: "sticky", get label() { return t("Menggantung"); } }, { value: "popup", get label() { return t("Popup"); } }], keywords: m("gambar foto stiker png logo popup menggantung hiasan ilustrasi") },
  { kind: "shape", get name() { return t("Bentuk"); }, get desc() { return t("Hiasan vektor berwarna merek: blob, bintang, kilau, cincin, panah, gelombang, dan lainnya."); }, category: "Konten", w: 2, h: 4, minW: 1, minH: 1,
    layouts: [{ value: "above", get label() { return t("Di atas blok"); } }, { value: "below", get label() { return t("Di belakang blok"); } }, { value: "sticky", get label() { return t("Menggantung"); } }], keywords: m("bentuk hiasan dekorasi stiker bintang blob ornamen") },
  { kind: "logo", get name() { return t("Logo"); }, get desc() { return t("Logo toko dari Profil bisnis, bisa ditaruh di mana saja. Logo di menu navigasi otomatis disembunyikan agar tidak dobel."); }, category: "Info toko", w: 3, h: 3, minW: 1, minH: 1, keywords: m("logo merek brand identitas") },
  { kind: "article_body", get name() { return t("Isi artikel"); }, get desc() { return t("Tulisan blog/artikel dengan judul bagian, daftar, kutipan, dan daftar isi otomatis. Bagus untuk SEO."); }, category: "Konten", w: 8, h: 16, minW: 4, minH: 4, keywords: m("artikel blog tulisan cerita seo konten") },
  { kind: "blog_list", get name() { return t("Daftar artikel"); }, get desc() { return t("Kartu artikel terbaru dari halaman artikel Anda, otomatis urut terbaru."); }, category: "Konten", w: 12, h: 10, minW: 4, minH: 4,
    layouts: [{ value: "grid", get label() { return t("Grid"); } }, { value: "list", get label() { return t("Daftar"); } }], keywords: m("blog artikel berita kabar daftar") },
  { kind: "contact", get name() { return t("Kontak"); }, get desc() { return t("Tombol WhatsApp, telepon, dan media sosial."); }, category: "Info toko", w: 4, h: 5, minW: 3, minH: 2, keywords: m("whatsapp telepon sosial") },
  { kind: "shop_cart", core: true, get name() { return t("Keranjang & checkout"); }, get desc() { return t("Daftar belanja, data pembeli, cara pengambilan, dan tombol pesan."); }, category: "Jualan", w: 12, h: 24, minW: 6, minH: 8, keywords: "" },
  { kind: "shop_orders", core: true, get name() { return t("Status pesanan"); }, get desc() { return t("Status, rincian, dan pembayaran pesanan pembeli."); }, category: "Jualan", w: 12, h: 24, minW: 6, minH: 8, keywords: "" },
  { kind: "shop_account", core: true, get name() { return t("Akun pembeli"); }, get desc() { return t("Masuk, daftar, dan riwayat pesanan pembeli."); }, category: "Jualan", w: 12, h: 24, minW: 6, minH: 8, keywords: "" },
];

/** Halaman sistem (slug = rute toko) → blok intinya. Sama dengan `umkm_domain::website::SYSTEM_PAGES`. */
export const SYSTEM_CORE: Record<string, CoreKind> = { keranjang: "shop_cart", pesanan: "shop_orders", akun: "shop_account" };
export const isCoreKind = (k: string): k is CoreKind => k === "shop_cart" || k === "shop_orders" || k === "shop_account";

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
  chatbot: { get title() { return t("Tanya kami"); }, get welcome() { return t("Halo! Ada yang bisa kami bantu?"); } },
  text: { get title() { return t("Pengumuman"); }, get body() { return t("Tulis informasi penting untuk pembeli di sini."); } },
  background: { fit: "crop", position: "center", opacity: 100 },
  image: { fit: "contain", position: "center", opacity: 100, corner: "bottom-right" },
  shape: { shape: "blob", color: "primary", opacity: 100 },
  logo: { show_name: true },
  article_body: { get body() { return t("## Pembuka\nTulis pembuka artikel di sini. Ceritakan apa yang akan dipelajari pembaca.\n\n## Langkah-langkah\n- Langkah pertama\n- Langkah kedua\n\n> Tips: gunakan judul bagian (##) agar daftar isi & Google memahami struktur artikel."); }, toc: true },
  blog_list: { get title() { return t("Artikel terbaru"); }, limit: 6 },
  shop_cart: {}, shop_orders: {}, shop_account: {},
  whatsapp: { get label() { return t("Pesan via WhatsApp"); }, get message() { return t("Halo, saya mau tanya produk di toko Anda."); } },
};

export type CellStyle = "plain" | "card" | "tint" | "dark" | "soft" | "flat";
export const CELL_STYLES: { value: CellStyle; label: string }[] = [
  { value: "plain", get label() { return t("Polos"); } },
  { value: "card", get label() { return t("Kartu"); } },
  { value: "tint", get label() { return t("Warna lembut"); } },
  { value: "dark", get label() { return t("Gelap"); } },
  { value: "soft", get label() { return t("3D lembut"); } },
  { value: "flat", get label() { return t("2D datar"); } },
];

/** Palet siap pakai: warna utama + warna teks di atas warna utama + permukaan halaman.
 *  Wajib: contrast(primary, onPrimary) ≥ 4.5 (WCAG AA) — dijaga lewat plugins.test.ts. */
export interface Palette { name: string; primary: string; onPrimary: string; surface: "light" | "cream" | "dark" }
export const PALETTES: Palette[] = [
  { get name() { return t("Senja"); },  primary: "#C2410C", onPrimary: "#FFFFFF", surface: "cream" },
  { get name() { return t("Indigo"); }, primary: "#4F46E5", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Daun"); },   primary: "#15803D", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Laut"); },   primary: "#075C9E", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Mawar"); },  primary: "#A61D6D", onPrimary: "#FFFFFF", surface: "cream" },
  { get name() { return t("Kopi"); },   primary: "#5C2A0D", onPrimary: "#FEF3C7", surface: "cream" },
  { get name() { return t("Malam"); },  primary: "#7C3AED", onPrimary: "#FFFFFF", surface: "dark" },
  { get name() { return t("Emas"); },   primary: "#CA8A04", onPrimary: "#1C1917", surface: "dark" },
  { get name() { return t("Sawi"); },   primary: "#33850C", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Terong"); }, primary: "#7C1FA0", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Bata"); },   primary: "#9F1239", onPrimary: "#FFFFFF", surface: "cream" },
  { get name() { return t("Sian"); },   primary: "#0A6F82", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Kuningan"); }, primary: "#7A7008", onPrimary: "#FFFFFF", surface: "cream" },
  { get name() { return t("Lumut"); },  primary: "#3F6212", onPrimary: "#F7FEE7", surface: "dark" },
  { get name() { return t("Abu"); },    primary: "#3F3F46", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Merah"); },  primary: "#DC2626", onPrimary: "#FFFFFF", surface: "light" },
  { get name() { return t("Toska"); },  primary: "#0F766E", onPrimary: "#FFFFFF", surface: "cream" },
  { get name() { return t("Koral"); },  primary: "#E11D48", onPrimary: "#FFFFFF", surface: "light" },
];
