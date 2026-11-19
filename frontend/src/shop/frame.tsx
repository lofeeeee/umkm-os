"use client";

// Bingkai halaman toko (keranjang, pesanan, akun, produk, teks hukum): navigasi, footer, dan brand kit
// yang SAMA dengan halaman website — pembeli tidak merasa pindah ke halaman generik.
// Sumber data: pratinjau Studio (konteks) → situs beranda tertanam di dokumen → ambil /_api/public/site.

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

import { API_ORIGIN } from "@/lib/api";
import { t as tx } from "@/i18n";
import { frameAttrs, frameStyle } from "@/renderer/frame";
import { storeSite } from "@/renderer/inline";
import { PublicNav, type NavLink } from "@/renderer/PublicNav";
import { SiteFooter } from "@/renderer/SiteFooter";
import type { Business, OpenStatus, PublicSite } from "@/renderer/types";
import type { PageV2 } from "@/studio/doc";

import { ShopNavActions } from "./nav";

export type ShopTexts = Partial<Record<"cart_title" | "cart_note" | "empty_cart" | "checkout_button" | "thanks" | "account_note", string>>;

export interface ShopSite {
  theme: Partial<PageV2["theme"]> & { shop?: ShopTexts };
  business: Business;
  status?: OpenStatus | null;
  pages: { slug: string; title: string; article?: unknown }[];
  platformUrl?: string | null;
  assetUrl: (ref: string) => string | undefined;
  /** Pratinjau Studio: tanpa tautan aktif & tanpa panggilan API. */
  preview?: boolean;
}

/** Pratinjau Studio menyediakan situsnya sendiri (tema yang sedang diedit). */
export const ShopSiteCtx = createContext<ShopSite | null>(null);

function fromPublic(s: PublicSite): ShopSite {
  return {
    theme: (s.page.content as { theme?: ShopSite["theme"] }).theme ?? {},
    business: s.business,
    status: s.status,
    pages: s.pages ?? [],
    platformUrl: s.platform_url,
    assetUrl: (r) => (s.assets[r] ? `${API_ORIGIN}${s.assets[r]}` : undefined),
  };
}

export function useShopSite(): ShopSite | null {
  const ctx = useContext(ShopSiteCtx);
  const [fetched, setFetched] = useState<ShopSite | null>(null);
  const inline = ctx ? null : storeSite();
  useEffect(() => {
    if (ctx || inline) return;
    fetch("/_api/public/site?page=home", { credentials: "omit" })
      .then((r) => (r.ok ? r.json() : null))
      .then((b) => { if (b?.success) setFetched(fromPublic(b.data as PublicSite)); })
      .catch(() => {});
  }, [ctx, inline]);
  return ctx ?? (inline ? fromPublic(inline) : fetched);
}

/** Teks halaman sistem dari brand kit (bawaan bila pemilik belum mengubahnya). */
export function useShopTexts(): ShopTexts {
  return useShopSite()?.theme.shop ?? {};
}

export function ShopFrame({ title, heading = true, wide, children }: { title: string; heading?: boolean; wide?: boolean; children: ReactNode }) {
  const site = useShopSite();
  const preview = !!site?.preview;
  const theme = site?.theme ?? {};
  const name = site?.business.profile.name ?? tx("Beranda toko");
  const logoUrl = site?.business.profile.logo ? site.assetUrl(site.business.profile.logo) : undefined;
  const pages = site?.pages ?? [];
  const links: NavLink[] = pages.filter((p) => !p.article).map((p) => ({ href: p.slug === "home" ? "/" : `/${p.slug}`, label: p.title }));
  const side = theme.navbar_style === "side";
  const actions = <ShopNavActions preview={preview} />;
  return (
    <div className={`sv2 site shop${side ? " s-has-side-nav" : ""}`} {...frameAttrs(theme)} style={frameStyle(theme)} data-preview={preview || undefined}>
      <div className="sv2-shell">
        <PublicNav style={side ? "side" : "top"} sticky={theme.navbar_sticky === true} brand={name} logoUrl={logoUrl} links={links} actions={actions} />
        <div className="sv2-main">
          <main className={`shop-main${wide ? " shop-main-wide" : ""}`}>
            {heading && <h1 className="shop-title">{title}</h1>}
            {children}
          </main>
          {site && (
            <SiteFooter business={site.business} status={site.status} logoUrl={logoUrl} pages={links.length > 1 ? links : []}
              editing={preview} badge={theme.hide_badge !== true} platformUrl={site.platformUrl} />
          )}
        </div>
      </div>
    </div>
  );
}
