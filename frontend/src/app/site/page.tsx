"use client";

// Shell situs publik tenant (ADR-0014). Caddy menyajikan file ini untuk SEMUA path di
// {slug}.ROOT_DOMAIN; data diambil same-origin dari /_api/public/* (tenant dari Host).
// Path toko (P3): /produk/{slug}, /keranjang, /pesanan, /akun, /akun/verifikasi, teks hukum.

import { useEffect, useState, useSyncExternalStore } from "react";

import { API_ORIGIN } from "@/lib/api";
import { Site, type GridProduct } from "@/renderer/Site";
import { StudioSite } from "@/renderer/StudioSite";
import { isV2 } from "@/studio/doc";
import type { PublicSite } from "@/renderer/types";
import { fileUrl, shopApi, type PublicProduct } from "@/shop/api";
import { route } from "@/shop/route";
import { ShopNavActions } from "@/shop/nav";
import { AccountPage, CartFab, CartPage, LegalPage, OrderPage, ProductPage, VerifyPage } from "@/shop/views";
import { ChatWidget } from "@/chat/widget";
import { inlinePage } from "@/renderer/inline";
import { FontLoader } from "@/renderer/FontLoader";
import "@/shop/shop.css";
import { t } from "@/i18n";

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
  const [site, setSite] = useState<PublicSite | null>(() => inlinePage(slug)?.site ?? null);
  const [products, setProducts] = useState<GridProduct[] | undefined>(() => inlinePage(slug)?.products?.items.map(toGrid));
  const [state, setState] = useState<"loading" | "missing" | "inactive" | "error">("loading");
  useEffect(() => {
    const inline = inlinePage(slug);
    if (inline) {
      document.title = `${inline.site.business.profile.name} - UMKM OS`;
      return;
    }
    fetch(`/_api/public/site?page=${encodeURIComponent(slug)}`, { credentials: "omit" })
      .then(async (r) => {
        if (r.status === 404) return setState("missing");
        // ADR-0016: langganan toko habis.
        if (r.status === 403) return setState("inactive");
        const body = await r.json();
        if (!r.ok || !body?.success) return setState("error");
        const data = body.data as PublicSite;
        setSite(data);
        document.title = `${data.business.profile.name} - UMKM OS`;
        // v1: section product_grid aktif; v2 (Studio): blok product_grid (ambil limit terbesar).
        const content = data.page.content as unknown;
        const grid = isV2(content)
          ? content.blocks.filter((b) => b.kind === "product_grid").map((b) => ({ data: b.data })).sort((a, b) => Number(b.data.limit ?? 0) - Number(a.data.limit ?? 0))[0]
          : data.page.content.sections?.find((s) => s.type === "product_grid" && s.enabled);
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
    const content = site.page.content as unknown;
    const assetUrl = (r: string) => (site.assets[r] ? `${API_ORIGIN}${site.assets[r]}` : undefined);
    if (isV2(content)) {
      // ADR-0017: dokumen Studio. Chatbot tampil hanya bila plugin Chatbot dipasang di halaman.
      return (
        <main>
          <FontLoader font={content.theme?.font} />
          <StudioSite
            doc={content}
            business={site.business}
            status={site.status}
            products={products ?? []}
            assetUrl={assetUrl}
            pages={(site.pages ?? []).map((p) => ({ slug: p.slug, title: p.title, current: p.slug === site.page.slug, article: p.article }))}
            platformUrl={site.platform_url}
            navActions={<ShopNavActions />}
          />
          <CartFab />
        </main>
      );
    }
    return (
      <main>
        <FontLoader font={site.page.content.theme?.font} />
        <Site
          content={site.page.content}
          business={site.business}
          status={site.status}
          products={products ?? []}
          assetUrl={(r) => (site.assets[r] ? `${API_ORIGIN}${site.assets[r]}` : undefined)}
          navActions={<ShopNavActions />}
        />
        <CartFab />
        <ChatWidget />
      </main>
    );
  }
  return <Fallback state={state} />;
}

/** Kerangka halaman toko selama JS & data dimuat: bentuk situs (bilah atas, banner, kartu produk),
 *  bukan teks "Memuat…" — terasa lebih cepat dan tidak melompat saat isi tampil. */
function SiteSkeleton() {
  return (
    <main className="s-skel" aria-busy="true" aria-label={t("Memuat…")}>
      <div className="s-skel-bar"><span className="s-skel-logo" /><span className="s-skel-line" /></div>
      <div className="s-skel-hero"><span className="s-skel-line s-skel-h1" /><span className="s-skel-line" /><span className="s-skel-line s-skel-short" /><span className="s-skel-btn" /></div>
      <div className="s-skel-grid">{[0, 1, 2, 3].map((i) => <span key={i} className="s-skel-card" />)}</div>
    </main>
  );
}

function Fallback({ state }: { state: "loading" | "missing" | "inactive" | "error" }) {
  if (state === "loading") return <SiteSkeleton />;
  return (
    // `site-fallback` memakai token situs (--s-*) dengan fallback ke token dasbor: halaman ini
    // berada DI LUAR bingkai brand kit, jadi --s-* tidak terpasang. Tanpa itu, di mode gelap OS
    // teks ikut --muted terang di atas latar putih = tidak terbaca.
    <main className="d-narrow site-fallback">
      {state === "missing" && <><h1>{t("Halaman tidak ditemukan")}</h1><p className="site-fallback-muted">{t("Situs ini belum tersedia.")}</p></>}
      {state === "inactive" && <><h1>{t("Toko sedang tidak aktif")}</h1><p className="site-fallback-muted">{t("Silakan kembali lagi nanti.")}</p></>}
      {state === "error" && <><h1>{t("Terjadi gangguan")}</h1><p className="site-fallback-muted">{t("Coba muat ulang beberapa saat lagi.")}</p></>}
    </main>
  );
}

const noop = () => () => {};

export default function PublicSitePage() {
  // Path hanya diketahui di browser; render statis = "memuat" (hydration aman).
  const path = useSyncExternalStore(noop, () => window.location.pathname, () => null);
  if (path === null) return <Fallback state="loading" />;
  const r = route(path);
  if (r.kind === "missing") return <Fallback state="missing" />;
  return (
    <>
      <RoutePage r={r} />
      {/* Web chat (P6-2): tampil hanya bila pemilik toko mengaktifkan asisten. Halaman website
          menentukan sendiri (v2: dari plugin Chatbot di Studio). */}
      {r.kind !== "page" && <ChatWidget />}
    </>
  );
}

function RoutePage({ r }: { r: Exclude<ReturnType<typeof route>, { kind: "missing" }> }) {
  switch (r.kind) {
    case "page": return <WebsitePage slug={r.slug} />;
    case "product": return <ProductPage slug={r.slug} />;
    case "cart": return <CartPage />;
    case "orders": return <OrderPage />;
    case "account": return <AccountPage />;
    case "verify": return <VerifyPage />;
    case "privacy": return <LegalPage kind="privacy" />;
    case "terms": return <LegalPage kind="terms" />;
  }
}
