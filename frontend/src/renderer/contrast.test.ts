// P9-6 (WCAG 1.4.3): teks di atas warna utama toko selalu terbaca (≥ 4.5:1), termasuk warna bebas pemilik toko.
import { describe, expect, it } from "vitest";

import { PALETTES } from "@/studio/plugins";
import { TEMPLATES } from "@/studio/templates";

import { contrast, onColor, primaryVars } from "./safe";

describe("kontras warna utama", () => {
  it("rasio WCAG sesuai nilai acuan", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 1);
    expect(contrast("#ffffff", "#ffffff")).toBeCloseTo(1, 5);
    expect(contrast("#767676", "#ffffff")).toBeCloseTo(4.54, 1); // abu-abu batas AA yang dikenal
  });
  it("warna terang mendapat teks gelap, warna gelap teks putih", () => {
    expect(onColor("#A78BFA")).toBe("#111111"); // lavender (template Malam): putih hanya ±2.7:1
    expect(onColor("#FACC15")).toBe("#111111"); // kuning
    expect(onColor("#0369A1")).toBe("#ffffff");
    expect(onColor("bukan-warna")).toBe("#ffffff"); // fallback aman (#2563EB)
  });
  it("setiap template & palet Studio: pasangan teks ≥ 4.5:1", () => {
    const primaries = [
      ...TEMPLATES.map((t) => t.build({ name: "x", description: null }).primary),
      ...PALETTES.map((p) => p.primary),
    ];
    for (const p of primaries) expect(contrast(p, onColor(p)), p).toBeGreaterThanOrEqual(4.5);
  });
  it("variabel CSS: warna utama, teksnya, dan arah gradien promo", () => {
    expect(primaryVars("#A78BFA")).toEqual({ "--s-primary": "#A78BFA", "--s-on-primary": "#111111", "--s-shade": "#ffffff" });
    expect(primaryVars("#C2410C")["--s-shade"]).toBe("#000000");
  });
});
