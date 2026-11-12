import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";

import { Site } from "@/renderer/Site";
import type { Business, PageContent } from "@/renderer/types";

import { rupiah } from "./api";
import { addToCart, cartCount, clearCart, displaySubtotalSen, getCart, saveOrder, savedOrders, setQuantity } from "./cart";
import { route } from "./route";

// Lingkungan node: localStorage + event minimal untuk modul cart.
const store = new Map<string, string>();
Object.assign(globalThis, {
  window: {
    localStorage: {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    },
    dispatchEvent: () => true,
  },
});

describe("routing situs toko", () => {
  it("path toko dan halaman website", () => {
    expect(route("/")).toEqual({ kind: "page", slug: "home" });
    expect(route("/promo")).toEqual({ kind: "page", slug: "promo" });
    expect(route("/produk/kopi-susu")).toEqual({ kind: "product", slug: "kopi-susu" });
    expect(route("/produk/kopi-susu/")).toEqual({ kind: "product", slug: "kopi-susu" });
    expect(route("/keranjang").kind).toBe("cart");
    expect(route("/pesanan").kind).toBe("orders");
    expect(route("/akun/verifikasi").kind).toBe("verify");
    expect(route("/produk/../etc").kind).toBe("missing");
    expect(route("/produk/A").kind).toBe("missing");
  });
});

describe("format rupiah", () => {
  it("ribuan bertitik, sen bila ada", () => {
    expect(rupiah("15000.00")).toBe("Rp15.000");
    expect(rupiah("1234567.50")).toBe("Rp1.234.567,50");
    expect(rupiah("0.00")).toBe("Rp0");
  });
});

describe("keranjang", () => {
  beforeEach(() => store.clear());
  const item = { variant_id: "v1", slug: "kopi", name: "Kopi", variant_name: "Default", price: "15000.00" };

  it("menggabungkan varian sama, batas 999, hapus saat 0", () => {
    addToCart(item, 2);
    addToCart(item, 3);
    expect(getCart()).toHaveLength(1);
    expect(cartCount()).toBe(5);
    addToCart(item, 5000);
    expect(getCart()[0]!.quantity).toBe(999);
    setQuantity("v1", 0);
    expect(getCart()).toEqual([]);
  });

  it("subtotal tampilan dalam sen tanpa floating error", () => {
    addToCart({ ...item, price: "0.10" }, 3);
    addToCart({ ...item, variant_id: "v2", price: "0.20" }, 1);
    expect(displaySubtotalSen(getCart())).toBe(50);
    clearCart();
    expect(cartCount()).toBe(0);
  });

  it("data storage rusak diabaikan", () => {
    store.set("umkm_cart_v1", "{bukan json");
    expect(getCart()).toEqual([]);
    store.set("umkm_cart_v1", JSON.stringify([{ variant_id: 5, quantity: 1 }, { ...item, quantity: 1 }]));
    expect(getCart()).toHaveLength(1);
  });

  it("token pesanan tersimpan, terbaru dulu, tanpa duplikat", () => {
    saveOrder({ code: "ORD-A", token: "t1", created_at: "2026-11-12" });
    saveOrder({ code: "ORD-B", token: "t2", created_at: "2026-11-12" });
    saveOrder({ code: "ORD-A", token: "t1", created_at: "2026-11-12" });
    expect(savedOrders().map((o) => o.code)).toEqual(["ORD-A", "ORD-B"]);
  });
});

describe("grid produk di renderer", () => {
  const business: Business = {
    profile: { name: "Toko", socials: [], timezone: "Asia/Jakarta", pickup_available: true, delivery_available: false },
    addresses: [], hours: [], exceptions: [],
  } as unknown as Business;
  const content: PageContent = {
    schema_version: 1, template: "modern_store", theme: { primary: "#2563EB", font: "inter" },
    sections: [{ id: "g", type: "product_grid", enabled: true, data: { title: "Produk", limit: 1 } }],
  };

  it("preview tanpa produk menampilkan placeholder; publik merender produk dengan batas", () => {
    expect(renderToStaticMarkup(<Site content={content} business={business} assetUrl={() => undefined} preview />)).toContain("Produk aktif toko tampil di sini");
    const html = renderToStaticMarkup(
      <Site content={content} business={business} assetUrl={() => undefined} products={[
        { slug: "kopi", name: "Kopi <b>", price: "18000.00", promo_price: "15000.00", available: true },
        { slug: "teh", name: "Teh", price: "5000.00", promo_price: null, available: true },
      ]} />,
    );
    expect(html).toContain('href="/produk/kopi"');
    expect(html).toContain("Kopi &lt;b&gt;");
    expect(html).toContain("<s>Rp18.000</s> Rp15.000");
    expect(html).not.toContain("/produk/teh");
  });
});
