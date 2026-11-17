// Pemeriksa "siap terbit" (P9-10, G-PUB-05): hal yang sering terlewat pemilik toko sebelum website dilihat
// pembeli. Murni (tanpa I/O) dan TIDAK memblokir terbit — hanya menunjukkan apa yang sebaiknya dilengkapi,
// dengan tautan ke blok (id) atau halaman pengaturan (area).

import type { Business, Json } from "@/renderer/types";

import type { PluginKind } from "./plugins";
import { t } from "@/i18n";

export interface ReadyCell { id: string; kind: PluginKind; data: { [k: string]: Json } }
export interface ReadyInput {
  cells: ReadyCell[];
  business: Business;
  /** Produk aktif yang akan tampil di galeri produk. */
  productCount: number;
  seo?: { title?: string; description?: string };
  /** Chatbot aktif di menu Asisten AI? null/undefined = belum diketahui (jangan memperingatkan). */
  chatReady?: boolean | null;
}
export interface Issue {
  key: string;
  message: string;
  /** Blok yang perlu diperbaiki (dipilih saat butir diklik). */
  cellId?: string;
  /** Data di luar Studio. */
  area?: "business" | "products" | "seo" | "ai";
}

const str = (v: Json | undefined) => (typeof v === "string" ? v.trim() : "");
const PHOTO_KINDS: PluginKind[] = ["hero", "about", "promotion"];

export function readiness({ cells, business, productCount, seo, chatReady }: ReadyInput): Issue[] {
  const out: Issue[] = [];
  const p = business.profile;
  const first = (k: PluginKind) => cells.find((c) => c.kind === k);

  const hero = first("hero");
  if (!hero || !str(hero.data.title)) {
    out.push({ key: "title", message: t("Belum ada judul utama. Tambahkan Banner berisi nama atau slogan toko."), cellId: hero?.id });
  }
  for (const c of cells) {
    if (PHOTO_KINDS.includes(c.kind) && str(c.data.image) && !str(c.data.image_alt)) {
      out.push({ key: `alt:${c.id}`, message: t("Foto belum diberi teks alternatif (dibacakan untuk pembeli tunanetra & dibaca Google)."), cellId: c.id });
    }
  }
  const wa = cells.find((c) => c.kind === "whatsapp" || (c.kind === "contact" && c.data.show_whatsapp === true));
  if (wa && !str(p.whatsapp ?? "") && !str(p.phone ?? "")) {
    out.push({ key: "whatsapp", message: t("Tombol WhatsApp belum berfungsi: nomor WhatsApp/telepon di Profil bisnis kosong."), cellId: wa.id, area: "business" });
  }
  const grid = first("product_grid");
  if (grid && productCount === 0) {
    out.push({ key: "products", message: t("Galeri produk masih kosong: belum ada produk aktif."), cellId: grid.id, area: "products" });
  }
  const loc = first("location");
  if (loc && business.addresses.length === 0) {
    out.push({ key: "address", message: t("Blok Lokasi belum menampilkan apa pun: alamat toko belum diisi."), cellId: loc.id, area: "business" });
  }
  const hours = first("business_hours");
  if (hours && business.hours.length === 0) {
    out.push({ key: "hours", message: t("Blok Jam buka kosong: jadwal buka belum diatur."), cellId: hours.id, area: "business" });
  }
  if (!str(seo?.description) && !str(p.description ?? "")) {
    out.push({ key: "description", message: t("Belum ada deskripsi toko untuk Google & pratinjau tautan WhatsApp."), area: "seo" });
  }
  // Plugin Chatbot yang dipasang tidak tampil di publik bila chatbot belum diaktifkan di Asisten AI.
  const chat = first("chatbot");
  if (chat && chatReady === false) {
    out.push({
      key: "chatbot",
      message: t("Blok Chatbot ada di halaman, tetapi chatbot belum aktif — blok ini tidak akan tampil di website. Aktifkan di Asisten AI (mode “Hanya staf” tidak butuh penyedia AI)."),
      cellId: chat.id,
      area: "ai",
    });
  }
  return out;
}
