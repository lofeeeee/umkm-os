// Template Studio (prototipe ADR-0017): titik mulai yang langsung terlihat jadi, diisi data toko
// (nama, deskripsi, produk, jam buka) — bukan kanvas kosong.

import type { Json } from "@/renderer/types";
import { t as tx } from "@/i18n";

import type { ArticleMeta } from "@/renderer/article";

import { compact, type Cell } from "./grid";
import { pluginDef, type CellStyle, type PluginKind } from "./plugins";

export interface TplCell extends Cell {
  kind: PluginKind;
  data: { [k: string]: Json };
  style: CellStyle;
  layout?: string;
  autoH?: boolean;
  chatOpen?: boolean;
  /** Sembunyikan blok di perangkat tertentu (G-LAY-02, didukung skema v2). */
  hideOn?: "mobile" | "desktop";
  /** Disembunyikan di semua perangkat (panel Layer Studio, gaya Photoshop). */
  hidden?: boolean;
  /** Urutan khusus ponsel (diisi saat blok disusun ulang di pratinjau ponsel). */
  mOrder?: number;
}
export interface TplResult {
  primary: string;
  surface: "light" | "cream" | "dark";
  font: string;
  cells: TplCell[];
  /** Brand kit templat (aksen, font judul, sudut, gerak, tekstur). */
  accent?: string;
  headingFont?: string;
  radius?: "sharp" | "soft" | "round";
  motion?: "none" | "subtle" | "lively";
  texture?: "none" | "grain" | "paper" | "dots";
  gradient?: boolean;
}
export interface TplInput { name: string; description: string | null }
/** Halaman tambahan paket situs (selain Beranda). */
export interface PackPage { slug: string; title: string; kind: "page" | "article" | "blog"; cells: TplCell[]; article?: ArticleMeta }
export interface Template {
  key: string; name: string; desc: string; tags: string;
  /** Kategori UMKM (label galeri). */
  category: string;
  build: (t: TplInput) => TplResult;
  /** Paket situs lengkap: halaman katalog, tentang, blog, dan satu contoh artikel. */
  pack?: (t: TplInput) => PackPage[];
}

let n = 0;
const id = (k: string) => `${k.replace(/_/g, "-")}-t${(n++).toString(36)}${Date.now().toString(36).slice(-4)}`;

/** Satu blok: kind, posisi (x, y, w, h), gaya, data. y hanya urutan kasar; compact() merapikan. */
function b(kind: PluginKind, x: number, y: number, w: number, h: number, style: CellStyle, data: { [k: string]: Json }, extra: Partial<TplCell> = {}): TplCell {
  const d = pluginDef(kind);
  const layer = kind === "background" || kind === "shape" || kind === "logo" || kind === "image";
  return { id: id(kind), kind, x, y, w, h, minW: d.minW, minH: d.minH, style, data, layout: d.layouts?.[0]?.value, autoH: !layer, chatOpen: true, ...(layer ? { layer: true } : {}), ...extra };
}

const cta = (label: string, href: string) => ({ label, href });
const chat = (title: string, welcome: string) => ({ title, welcome });
const testi = (title: string, items: [string, string][]) => ({ title, items: items.map(([name, quote]) => ({ name, quote })) });
const lead = (t: TplInput, fallback: string) => (t.description?.trim() ? t.description.trim().slice(0, 240) : fallback);

