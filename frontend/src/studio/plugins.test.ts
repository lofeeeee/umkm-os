// ADR-0019 (P9-11): paket plugin dari manifest = server; penandaan "Butuh paket Pro" di editor.
import { describe, expect, it } from "vitest";

import { PALETTES, PLUGINS, needsUpgrade, pluginPlan } from "./plugins";

/** HSL dari `#RRGGBB` (untuk uji jarak rona palet). */
function hsl(hex: string): [number, number, number] {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
  const [r, g, b] = c;
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b);
  let h = 0, s = 0;
  const l = (mx + mn) / 2;
  if (mx !== mn) {
    const d = mx - mn;
    s = l > 0.5 ? d / (2 - mx - mn) : d / (mx + mn);
    switch (mx) {
      case r: h = (g - b) / d + (g < b ? 6 : 0); break;
      case g: h = (b - r) / d + 2; break;
      default: h = (r - g) / d + 4;
    }
    h /= 6;
  }
  return [h * 360, s * 100, l * 100];
}

describe("paket plugin", () => {
  it("Fase 1: semua plugin resmi starter; jenis tak dikenal diperlakukan pro (fail-closed)", () => {
    expect(PLUGINS.every((p) => pluginPlan(p.kind) === "starter")).toBe(true);
    expect(pluginPlan("formulir-baru")).toBe("pro");
  });
  it("tanda hanya untuk plugin pro pada toko non-Pro yang paketnya diketahui", () => {
    expect(needsUpgrade("pro", "starter")).toBe(true);
    expect(needsUpgrade("pro", "pro")).toBe(false);
    expect(needsUpgrade("pro", null)).toBe(false);
    expect(needsUpgrade("starter", "starter")).toBe(false);
  });
});

describe("palet: pasangan rona terlalu mirip (dapat dikira-kira sama pada swatch)", () => {
  it("dua palet kromatik tidak boleh berjarak rona <12° sekaligus ΔL <15", () => {
    const H = PALETTES.map((p) => ({ name: p.name, hsl: hsl(p.primary) }));
    for (let i = 0; i < H.length; i++) {
      for (let j = i + 1; j < H.length; j++) {
        const a = H[i]!, b = H[j]!;
        let dh = Math.abs(a.hsl[0] - b.hsl[0]);
        if (dh > 180) dh = 360 - dh;
        const chromatic = a.hsl[1] > 20 && b.hsl[1] > 20; // Abu (s≈5) bebas — abu-abu netral mirip semua
        const label = `${a.name} vs ${b.name} (Δhue=${dh.toFixed(0)}°, ΔL=${Math.abs(a.hsl[2] - b.hsl[2]).toFixed(0)})`;
        const tooClose = chromatic && dh < 12 && Math.abs(a.hsl[2] - b.hsl[2]) < 15;
        expect(tooClose, label).toBe(false);
      }
    }
  });
});
