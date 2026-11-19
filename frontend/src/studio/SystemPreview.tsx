"use client";

// Halaman sistem (keranjang, pesanan, akun) = halaman Studio biasa dengan BLOK INTI yang terkunci.
// Di kanvas, blok inti merender komponen halaman toko yang ASLI dalam mode tertanam, dengan brand kit
// yang sedang diedit dan data contoh (tanpa API). Teksnya diubah di Inspector blok inti.

import type { PageV2 } from "./doc";
import type { CoreKind } from "./plugins";
import { t as tx } from "@/i18n";
import type { GridProduct } from "@/renderer/Site";
import type { Business } from "@/renderer/types";
import { ShopEmbedCtx, ShopSiteCtx, type ShopSite } from "@/shop/frame";
import { AccountPage, CartPage, OrderPage, ShopPreviewCtx, type ShopPreview } from "@/shop/views";
import "@/shop/shop.css";

export type SystemPage = "cart" | "orders" | "account";

export const SYSTEM_PAGES = (): { kind: SystemPage; core: CoreKind; title: string; path: string; slug: string }[] => [
  { kind: "cart", core: "shop_cart", title: tx("Keranjang & checkout"), path: "/keranjang", slug: "keranjang" },
  { kind: "orders", core: "shop_orders", title: tx("Status pesanan"), path: "/pesanan", slug: "pesanan" },
  { kind: "account", core: "shop_account", title: tx("Akun pembeli"), path: "/akun", slug: "akun" },
];

function sample(products: GridProduct[]): ShopPreview {
  const picks = products.length ? products.slice(0, 2) : [
    { slug: "contoh-1", name: tx("Produk contoh"), price: "25000.00", image: undefined },
    { slug: "contoh-2", name: tx("Produk contoh lain"), price: "40000.00", image: undefined },
  ];
  const cart = picks.map((p, i) => ({
    variant_id: `v${i}`, slug: p.slug, name: p.name, variant_name: "Default", price: p.price, quantity: i + 1, image: p.image,
  }));
  const total = cart.reduce((n, c) => n + Number(c.price) * c.quantity, 0);
  return {
    cart,
    cfg: { checkout_enabled: true, pickup_available: true, delivery_available: true, delivery_fee: "10000.00", payment_method: "qris_static", privacy_version: "v1", terms_version: "v1" },
    order: {
      code: "ORD-CONTOH", status: "PROCESSING", fulfillment: "pickup", name: tx("Pembeli"), delivery_address: null,
      items: cart.map((c) => ({ product_name: c.name, variant_name: "Default", sku: "", unit_price: c.price, quantity: c.quantity, line_total: (Number(c.price) * c.quantity).toFixed(2) })),
      subtotal: total.toFixed(2), delivery_fee: "0.00", total: total.toFixed(2), created_at: new Date().toISOString(),
      expires_at: new Date(Date.now() + 86400000).toISOString(), payment: null,
    } as unknown as ShopPreview["order"],
    orders: [],
  };
}

/** Isi blok inti di kanvas Studio: halaman toko asli (tertanam) dengan data contoh. */
export function CorePreview({ kind, theme, business, pages, assetUrl, products }: {
  kind: string; theme: PageV2["theme"]; business: Business;
  pages: { slug: string; title: string }[]; assetUrl: (r: string) => string | undefined; products: GridProduct[];
}) {
  const site: ShopSite = { theme, business, pages, assetUrl, preview: true };
  return (
    <ShopSiteCtx.Provider value={site}>
      <ShopPreviewCtx.Provider value={sample(products)}>
        <ShopEmbedCtx.Provider value={{ preview: true }}>
          {kind === "shop_cart" ? <CartPage /> : kind === "shop_orders" ? <OrderPage /> : <AccountPage />}
        </ShopEmbedCtx.Provider>
      </ShopPreviewCtx.Provider>
    </ShopSiteCtx.Provider>
  );
}

const FIELDS = (): { key: keyof NonNullable<PageV2["theme"]["shop"]>; label: string; max: number; page: SystemPage; long?: boolean; placeholder: string }[] => [
  { key: "cart_title", label: tx("Judul halaman keranjang"), max: 60, page: "cart", placeholder: tx("Keranjang") },
  { key: "cart_note", label: tx("Catatan di ringkasan"), max: 300, page: "cart", long: true, placeholder: tx("mis. Pesanan diproses dalam 1×24 jam. Gratis ongkir di atas Rp100.000.") },
  { key: "checkout_button", label: tx("Tulisan tombol pesan"), max: 40, page: "cart", placeholder: tx("Buat pesanan") },
  { key: "empty_cart", label: tx("Pesan keranjang kosong"), max: 160, page: "cart", long: true, placeholder: tx("Keranjang masih kosong. Yuk lihat produk kami!") },
  { key: "thanks", label: tx("Ucapan di halaman pesanan"), max: 300, page: "orders", long: true, placeholder: tx("mis. Terima kasih sudah belanja! Pesanan Anda kami siapkan dengan sepenuh hati.") },
  { key: "account_note", label: tx("Catatan di halaman akun"), max: 300, page: "account", long: true, placeholder: tx("mis. Member mendapat info promo lebih dulu.") },
];

/** Kolom teks blok inti (Inspector). Teks disimpan di brand kit (`theme.shop`), jadi berlaku di situs. */
export function ShopTextFields({ core, texts, onChange }: {
  core: CoreKind; texts: PageV2["theme"]["shop"]; onChange: (next: PageV2["theme"]["shop"]) => void;
}) {
  const page = SYSTEM_PAGES().find((p) => p.core === core)!;
  return (
    <>
      <p className="d-hint u-mt-0">{tx("Blok inti halaman {0} tidak bisa dihapus. Pindahkan, ubah ukurannya, dan tambahkan blok lain di sekitarnya. Pratinjau memakai data contoh.", page.path)}</p>
      {FIELDS().filter((f) => f.page === page.kind).map((f) => (
        <label key={f.key} className="d-field">
          <span>{f.label}</span>
          {f.long
            ? <textarea rows={3} maxLength={f.max} placeholder={f.placeholder} value={texts?.[f.key] ?? ""} onChange={(e) => onChange({ ...texts, [f.key]: e.target.value })} />
            : <input maxLength={f.max} placeholder={f.placeholder} value={texts?.[f.key] ?? ""} onChange={(e) => onChange({ ...texts, [f.key]: e.target.value })} />}
          <span className="d-hint">{tx("Kosongkan untuk memakai teks bawaan.")}</span>
        </label>
      ))}
    </>
  );
}
