// Template Studio (prototipe ADR-0017): titik mulai yang langsung terlihat jadi, diisi data toko
// (nama, deskripsi, produk, jam buka) — bukan kanvas kosong.

import type { Json } from "@/renderer/types";

import { compact, type Cell } from "./grid";
import { pluginDef, type CellStyle, type PluginKind } from "./plugins";

export interface TplCell extends Cell {
  kind: PluginKind;
  data: { [k: string]: Json };
  style: CellStyle;
  layout?: string;
  autoH?: boolean;
  chatOpen?: boolean;
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
    key: "hangat", name: "Hangat", desc: "Kuliner & kafe rumahan. Banner dengan chatbot di samping, menu jadi bintang.", tags: "kuliner kafe makanan minuman",
    build: (t) => ({
      primary: "#C2410C", surface: "cream", font: "poppins",
      cells: compact([
        b("hero", 0, 0, 8, 10, "plain", { title: t.name, subtitle: lead(t, "Dibuat segar setiap hari dengan bahan pilihan. Pesan online, ambil atau kami antar."), cta: cta("Lihat menu", "#menu") }),
        b("chatbot", 8, 0, 4, 10, "card", chat("Tanya kami", "Halo! Mau pesan apa hari ini? 😊")),
        b("promotion", 0, 10, 12, 5, "tint", { title: "Gratis ongkir minggu ini", body: "Untuk pesanan di atas Rp50.000 area dalam kota.", cta: cta("Pesan sekarang", "#menu") }),
        b("product_grid", 0, 15, 12, 12, "plain", { title: "Menu favorit", limit: 8 }),
        b("business_hours", 0, 27, 4, 9, "card", { title: "Jam buka" }),
        b("location", 4, 27, 4, 6, "card", { title: "Lokasi", show_map: true }),
        b("contact", 8, 27, 4, 6, "card", { title: "Pesan lewat WhatsApp", show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("testimonials", 0, 36, 12, 8, "plain", testi("Kata pelanggan", [["Rina", "Rasanya pas, selalu jadi langganan tiap pagi."], ["Budi", "Pengiriman cepat dan masih hangat."], ["Sari", "Porsinya mantap, harganya bersahabat."]])),
      ]) as TplCell[],
    }),
  },
  {
    key: "butik", name: "Butik", desc: "Fashion & aksesori. Banner gelap elegan, katalog carousel, chatbot melayang.", tags: "fashion baju aksesori butik",
    build: (t) => ({
      primary: "#BE185D", surface: "light", font: "merriweather",
      cells: compact([
        b("hero", 0, 0, 12, 10, "dark", { title: t.name, subtitle: lead(t, "Koleksi terbaru yang nyaman dipakai setiap hari."), cta: cta("Lihat koleksi", "#koleksi") }),
        b("product_grid", 0, 10, 12, 12, "plain", { title: "Koleksi terbaru", limit: 12 }, { layout: "carousel" }),
        b("about", 0, 22, 6, 8, "plain", { title: "Tentang kami", body: "Kami memilih bahan berkualitas dan menjahit dengan teliti, supaya setiap potong awet dipakai bertahun-tahun." }),
        b("gallery", 6, 22, 6, 8, "plain", { title: "Lookbook", images: [] }),
        b("testimonials", 0, 30, 12, 8, "tint", testi("Mereka sudah mencoba", [["Dewi", "Bahannya adem, jahitannya rapi."], ["Putri", "Ukurannya pas sesuai tabel."], ["Ayu", "Respon admin cepat dan ramah."]])),
        b("chatbot", 8, 38, 4, 10, "card", chat("Butuh bantuan ukuran?", "Hai! Mau dibantu pilih ukuran atau warna?"), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "profesional", name: "Profesional", desc: "Jasa & servis. Kontak menonjol di samping banner, kepercayaan lewat testimoni.", tags: "jasa servis laundry bengkel salon",
    build: (t) => ({
      primary: "#0369A1", surface: "light", font: "inter",
      cells: compact([
        b("hero", 0, 0, 7, 10, "plain", { title: t.name, subtitle: lead(t, "Layanan cepat, harga jelas, hasil bergaransi."), cta: cta("Konsultasi gratis", "#kontak") }),
        b("contact", 7, 0, 5, 10, "card", { title: "Hubungi kami", show_whatsapp: true, show_phone: true, show_email: true, socials: [] }),
        b("about", 0, 10, 12, 7, "tint", { title: "Kenapa memilih kami", body: "Tim berpengalaman, jadwal fleksibel, dan laporan pekerjaan yang transparan." }),
        b("product_grid", 0, 17, 12, 10, "plain", { title: "Layanan & harga", limit: 8 }, { layout: "list" }),
        b("testimonials", 0, 27, 12, 8, "plain", testi("Ulasan pelanggan", [["Hendra", "Pekerjaan rapi dan tepat waktu."], ["Lina", "Harga sesuai yang dijanjikan."], ["Agus", "Komunikasinya enak, recommended."]])),
        b("business_hours", 0, 35, 6, 9, "card", { title: "Jam layanan" }),
        b("location", 6, 35, 6, 6, "card", { title: "Area layanan", show_map: true }),
        b("chatbot", 8, 44, 4, 10, "card", chat("Tanya jadwal", "Halo! Mau cek jadwal kosong?"), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "galeri", name: "Galeri", desc: "Kerajinan & karya. Foto besar dulu, cerita di samping testimoni.", tags: "kerajinan handmade seni karya",
    build: (t) => ({
      primary: "#92400E", surface: "cream", font: "merriweather",
      cells: compact([
        b("hero", 0, 0, 12, 9, "plain", { title: t.name, subtitle: lead(t, "Dibuat dengan tangan, satu per satu."), cta: cta("Lihat karya", "#karya") }),
        b("gallery", 0, 9, 12, 8, "plain", { title: "Karya kami", images: [] }),
        b("product_grid", 0, 17, 12, 12, "plain", { title: "Bisa dipesan", limit: 8 }),
        b("about", 0, 29, 6, 8, "card", { title: "Cerita kami", body: "Berawal dari hobi di garasi rumah, kini karya kami dipakai pelanggan di berbagai kota." }),
        b("testimonials", 6, 29, 6, 8, "card", testi("Kata mereka", [["Maya", "Detailnya cantik sekali."], ["Rudi", "Bungkusnya rapi, aman sampai tujuan."]])),
        b("chatbot", 0, 37, 4, 10, "card", chat("Pesan khusus?", "Halo! Mau pesan desain khusus?")),
        b("contact", 4, 37, 4, 6, "plain", { title: "Kontak", show_whatsapp: true, show_phone: false, show_email: true, socials: [] }),
        b("location", 8, 37, 4, 6, "plain", { title: "Workshop", show_map: false }),
      ]) as TplCell[],
    }),
  },
  {
    key: "malam", name: "Malam", desc: "Kafe malam & hiburan. Latar gelap, menu daftar berdampingan dengan chatbot.", tags: "kafe malam bar angkringan",
    build: (t) => ({
      primary: "#A78BFA", surface: "dark", font: "poppins",
      cells: compact([
        b("hero", 0, 0, 12, 9, "tint", { title: t.name, subtitle: lead(t, "Tempat nongkrong santai sampai larut malam."), cta: cta("Lihat menu", "#menu") }),
        b("product_grid", 0, 9, 7, 14, "plain", { title: "Menu malam ini", limit: 8 }, { layout: "list" }),
        b("chatbot", 7, 9, 5, 12, "card", chat("Reservasi meja", "Hai! Mau reservasi untuk berapa orang?")),
        b("business_hours", 0, 23, 6, 9, "card", { title: "Buka" }),
        b("location", 6, 23, 6, 6, "card", { title: "Temukan kami", show_map: true }),
        b("testimonials", 0, 32, 12, 8, "plain", testi("Kesan pengunjung", [["Dimas", "Suasananya cozy, musiknya pas."], ["Nadia", "Kopinya enak buat begadang."]])),
      ]) as TplCell[],
    }),
  },
  {
    key: "segar", name: "Segar", desc: "Toko kelontong & produk harian. Promo di atas, belanja cepat, WhatsApp jelas.", tags: "kelontong sembako sayur buah toko",
    build: (t) => ({
      primary: "#15803D", surface: "light", font: "nunito",
      cells: compact([
        b("promotion", 0, 0, 12, 5, "tint", { title: "Promo hari ini", body: "Diskon 10% untuk belanja pertama lewat website.", cta: cta("Belanja sekarang", "#produk") }),
        b("hero", 0, 5, 12, 8, "plain", { title: t.name, subtitle: lead(t, "Kebutuhan harian lengkap, diantar ke rumah."), cta: cta("Mulai belanja", "#produk") }),
        b("product_grid", 0, 13, 12, 12, "plain", { title: "Paling laris", limit: 12 }),
        b("contact", 0, 25, 6, 6, "card", { title: "Pesan cepat", show_whatsapp: true, show_phone: true, show_email: false, socials: [] }),
        b("business_hours", 6, 25, 6, 9, "card", { title: "Jam buka" }),
        b("chatbot", 8, 34, 4, 10, "card", chat("Tanya stok", "Halo! Mau cek stok barang apa?"), { layout: "floating", chatOpen: false }),
      ]) as TplCell[],
    }),
  },
];
