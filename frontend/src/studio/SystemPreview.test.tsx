import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { Business } from "@/renderer/types";

import { SystemPreview } from "./SystemPreview";

const biz: Business = { profile: { name: "Kopi Senja", socials: [], timezone: "Asia/Jakarta", pickup_available: true, delivery_available: false }, addresses: [], hours: [], exceptions: [] };
const theme = { primary: "#0F766E", font: "inter", accent: "#F59E0B", radius: "round" as const, shop: { cart_note: "Diproses 1×24 jam", checkout_button: "Pesan sekarang", thanks: "Terima kasih, kak!" } };
const render = (kind: "cart" | "orders" | "account") => renderToStaticMarkup(
  <SystemPreview kind={kind} device="desktop" theme={theme} business={biz} pages={[{ slug: "home", title: "Beranda" }, { slug: "menu", title: "Menu" }]}
    assetUrl={() => undefined} products={[{ slug: "kopi", name: "Kopi susu", price: "18000.00", promo_price: null, available: true }]} />,
);

describe("pratinjau halaman sistem", () => {
  it("keranjang memakai bingkai situs (nav + footer + brand kit), data contoh, dan teks pemilik", () => {
    const html = render("cart");
    expect(html).toContain('class="sv2 site shop"');
    expect(html).toContain("--s-accent:#F59E0B");
    expect(html).toContain('href="/menu"');
    expect(html).toContain("sv2-foot");
    expect(html).toContain("Kopi susu");
    expect(html).toContain("Diproses 1×24 jam");
    expect(html).toContain("Pesan sekarang");
    expect(html).toContain('aria-current="step"');
  });
  it("pesanan menampilkan linimasa status & ucapan; akun menampilkan formulir masuk", () => {
    const order = render("orders");
    expect(order).toContain("shop-timeline");
    expect(order).toMatch(/data-state="now" aria-current="step"><span class="shop-timeline-dot"[^>]*><\/span><span>Diproses/);
    expect(order).toContain("Terima kasih, kak!");
    expect(render("account")).toContain("Kenapa punya akun?");
  });
});
