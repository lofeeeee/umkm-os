// Template Studio (prototipe ADR-0017): titik mulai yang langsung terlihat jadi, diisi data toko
// (nama, deskripsi, produk, jam buka) — bukan kanvas kosong.

import type { Json } from "@/renderer/types";
import { t as tx } from "@/i18n";

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
  /** Urutan khusus ponsel (diisi saat blok disusun ulang di pratinjau ponsel). */
  mOrder?: number;
}
export interface TplResult {
  primary: string;
  surface: "light" | "cream" | "dark";
  font: string;
  cells: TplCell[];
}
export interface TplInput { name: string; description: string | null }
export interface Template { key: string; name: string; desc: string; tags: string; build: (t: TplInput) => TplResult }

let n = 0;
const id = (k: string) => `${k.replace(/_/g, "-")}-t${(n++).toString(36)}${Date.now().toString(36).slice(-4)}`;

/** Satu blok: kind, posisi (x, y, w, h), gaya, data. y hanya urutan kasar; compact() merapikan. */
function b(kind: PluginKind, x: number, y: number, w: number, h: number, style: CellStyle, data: { [k: string]: Json }, extra: Partial<TplCell> = {}): TplCell {
  const d = pluginDef(kind);
  return { id: id(kind), kind, x, y, w, h, minW: d.minW, minH: d.minH, style, data, layout: d.layouts?.[0]?.value, autoH: true, chatOpen: true, ...extra };
}

const cta = (label: string, href: string) => ({ label, href });
const chat = (title: string, welcome: string) => ({ title, welcome });
const testi = (title: string, items: [string, string][]) => ({ title, items: items.map(([name, quote]) => ({ name, quote })) });
const lead = (t: TplInput, fallback: string) => (t.description?.trim() ? t.description.trim().slice(0, 240) : fallback);

export const TEMPLATES: Template[] = [
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
];
