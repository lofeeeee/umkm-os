// Bingkai situs v2 dari tema (brand kit): variabel CSS & atribut data. Dipakai halaman website (StudioSite)
// dan halaman toko (keranjang, pesanan, akun, produk) supaya keduanya identik secara visual.

import type { CSSProperties } from "react";

import type { PageV2 } from "@/studio/doc";

import { fontStack, onColor, primaryVars, safeColor } from "./safe";

type Theme = Partial<PageV2["theme"]> & { primary?: string; font?: string };

export function frameStyle(t: Theme): CSSProperties {
  const accent = safeColor(t.accent ?? t.primary);
  return {
    ...primaryVars(t.primary),
    "--s-font": fontStack(t.font),
    "--s-head-font": fontStack(t.heading_font ?? t.font),
    "--s-head-weight": t.heading_weight ?? 800,
    "--s-accent": accent,
    "--s-on-accent": onColor(accent),
    "--s-radius-kit": t.radius === "sharp" ? "4px" : t.radius === "round" ? "24px" : "14px",
    "--s-size": t.text_scale ?? 1,
  } as CSSProperties;
}

export function frameAttrs(t: Theme): Record<string, string | undefined> {
  return {
    "data-surface": t.surface ?? "light",
    "data-motion": t.motion && t.motion !== "none" ? t.motion : undefined,
    "data-texture": t.texture && t.texture !== "none" ? t.texture : undefined,
    "data-gradient": t.gradient ? "" : undefined,
    "data-radius": t.radius,
  };
}
