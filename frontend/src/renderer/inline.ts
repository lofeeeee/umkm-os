// Data situs yang ditanam API di dokumen HTML toko (`site_head::document`, <script id="__site">).
// Dibaca sekali saat modul dimuat (hanya di browser). Bukan sumber kebenaran: bila tidak ada
// (shell statis cadangan, build lama), halaman mengambil data lewat /_api/public/* seperti biasa.

import type { CSSProperties } from "react";

import { API_ORIGIN } from "@/lib/api";
import type { PublicProduct } from "@/shop/api";
import { fontStack, primaryVars } from "./safe";
import type { PublicSite } from "./types";

export interface InlineData {
  site: PublicSite;
  products: { items: PublicProduct[] } | null;
  product: PublicProduct | null;
}

const INLINE: InlineData | null = (() => {
  if (typeof document === "undefined") return null;
  const raw = document.getElementById("__site")?.textContent;
  if (!raw) return null;
  try {
    return JSON.parse(raw) as InlineData;
  } catch {
    return null;
  }
})();

/** Data halaman website `slug` (situs + produk galeri) bila tertanam. */
export const inlinePage = (slug: string): InlineData | null => (INLINE?.site?.page?.slug === slug ? INLINE : null);

/** Produk `slug` bila tertanam (dokumen `/produk/{slug}`). */
export const inlineProduct = (slug: string): PublicProduct | null => (INLINE?.product?.slug === slug ? INLINE.product : null);

export interface StoreBrand {
  name: string;
  logoUrl?: string;
  style: CSSProperties;
}

/** Identitas toko (nama, logo, warna & font tema beranda) untuk halaman toko (produk, keranjang, akun). */
export function storeBrand(): StoreBrand | null {
  const s = INLINE?.site;
  if (!s) return null;
  const theme = (s.page.content as { theme?: { primary?: string; font?: string } }).theme ?? {};
  const logo = s.business.profile.logo;
  const v = primaryVars(theme.primary);
  return {
    name: s.business.profile.name,
    logoUrl: logo && s.assets[logo] ? `${API_ORIGIN}${s.assets[logo]}` : undefined,
    style: {
      "--shop-primary": v["--s-primary"],
      "--shop-accent": v["--s-primary"],
      "--shop-on-primary": v["--s-on-primary"],
      fontFamily: fontStack(theme.font),
    } as CSSProperties,
  };
}

/** Situs beranda tertanam (tema situs, profil, halaman) untuk bingkai halaman toko. */
export const storeSite = (): PublicSite | null => INLINE?.site ?? null;
