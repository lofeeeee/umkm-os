// Kontras warna halaman publik: memastikan kombinasi token situs/dasbor selalu terbaca.
//
// Bug yang dicegah (UAT 2026-11): `/syarat-transaksi` memakai `--ink-2` (token dasbor).
// Dalam mode gelap OS, `--ink-2` = #d4d4d8; di atas latar toko putih kontrasnya hanya
// 1.48:1 → teks praktis tak terlihat.
import { describe, expect, it } from "vitest";

import { contrast } from "./contrast";

/** Token situs (brand kit) — `src/renderer/site.css` `.site`. */
const SITE_LIGHT = { bg: "#ffffff", ink: "#0f172a", ink2: "#334155", muted: "#64748b" };
/** Permukaan gelap brand kit — `.sv2[data-surface="dark"]`. */
const SITE_DARK = { bg: "#0d0d12", ink: "#f4f4f5", ink2: "#d4d4d8", muted: "#a1a1aa" };
/** Token dasbor — `src/app/globals.css` `:root` (mode terang & gelap). */
const DASH_LIGHT = { bg: "#ffffff", ink: "#18181b", ink2: "#3f3f46", muted: "#63636e" };
const DASH_DARK = { bg: "#09090b", ink: "#fafafa", ink2: "#d4d4d8", muted: "#a1a1aa" };

const AA_TEXT = 4.5;
const AA_LARGE = 3;

describe("kontras halaman publik", () => {
  it("token situs pada latar putih selalu terbaca (AA)", () => {
    expect(contrast(SITE_LIGHT.ink, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_LIGHT.ink2, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_LIGHT.muted, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
  });

  it("token situs pada latar gelap selalu terbaca (AA)", () => {
    expect(contrast(SITE_DARK.ink, SITE_DARK.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_DARK.ink2, SITE_DARK.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_DARK.muted, SITE_DARK.bg)).toBeGreaterThanOrEqual(AA_TEXT);
  });

  it("BUG LAMA: token dasbor mode gelap di atas latar toko putih → tak terbaca", () => {
    // Inilah penyebab "/syarat-transaksi tidak terlihat" saat OS dalam mode gelap.
    expect(contrast(DASH_DARK.ink2, SITE_LIGHT.bg)).toBeLessThan(AA_TEXT);
    expect(contrast(DASH_DARK.muted, SITE_LIGHT.bg)).toBeLessThan(AA_TEXT);
  });

  it("PERBAIKAN: teks legal memakai token situs → terbaca di kedua latar", () => {
    // `.legal-doc p` → var(--s-ink-2, var(--ink-2)) → memakai --s-ink-2 saat tersedia.
    expect(contrast(SITE_LIGHT.ink2, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_DARK.ink2, SITE_DARK.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    // Fallback token dasbor (dipakai di konteks dasbor) juga harus aman.
    expect(contrast(DASH_LIGHT.ink2, DASH_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(DASH_DARK.ink2, DASH_DARK.bg)).toBeGreaterThanOrEqual(AA_TEXT);
  });

  it("PERBAIKAN: halaman cadangan (di luar bingkai) memakai token situs", () => {
    expect(contrast(SITE_LIGHT.ink, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(contrast(SITE_LIGHT.muted, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_TEXT);
  });

  it("judul besar minimal AA-large (3:1) terpenuhi", () => {
    expect(contrast(SITE_LIGHT.ink, SITE_LIGHT.bg)).toBeGreaterThanOrEqual(AA_LARGE);
    expect(contrast(SITE_DARK.ink, SITE_DARK.bg)).toBeGreaterThanOrEqual(AA_LARGE);
  });
});
