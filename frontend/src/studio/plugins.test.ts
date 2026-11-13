// ADR-0019 (P9-11): paket plugin dari manifest = server; penandaan "Butuh paket Pro" di editor.
import { describe, expect, it } from "vitest";

import { PLUGINS, needsUpgrade, pluginPlan } from "./plugins";

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