const BASE: Omit<Template, "category" | "pack">[] = [
  {
    key: "hangat", get name() { return tx("Hangat"); }, get desc() { return tx("Kuliner & kafe rumahan. Banner dengan chatbot di samping, menu jadi bintang."); }, tags: "kuliner kafe makanan minuman",
    build: (t) => ({
      primary: "#C2410C", surface: "cream", font: "poppins",
      cells: compact([
        b("hero", 0, 0, 8, 10, "plain", { title: t.name, subtitle: lead(t, tx("Dibuat segar setiap hari dengan bahan pilihan. Pesan online, ambil atau kami antar.")), cta: cta(tx("Lihat menu"), "#menu") }),
        b("chatbot", 8, 0, 4, 10, "card", chat(tx("Tanya kami"), tx("Halo! Mau pesan apa hari ini? 😊"))),
        b("promotion", 0, 10, 12, 5, "tint", { title: tx("Gratis ongkir minggu ini"), body: tx("Untuk pesanan di atas Rp50.000 area dalam kota."), cta: cta(tx("Pesan sekarang"), "#menu") }),
        b("product_grid", 0, 15, 12, 12, "plain", { title: tx("Menu favorit"), limit: 8 }),
        b("business_hours", 0, 27, 4, 9, "card", { title: tx("Jam buka") }),
        b("location", 4, 27, 4, 6, "card", { title: tx("Lokasi"), show_map: true }),
        b("contact", 8, 27, 4, 6, "card", { title: tx("Pesan lewat WhatsApp"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("testimonials", 0, 36, 12, 8, "plain", testi(tx("Kata pelanggan"), [["Rina", tx("Rasanya pas, selalu jadi langganan tiap pagi.")], ["Budi", tx("Pengiriman cepat dan masih hangat.")], ["Sari", tx("Porsinya mantap, harganya bersahabat.")]])),
      ]) as TplCell[],
    }),
  },
  {
    key: "butik", get name() { return tx("Butik"); }, get desc() { return tx("Fashion & aksesori. Banner gelap elegan, katalog carousel, chatbot melayang."); }, tags: "fashion baju aksesori butik",
    build: (t) => ({
      primary: "#BE185D", surface: "light", font: "fraunces",
      cells: compact([
        b("hero", 0, 0, 12, 10, "dark", { title: t.name, subtitle: lead(t, tx("Koleksi terbaru yang nyaman dipakai setiap hari.")), cta: cta(tx("Lihat koleksi"), "#koleksi") }),
        b("product_grid", 0, 10, 12, 12, "plain", { title: tx("Koleksi terbaru"), limit: 12 }, { layout: "carousel" }),
        b("about", 0, 22, 6, 8, "plain", { title: tx("Tentang kami"), body: tx("Kami memilih bahan berkualitas dan menjahit dengan teliti, supaya setiap potong awet dipakai bertahun-tahun.") }),
        b("gallery", 6, 22, 6, 8, "plain", { title: tx("Lookbook"), images: [] }),
        b("testimonials", 0, 30, 12, 8, "tint", testi(tx("Mereka sudah mencoba"), [["Dewi", tx("Bahannya adem, jahitannya rapi.")], ["Putri", tx("Ukurannya pas sesuai tabel.")], ["Ayu", tx("Respon admin cepat dan ramah.")]])),
        b("chatbot", 8, 38, 4, 10, "card", chat(tx("Butuh bantuan ukuran?"), tx("Hai! Mau dibantu pilih ukuran atau warna?")), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "profesional", get name() { return tx("Profesional"); }, get desc() { return tx("Jasa & servis. Kontak menonjol di samping banner, kepercayaan lewat testimoni."); }, tags: "jasa servis laundry bengkel salon",
    build: (t) => ({
      primary: "#0369A1", surface: "light", font: "jakarta",
      cells: compact([
        b("hero", 0, 0, 7, 10, "plain", { title: t.name, subtitle: lead(t, tx("Layanan cepat, harga jelas, hasil bergaransi.")), cta: cta(tx("Konsultasi gratis"), "#kontak") }),
        b("contact", 7, 0, 5, 10, "card", { title: tx("Hubungi kami"), show_whatsapp: true, show_phone: true, show_email: true, socials: [] }),
        b("about", 0, 10, 12, 7, "tint", { title: tx("Kenapa memilih kami"), body: tx("Tim berpengalaman, jadwal fleksibel, dan laporan pekerjaan yang transparan.") }),
        b("product_grid", 0, 17, 12, 10, "plain", { title: tx("Layanan & harga"), limit: 8 }, { layout: "list" }),
        b("testimonials", 0, 27, 12, 8, "plain", testi(tx("Ulasan pelanggan"), [["Hendra", tx("Pekerjaan rapi dan tepat waktu.")], ["Lina", tx("Harga sesuai yang dijanjikan.")], ["Agus", tx("Komunikasinya enak, recommended.")]])),
        b("business_hours", 0, 35, 6, 9, "card", { title: tx("Jam layanan") }),
        b("location", 6, 35, 6, 6, "card", { title: tx("Area layanan"), show_map: true }),
        b("chatbot", 8, 44, 4, 10, "card", chat(tx("Tanya jadwal"), tx("Halo! Mau cek jadwal kosong?")), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "galeri", get name() { return tx("Galeri"); }, get desc() { return tx("Kerajinan & karya. Foto besar dulu, cerita di samping testimoni."); }, tags: "kerajinan handmade seni karya",
    build: (t) => ({
      primary: "#92400E", surface: "cream", font: "merriweather",
      cells: compact([
        b("hero", 0, 0, 12, 9, "plain", { title: t.name, subtitle: lead(t, tx("Dibuat dengan tangan, satu per satu.")), cta: cta(tx("Lihat karya"), "#karya") }),
        b("gallery", 0, 9, 12, 8, "plain", { title: tx("Karya kami"), images: [] }),
        b("product_grid", 0, 17, 12, 12, "plain", { title: tx("Bisa dipesan"), limit: 8 }),
        b("about", 0, 29, 6, 8, "card", { title: tx("Cerita kami"), body: tx("Berawal dari hobi di garasi rumah, kini karya kami dipakai pelanggan di berbagai kota.") }),
        b("testimonials", 6, 29, 6, 8, "card", testi(tx("Kata mereka"), [["Maya", tx("Detailnya cantik sekali.")], ["Rudi", tx("Bungkusnya rapi, aman sampai tujuan.")]])),
        b("chatbot", 0, 37, 4, 10, "card", chat(tx("Pesan khusus?"), tx("Halo! Mau pesan desain khusus?"))),
        b("contact", 4, 37, 4, 6, "plain", { title: tx("Kontak"), show_whatsapp: true, show_phone: false, show_email: true, socials: [] }),
        b("location", 8, 37, 4, 6, "plain", { title: tx("Workshop"), show_map: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "malam", get name() { return tx("Malam"); }, get desc() { return tx("Kafe malam & hiburan. Latar gelap, menu daftar berdampingan dengan chatbot."); }, tags: "kafe malam bar angkringan",
    build: (t) => ({
      primary: "#A78BFA", surface: "dark", font: "poppins",
      cells: compact([
        b("hero", 0, 0, 12, 9, "tint", { title: t.name, subtitle: lead(t, tx("Tempat nongkrong santai sampai larut malam.")), cta: cta(tx("Lihat menu"), "#menu") }),
        b("product_grid", 0, 9, 7, 14, "plain", { title: tx("Menu malam ini"), limit: 8 }, { layout: "list" }),
        b("chatbot", 7, 9, 5, 12, "card", chat(tx("Reservasi meja"), tx("Hai! Mau reservasi untuk berapa orang?"))),
        b("business_hours", 0, 23, 6, 9, "card", { title: tx("Buka") }),
        b("location", 6, 23, 6, 6, "card", { title: tx("Temukan kami"), show_map: true }),
        b("testimonials", 0, 32, 12, 8, "plain", testi(tx("Kesan pengunjung"), [["Dimas", tx("Suasananya cozy, musiknya pas.")], ["Nadia", tx("Kopinya enak buat begadang.")]])),
      ]) as TplCell[],
    }),
  },
  {
    key: "segar", get name() { return tx("Segar"); }, get desc() { return tx("Toko kelontong & produk harian. Promo di atas, belanja cepat, WhatsApp jelas."); }, tags: "kelontong sembako sayur buah toko",
    build: (t) => ({
      primary: "#15803D", surface: "light", font: "nunito",
      cells: compact([
        b("promotion", 0, 0, 12, 5, "tint", { title: tx("Promo hari ini"), body: tx("Diskon 10% untuk belanja pertama lewat website."), cta: cta(tx("Belanja sekarang"), "#produk") }),
        b("hero", 0, 5, 12, 8, "plain", { title: t.name, subtitle: lead(t, tx("Kebutuhan harian lengkap, diantar ke rumah.")), cta: cta(tx("Mulai belanja"), "#produk") }),
        b("product_grid", 0, 13, 12, 12, "plain", { title: tx("Paling laris"), limit: 12 }),
        b("contact", 0, 25, 6, 6, "card", { title: tx("Pesan cepat"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("business_hours", 6, 25, 6, 9, "card", { title: tx("Jam buka") }),
        b("chatbot", 8, 34, 4, 10, "card", chat(tx("Tanya stok"), tx("Halo! Mau cek stok barang apa?")), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "ayu", get name() { return tx("Ayu"); }, get desc() { return tx("Kecantikan & salon. Lembut, elegan, jadwal & booking lewat chat."); }, tags: "salon kecantikan spa skincare barbershop nail",
    build: (t) => ({
      primary: "#9D174D", surface: "cream", font: "nunito",
      cells: compact([
        b("hero", 0, 0, 7, 10, "plain", { title: t.name, subtitle: lead(t, tx("Perawatan yang membuat Anda tampil percaya diri, dengan produk aman dan terapis berpengalaman.")), cta: cta(tx("Booking sekarang"), "#kontak") }),
        b("chatbot", 7, 0, 5, 10, "card", chat(tx("Booking jadwal"), tx("Halo! Mau perawatan apa dan kapan?"))),
        b("product_grid", 0, 10, 12, 11, "plain", { title: tx("Perawatan favorit"), limit: 8 }),
        b("promotion", 0, 21, 12, 5, "tint", { title: tx("Member baru diskon 15%"), body: tx("Berlaku untuk perawatan pertama, cukup tunjukkan halaman ini."), cta: cta(tx("Klaim diskon"), "#kontak") }),
        b("testimonials", 0, 26, 12, 8, "plain", testi(tx("Cerita pelanggan"), [["Anisa", tx("Hasilnya natural, terapisnya telaten.")], ["Fitri", tx("Tempatnya bersih dan wangi.")], ["Lusi", tx("Booking gampang, tidak perlu antre.")]])),
        b("business_hours", 0, 34, 4, 9, "card", { title: tx("Jam buka") }),
        b("location", 4, 34, 4, 6, "card", { title: tx("Lokasi"), show_map: true }),
        b("contact", 8, 34, 4, 6, "card", { title: tx("Hubungi kami"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
      ]) as TplCell[],
    }),
  },
  {
    key: "cerdas", get name() { return tx("Cerdas"); }, get desc() { return tx("Kursus & edukasi. Program jelas, testimoni alumni, daftar lewat WhatsApp."); }, tags: "kursus les bimbel edukasi pelatihan kelas",
    build: (t) => ({
      primary: "#1D4ED8", surface: "light", font: "jakarta",
      cells: compact([
        b("hero", 0, 0, 12, 9, "plain", { title: t.name, subtitle: lead(t, tx("Belajar dengan pengajar berpengalaman, kelas kecil, dan jadwal fleksibel.")), cta: cta(tx("Lihat program"), "#produk") }),
        b("about", 0, 9, 12, 7, "tint", { title: tx("Kenapa belajar di sini"), body: tx("Materi praktis, kelas maksimal 10 orang, dan laporan perkembangan setiap bulan.") }),
        b("product_grid", 0, 16, 12, 11, "plain", { title: tx("Program & kelas"), limit: 8 }, { layout: "list" }),
        b("testimonials", 0, 27, 12, 8, "plain", testi(tx("Kata alumni"), [["Raka", tx("Nilai saya naik jauh dalam 3 bulan.")], ["Salsa", tx("Pengajarnya sabar dan jelas.")], ["Bima", tx("Jadwalnya bisa menyesuaikan sekolah.")]])),
        b("contact", 0, 35, 6, 6, "card", { title: tx("Daftar sekarang"), show_whatsapp: true, show_phone: true, show_email: true, socials: [] }),
        b("location", 6, 35, 6, 6, "card", { title: tx("Tempat belajar"), show_map: true }),
        b("chatbot", 8, 41, 4, 10, "card", chat(tx("Tanya program"), tx("Halo! Mau tanya program untuk usia berapa?")), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "hajatan", get name() { return tx("Hajatan"); }, get desc() { return tx("Katering & kue pesanan. Paket menu, promo acara, pesan jauh hari."); }, tags: "katering catering kue tart nasi box hampers acara",
    build: (t) => ({
      primary: "#B45309", surface: "cream", font: "nunito",
      cells: compact([
        b("hero", 0, 0, 12, 10, "plain", { title: t.name, subtitle: lead(t, tx("Hidangan lezat untuk setiap acara — dari nasi kotak kantor sampai kue pernikahan.")), cta: cta(tx("Lihat paket"), "#menu") }),
        b("promotion", 0, 10, 6, 6, "tint", { title: tx("Pesan H-3 hemat 10%"), body: tx("Pesan minimal 3 hari sebelum acara untuk harga spesial."), cta: cta(tx("Pesan sekarang"), "#menu") }),
        b("about", 6, 10, 6, 6, "card", { title: tx("Dimasak di hari acara"), body: tx("Semua menu dimasak segar di hari yang sama, dikemas higienis, dan diantar tepat waktu.") }),
        b("product_grid", 0, 16, 12, 12, "plain", { title: tx("Paket & menu"), limit: 12 }),
        b("gallery", 0, 28, 12, 8, "plain", { title: tx("Dokumentasi acara"), images: [] }),
        b("testimonials", 0, 36, 12, 8, "plain", testi(tx("Kata pelanggan"), [["Bu Ratna", tx("Tamu-tamu memuji rasanya.")], ["Pak Joko", tx("Datang tepat waktu, porsinya pas.")]])),
        b("contact", 0, 44, 6, 6, "card", { title: tx("Konsultasi menu"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("business_hours", 6, 44, 6, 9, "card", { title: tx("Jam pemesanan") }),
      ]) as TplCell[],
    }),
  },
  {
    key: "tani", get name() { return tx("Tani"); }, get desc() { return tx("Hasil tani & oleh-oleh lokal. Cerita asal produk, segar dari kebun."); }, tags: "pertanian kebun madu kopi oleh-oleh lokal organik",
    build: (t) => ({
      primary: "#3F6212", surface: "cream", font: "merriweather",
      cells: compact([
        b("hero", 0, 0, 12, 10, "plain", { title: t.name, subtitle: lead(t, tx("Langsung dari kebun kami ke meja Anda — segar, jujur, dan mendukung petani lokal.")), cta: cta(tx("Belanja hasil kebun"), "#produk") }),
        b("product_grid", 0, 10, 12, 12, "plain", { title: tx("Panen minggu ini"), limit: 8 }),
        b("about", 0, 22, 7, 8, "card", { title: tx("Dari kebun kami"), body: tx("Kami merawat tanaman tanpa bahan kimia berlebihan dan memanen saat matang sempurna.") }),
        b("gallery", 7, 22, 5, 8, "plain", { title: tx("Suasana kebun"), images: [] }),
        b("testimonials", 0, 30, 12, 8, "plain", testi(tx("Kata pembeli"), [["Wulan", tx("Sayurnya masih renyah saat sampai.")], ["Doni", tx("Madunya asli, beda rasanya.")]])),
        b("contact", 0, 38, 6, 6, "card", { title: tx("Pesan langsung"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("location", 6, 38, 6, 6, "card", { title: tx("Kebun & toko"), show_map: true }),
      ]) as TplCell[],
    }),
  },
];

/* ------------------------------------------------------------------ paket situs per kategori */

type Family = "food" | "product" | "service";
interface Style { category: () => string; family: Family; band: string; edge: string; brand: Partial<TplResult>; catalog: { slug: string; title: () => string; noun: () => string }; story: () => string }

/** Kategori, brand kit, dekorasi, dan halaman katalog tiap templat. */
const STYLE: Record<string, Style> = {
  hangat: { category: () => tx("Kuliner & kafe"), family: "food", band: "#FFF1E6", edge: "wave", brand: { accent: "#B45309", headingFont: "fraunces", radius: "round", motion: "subtle", texture: "paper" },
    catalog: { slug: "menu", title: () => tx("Menu"), noun: () => tx("menu") }, story: () => tx("Berawal dari dapur rumah, kami memasak dengan resep keluarga dan bahan segar pilihan setiap pagi.") },
  butik: { category: () => tx("Fashion & aksesori"), family: "product", band: "#FDF2F8", edge: "curve", brand: { accent: "#9D174D", headingFont: "playfair", radius: "sharp", motion: "subtle" },
    catalog: { slug: "koleksi", title: () => tx("Koleksi"), noun: () => tx("koleksi") }, story: () => tx("Kami merancang pakaian yang nyaman dipakai setiap hari, dengan bahan pilihan dan jahitan yang teliti.") },
  profesional: { category: () => tx("Jasa & servis"), family: "service", band: "#EFF6FF", edge: "slant", brand: { accent: "#0E7490", headingFont: "grotesk", radius: "soft", motion: "subtle" },
    catalog: { slug: "layanan", title: () => tx("Layanan"), noun: () => tx("layanan") }, story: () => tx("Tim kami terlatih, bekerja dengan standar yang jelas, dan selalu memberi laporan sebelum pekerjaan selesai.") },
  galeri: { category: () => tx("Kerajinan & seni"), family: "product", band: "#FBF3E4", edge: "wave", brand: { accent: "#92400E", headingFont: "dmserif", radius: "soft", motion: "subtle", texture: "paper" },
    catalog: { slug: "karya", title: () => tx("Karya"), noun: () => tx("karya") }, story: () => tx("Setiap karya dibuat dengan tangan, satu per satu, sehingga tidak ada dua yang benar-benar sama.") },
  malam: { category: () => tx("Kafe malam & hiburan"), family: "food", band: "#1E1B2E", edge: "zigzag", brand: { accent: "#F472B6", headingFont: "grotesk", radius: "round", motion: "lively", texture: "grain", gradient: true },
    catalog: { slug: "menu", title: () => tx("Menu"), noun: () => tx("menu") }, story: () => tx("Kami membuka tempat ini untuk teman-teman yang butuh ruang santai, kopi enak, dan musik yang pas sampai larut.") },
  segar: { category: () => tx("Kelontong & kebutuhan harian"), family: "product", band: "#ECFDF5", edge: "wave", brand: { accent: "#CA8A04", headingFont: "poppins", radius: "round", motion: "lively" },
    catalog: { slug: "produk", title: () => tx("Produk"), noun: () => tx("produk") }, story: () => tx("Kami melayani kebutuhan harian tetangga sekitar dengan harga jujur dan antar cepat ke rumah.") },
  ayu: { category: () => tx("Kecantikan & salon"), family: "service", band: "#FCE7F3", edge: "curve", brand: { accent: "#DB2777", headingFont: "playfair", radius: "round", motion: "subtle" },
    catalog: { slug: "perawatan", title: () => tx("Perawatan"), noun: () => tx("perawatan") }, story: () => tx("Kami percaya setiap orang berhak merasa cantik — dengan perawatan aman, ruang yang bersih, dan pelayanan yang tulus.") },
  cerdas: { category: () => tx("Kursus & edukasi"), family: "service", band: "#EEF2FF", edge: "slant", brand: { accent: "#F59E0B", headingFont: "grotesk", radius: "round", motion: "subtle", gradient: true },
    catalog: { slug: "program", title: () => tx("Program"), noun: () => tx("program") }, story: () => tx("Kami mulai dari kelas kecil di ruang tamu, dan kini telah membantu ratusan murid meraih target belajarnya.") },
  hajatan: { category: () => tx("Katering & kue"), family: "food", band: "#FEF3C7", edge: "wave", brand: { accent: "#15803D", headingFont: "fraunces", radius: "round", motion: "subtle", texture: "paper" },
    catalog: { slug: "paket", title: () => tx("Paket"), noun: () => tx("paket") }, story: () => tx("Dari arisan keluarga sampai pesta pernikahan, kami menyiapkan hidangan dengan rasa rumahan yang selalu dirindukan.") },
  tani: { category: () => tx("Hasil tani & oleh-oleh"), family: "product", band: "#F7FEE7", edge: "wave", brand: { accent: "#CA8A04", headingFont: "dmserif", radius: "soft", motion: "subtle", texture: "paper" },
    catalog: { slug: "produk", title: () => tx("Produk"), noun: () => tx("hasil kebun") }, story: () => tx("Kebun kami dikelola keluarga sejak tiga generasi, dan kini hasilnya bisa Anda nikmati langsung tanpa perantara.") },
};

/** Beranda + brand kit + pita latar selebar layar di belakang banner. */
function decorate(key: string, r: TplResult): TplResult {
  const st = STYLE[key];
  if (!st) return r;
  const hero = r.cells.find((c) => c.kind === "hero");
  const band = hero
    ? [b("background", 0, hero.y, 12, hero.h, "plain", { color: st.band, bleed: true, edge_bottom: st.edge })]
    : [];
  return { ...st.brand, ...r, cells: [...band, ...r.cells] };
}

const ARTICLE: Record<Family, { slug: string; title: () => string; excerpt: () => string; tags: () => string[]; body: () => string }> = {
  food: {
    slug: "tips-pesanan-tetap-enak", title: () => tx("Tips agar pesanan tetap enak sampai rumah"),
    excerpt: () => tx("Cara menyimpan dan menghangatkan makanan pesanan supaya rasanya tetap seperti baru dimasak."),
    tags: () => [tx("tips"), tx("kuliner")],
    body: () => tx("## Simpan dengan benar\nBegitu pesanan sampai, pisahkan makanan berkuah dan yang kering. Simpan di wadah tertutup bila tidak langsung disantap.\n\n## Menghangatkan tanpa merusak rasa\n- Makanan berkuah: panaskan di panci dengan api kecil.\n- Gorengan: gunakan oven atau wajan tanpa minyak agar tetap renyah.\n- Hindari memanaskan berulang kali.\n\n## Berapa lama aman disimpan?\n1. Suhu ruang: maksimal 4 jam.\n2. Kulkas: 1–2 hari dalam wadah tertutup.\n\n> Ada pertanyaan soal menu? Tanyakan lewat chat — kami senang membantu."),
  },
  product: {
    slug: "cara-memilih-produk", title: () => tx("Cara memilih produk yang tepat untuk Anda"),
    excerpt: () => tx("Panduan singkat agar tidak salah pilih: kenali kebutuhan, cek detail, dan manfaatkan layanan tanya jawab."),
    tags: () => [tx("panduan"), tx("belanja")],
    body: () => tx("## Kenali kebutuhan Anda\nSebelum membeli, tentukan untuk apa produk akan dipakai dan berapa anggaran yang nyaman.\n\n## Perhatikan detailnya\n- Baca deskripsi dan ukuran dengan teliti.\n- Lihat foto dari beberapa sudut.\n- Cek ulasan pembeli lain.\n\n## Masih ragu?\nTanyakan langsung kepada kami lewat chat atau WhatsApp. Kami akan membantu memilih yang paling cocok.\n\n> Belanja yang tepat itu hemat — tidak perlu tukar atau beli ulang."),
  },
  service: {
    slug: "persiapan-sebelum-datang", title: () => tx("Persiapan sebelum memakai layanan kami"),
    excerpt: () => tx("Hal-hal sederhana yang membuat layanan lebih cepat, nyaman, dan hasilnya sesuai harapan."),
    tags: () => [tx("panduan"), tx("layanan")],
    body: () => tx("## Buat janji lebih dulu\nPesan jadwal lewat chat atau WhatsApp agar Anda tidak perlu menunggu lama.\n\n## Siapkan informasi penting\n- Ceritakan kebutuhan atau keluhan Anda dengan jelas.\n- Bawa foto atau contoh hasil yang diinginkan bila ada.\n\n## Setelah layanan\n1. Periksa hasilnya bersama kami.\n2. Simpan bukti pembayaran dan jadwal kunjungan berikutnya.\n\n> Kepuasan Anda adalah prioritas kami — sampaikan saran kapan saja."),
  },
};

/** Halaman tambahan paket (Beranda + 4 = batas paket Starter). */
function pack(key: string, t: TplInput): PackPage[] {
  const st = STYLE[key];
  if (!st) return [];
  const art = ARTICLE[st.family];
  const today = new Date().toISOString().slice(0, 10);
  const noun = st.catalog.noun();
  return [
    { slug: st.catalog.slug, title: st.catalog.title(), kind: "page", cells: compact([
      b("background", 0, 0, 12, 7, "plain", { color: st.band, bleed: true, edge_bottom: st.edge }),
      b("text", 0, 0, 12, 6, "plain", { title: st.catalog.title(), body: tx("Semua {0} kami ada di sini — pilih, pesan online, lalu ambil atau kami antar.", noun) }),
      b("product_grid", 0, 6, 12, 14, "plain", { title: tx("Semua {0}", noun), limit: 24 }),
      b("promotion", 0, 20, 7, 6, "tint", { title: tx("Bingung memilih?"), body: tx("Tanyakan rekomendasi kami lewat chat atau WhatsApp."), cta: cta(tx("Tanya sekarang"), "#kontak") }),
      b("contact", 7, 20, 5, 6, "card", { title: tx("Pesan cepat"), show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
    ]) as TplCell[] },
    { slug: "tentang", title: tx("Tentang kami"), kind: "page", cells: compact([
      b("about", 0, 0, 12, 9, "plain", { title: tx("Cerita {0}", t.name), body: lead(t, st.story()) }),
      b("shape", 10, 0, 2, 3, "plain", { shape: "sparkle", color: "accent", opacity: 80, rotate: 12 }, { layout: "below" }),
      b("gallery", 0, 9, 12, 8, "plain", { title: tx("Di balik layar"), images: [] }),
      b("business_hours", 0, 17, 4, 9, "card", { title: tx("Jam buka") }),
      b("location", 4, 17, 4, 6, "card", { title: tx("Lokasi"), show_map: true }),
      b("contact", 8, 17, 4, 6, "card", { title: tx("Kontak"), show_whatsapp: true, show_phone: true, show_email: true, socials: [] }),
    ]) as TplCell[] },
    { slug: "blog", title: tx("Blog"), kind: "blog", cells: compact([
      b("text", 0, 0, 12, 5, "plain", { title: tx("Blog & tips"), body: tx("Kabar terbaru, cerita, dan tips dari {0}.", t.name) }),
      b("blog_list", 0, 5, 12, 12, "plain", { title: tx("Artikel terbaru"), limit: 9 }),
    ]) as TplCell[] },
    { slug: art.slug, title: art.title(), kind: "article", article: { excerpt: art.excerpt(), author: t.name.slice(0, 80), published_at: today, tags: art.tags() }, cells: compact([
      b("article_body", 2, 0, 8, 24, "plain", { body: art.body(), toc: true }),
      b("blog_list", 0, 24, 12, 10, "plain", { title: tx("Artikel lainnya"), limit: 3 }),
    ]) as TplCell[] },
  ];
}

export const TEMPLATES: Template[] = BASE.map((t) => ({
  // Getter (bukan spread): nama & deskripsi mengikuti bahasa antarmuka saat dibaca.
  key: t.key, tags: t.tags,
  get name() { return t.name; },
  get desc() { return t.desc; },
  get category() { return STYLE[t.key]?.category() ?? ""; },
  build: (i: TplInput) => decorate(t.key, t.build(i)),
  pack: (i: TplInput) => pack(t.key, i),
}));
