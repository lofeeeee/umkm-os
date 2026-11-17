// Judul tab browser per halaman (satu sumber untuk metadata Next & penerjemah judul di I18nRoot).
// Hanya judul yang terdaftar di sini yang diterjemahkan — nama produk/toko di judul situs tidak.
import { m, t } from "./index";

export const TITLES = {
  home: m("Beranda"),
  orders: m("Pesanan"),
  products: m("Produk"),
  inventory: m("Stok"),
  customers: m("Pelanggan"),
  conversations: m("Percakapan"),
  tickets: m("Tiket"),
  ai: m("Asisten AI"),
  website: m("Website"),
  shopSettings: m("Pembayaran & pengiriman"),
  business: m("Profil bisnis"),
  analytics: m("Analitik"),
  audit: m("Log aktivitas"),
  team: m("Tim"),
  approvals: m("Persetujuan"),
  subscription: m("Langganan"),
  security: m("Keamanan akun"),
  platform: m("Admin platform"),
  studio: m("Studio"),
  invite: m("Terima undangan"),
  login: m("Masuk"),
  register: m("Daftarkan usaha"),
  privacy: m("Kebijakan Privasi — UMKM Business OS"),
  terms: m("Syarat & Ketentuan — UMKM Business OS"),
} as const;

const KNOWN = new Set<string>(Object.values(TITLES));

/** "Pesanan · UMKM OS" → "Orders · UMKM OS" (bagian yang bukan judul terdaftar dibiarkan). */
export function translateTitle(title: string): string {
  return title.split(" · ").map((p) => (KNOWN.has(p) ? t(p) : p)).join(" · ");
}
