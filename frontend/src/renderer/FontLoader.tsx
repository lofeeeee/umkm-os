"use client";

// Memuat CSS huruf per-preset. Semua chunk didaftarkan statis (wajib agar webpack membuat
// berkas terpisah), lalu dimuat malas hanya untuk preset yang benar-benar dipakai — masing-masing
// ~1–3 KB gzip. "system" tidak punya berkas: tidak ada yang dimuat.

import { useEffect } from "react";

import { fontCssImportForPreset } from "@/renderer/safe";

const loaded = new Set<string>();

/** Impor malas tiap chunk. Kunci = path dari fontCssImportForPreset ("./fonts/…" relatif ke safe.ts). */
const CHUNKS: Record<string, () => Promise<unknown>> = {
  "./fonts/plus-jakarta-sans.css": () => import("./fonts/plus-jakarta-sans.css"),
  "./fonts/inter.css": () => import("./fonts/inter.css"),
  "./fonts/poppins.css": () => import("./fonts/poppins.css"),
  "./fonts/nunito.css": () => import("./fonts/nunito.css"),
  "./fonts/merriweather.css": () => import("./fonts/merriweather.css"),
  "./fonts/fraunces.css": () => import("./fonts/fraunces.css"),
  "./fonts/playfair.css": () => import("./fonts/playfair.css"),
  "./fonts/grotesk.css": () => import("./fonts/grotesk.css"),
  "./fonts/caveat.css": () => import("./fonts/caveat.css"),
  "./fonts/dmserif.css": () => import("./fonts/dmserif.css"),
};

/** Muat chunk CSS huruf untuk preset tema; no-op untuk "system" / preset tak dikenal. */
export function ensureFontCss(font: string | null | undefined) {
  const href = fontCssImportForPreset(font);
  if (!href || loaded.has(href)) return;
  loaded.add(href);
  void CHUNKS[href]?.().catch(() => loaded.delete(href));
}

/** Komponen: memuat CSS huruf saat preset berubah (situs publik & pratinjau Studio). */
export function FontLoader({ font }: { font: string | null | undefined }) {
  useEffect(() => {
    ensureFontCss(font);
  }, [font]);
  return null;
}
