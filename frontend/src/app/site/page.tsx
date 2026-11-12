"use client";

// Shell situs publik tenant (ADR-0014). Caddy menyajikan file ini untuk SEMUA path di
// {slug}.ROOT_DOMAIN; data diambil same-origin dari /_api/public/* (tenant dari Host).
// Path toko (P3): /produk/{slug}, /keranjang, /pesanan, /akun, /akun/verifikasi, teks hukum.

import { useEffect, useState, useSyncExternalStore } from "react";

import { API_ORIGIN } from "@/lib/api";
import { Site, type GridProduct } from "@/renderer/Site";
import type { PublicSite } from "@/renderer/types";
import { fileUrl, shopApi, type PublicProduct } from "@/shop/api";
import { route } from "@/shop/route";
import { AccountPage, CartFab, CartPage, LegalPage, OrderPage, ProductPage, VerifyPage } from "@/shop/views";
import "@/shop/shop.css";

function toGrid(p: PublicProduct): GridProduct {
  const v = p.variants[0];
  return {
    slug: p.slug,
    name: p.name,
    image: p.images[0] ? fileUrl(p.images[0]) : undefined,
    price: v?.price ?? "0.00",
    promo_price: v?.promo_price ?? null,
    available: p.available && p.variants.length > 0,
  };
}

function WebsitePage({ slug }: { slug: string }) {
  const [site, setSite] = useState<PublicSite | null>(null);
  const [products, setProducts] = useState<GridProduct[] | undefined>(undefined);
  const [state, setState] = useState<"loading" | "missing" | "error">("loading");
  useEffect(() => {
    fetch(`/_api/public/site?page=${encodeURIComponent(slug)}`, { credentials: "omit" })
      .then(async (r) => {
        if (r.status === 404) return setState("missing");
        const body = await r.json();
        if (!r.ok || !body?.success) return setState("error");
        const data = body.data as PublicSite;
        setSite(data);
        document.title = data.business.profile.name;
        const grid = data.page.content.sections?.find((s) => s.type === "product_grid" && s.enabled);
        if (grid) {
          const limit = typeof grid.data.limit === "number" ? grid.data.limit : 8;
          shopApi<{ items: PublicProduct[] }>("GET", `/products?limit=${Math.min(24, Math.max(1, limit))}`)
            .then((x) => setProducts(x.items.map(toGrid)))
            .catch(() => setProducts([]));
        }
      })
      .catch(() => setState("error"));
  }, [slug]);
  if (site) {
    return (
      <main>
        <Site
          content={site.page.content}
          business={site.business}
          status={site.status}
          products={products ?? []}
          assetUrl={(r) => (site.assets[r] ? `${API_ORIGIN}${site.assets[r]}` : undefined)}
        />
        <CartFab />
      </main>
    );
  }
  return <Fallback state={state} />;
}

function Fallback({ state }: { state: "loading" | "missing" | "error" }) {
  return (
    <main className="d-narrow" aria-busy={state === "loading"}>
      {state === "loading" && <p className="d-muted">Memuat…</p>}
      {state === "missing" && <><h1>Halaman tidak ditemukan</h1><p className="d-muted">Situs ini belum tersedia.</p></>}
      {state === "error" && <><h1>Terjadi gangguan</h1><p className="d-muted">Coba muat ulang beberapa saat lagi.</p></>}
    </main>
  );
}

const noop = () => () => {};

export default function PublicSitePage() {
  // Path hanya diketahui di browser; render statis = "memuat" (hydration aman).
  const path = useSyncExternalStore(noop, () => window.location.pathname, () => null);
  if (path === null) return <Fallback state="loading" />;
  const r = route(path);
  switch (r.kind) {
    case "page": return <WebsitePage slug={r.slug} />;
    case "product": return <ProductPage slug={r.slug} />;
    case "cart": return <CartPage />;
    case "orders": return <OrderPage />;
    case "account": return <AccountPage />;
    case "verify": return <VerifyPage />;
    case "privacy": return <LegalPage kind="privacy" />;
    case "terms": return <LegalPage kind="terms" />;
    case "missing": return <Fallback state="missing" />;
  }
}
