import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { StudioSite } from "@/renderer/StudioSite";
import type { Business } from "@/renderer/types";

import type { PageV2 } from "./doc";
import { CorePreview } from "./SystemPreview";

const biz: Business = { profile: { name: "Kopi Senja", socials: [], timezone: "Asia/Jakarta", pickup_available: true, delivery_available: false }, addresses: [], hours: [], exceptions: [] };
const theme = { primary: "#0F766E", font: "inter", accent: "#F59E0B", radius: "round" as const, shop: { cart_note: "Diproses 1×24 jam", checkout_button: "Pesan sekarang", thanks: "Terima kasih, kak!" } };
const pages = [{ slug: "home", title: "Beranda" }, { slug: "menu", title: "Menu" }];
const products = [{ slug: "kopi", name: "Kopi susu", price: "18000.00", promo_price: null, available: true }];

/** Halaman sistem = dokumen Studio: blok bebas (Teks) + blok inti, dirender seperti di kanvas Studio. */
const render = (core: "shop_cart" | "shop_orders" | "shop_account") => {
  const doc = {
    schema_version: 2, template: "modern_store", theme,
    blocks: [
      { id: "t1", kind: "text", x: 0, y: 0, w: 12, h: 4, style: "plain", data: { body: "Gratis ongkir minggu ini!" } },
      { id: "inti", kind: core, x: 0, y: 4, w: 12, h: 24, style: "plain", data: {} },
    ],
  } as unknown as PageV2;
  return renderToStaticMarkup(
    <StudioSite doc={doc} business={biz} assetUrl={() => undefined} pages={pages} products={products}
      coreBlock={(k) => <CorePreview kind={k} theme={theme} business={biz} pages={pages} assetUrl={() => undefined} products={products} />} />,
  );
};

describe("halaman sistem sebagai dokumen Studio", () => {
  it("keranjang: blok bebas + blok inti tertanam (tanpa navigasi/footer ganda), data contoh & teks pemilik", () => {
    const html = render("shop_cart");
    expect(html).toContain("Gratis ongkir minggu ini!");
    expect(html).toContain('data-core="shop_cart"');
    expect(html).toContain("shop shop-embed");
    expect(html.match(/class="s-headbar/g)).toHaveLength(1);
    expect(html).toContain('href="/menu"');
    expect(html).toContain("Kopi susu");
    expect(html).toContain("Diproses 1×24 jam");
    expect(html).toContain("Pesan sekarang");
    expect(html).toContain('aria-current="step"');
  });
  it("pesanan menampilkan linimasa status & ucapan; akun menampilkan formulir masuk", () => {
    const order = render("shop_orders");
    expect(order).toContain("shop-timeline");
    expect(order).toMatch(/data-state="now" aria-current="step"><span class="shop-timeline-dot"[^>]*><\/span><span>Diproses/);
    expect(order).toContain("Terima kasih, kak!");
    expect(render("shop_account")).toContain("Kenapa punya akun?");
  });
});
