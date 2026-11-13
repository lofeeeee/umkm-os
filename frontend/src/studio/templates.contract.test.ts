// Kontrak template Studio ↔ validator Rust: setiap template dikonversi ke dokumen v2 dan dibandingkan
// dengan fixture yang divalidasi crates/domain/tests/frontend_contract.rs.
// Perbarui fixture: UPDATE_FIXTURES=1 npx vitest run src/studio/templates.contract.test.ts
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { toDoc } from "./doc";
import manifest from "./plugin-manifest.json";
import { EXTRA_DATA, PLUGINS, type PluginKind } from "./plugins";
import { TEMPLATES, type TplCell } from "./templates";

const FIXTURE = resolve(__dirname, "../../../crates/domain/tests/fixtures/studio_templates.json");

/** Plugin di luar template (Teks, WhatsApp di halaman / melayang) dengan data awal editor. */
function extraDoc(floatingWa: boolean) {
  const cell = (kind: PluginKind, x: number, y: number, w: number, h: number, layout?: string): TplCell => ({
    id: kind, kind, x, y, w, h, minW: 2, minH: 2, style: "plain", data: { ...(EXTRA_DATA[kind] ?? {}) }, layout, autoH: true, chatOpen: true,
  });
  return toDoc({
    primary: "#15803D", surface: "light", font: "inter", installed: [],
    cells: [
      { ...cell("hero", 0, 0, 12, 8), data: { title: "Toko Segar" } },
      cell("text", 0, 8, 6, 4),
      // Melayang = embed: boleh "menimpa" banner (dikecualikan dari cek tumpang tindih server).
      floatingWa ? cell("whatsapp", 0, 0, 4, 3, "floating") : cell("whatsapp", 6, 8, 6, 3, "inline"),
    ],
  }, "modern_store");
}

function docs() {
  return Object.fromEntries([...TEMPLATES.map((t) => {
    const r = t.build({ name: "Kopi Senja Bandung", description: "Kedai kopi rumahan dengan biji pilihan." });
    const d = toDoc({ ...r, installed: [] }, "modern_store");
    // id acak → deterministik untuk fixture
    d.blocks = d.blocks.map((b, i) => ({ ...b, id: `${b.kind.replace(/_/g, "-")}-${i + 1}` }));
    return [t.key, d] as const;
  }), ["plugin-teks-wa", extraDoc(false)] as const, ["plugin-wa-melayang", extraDoc(true)] as const]);
}

describe("kontrak template Studio", () => {
  it("fixture sama dengan keluaran template", () => {
    const now = JSON.stringify(docs(), null, 2) + "\n";
    if (process.env.UPDATE_FIXTURES) writeFileSync(FIXTURE, now);
    expect(readFileSync(FIXTURE, "utf8")).toBe(now);
  });
});

describe("katalog plugin Studio ↔ manifest resmi (ADR-0019)", () => {
  it("setiap plugin resmi bisa dipasang dari katalog dengan tata letak yang sama", () => {
    const m = manifest.plugins as Record<string, { layouts: string[] }>;
    expect(PLUGINS.map((p) => p.kind).sort()).toEqual(Object.keys(m).sort());
    for (const p of PLUGINS) {
      expect((p.layouts ?? []).map((l) => l.value).sort(), p.kind).toEqual([...m[p.kind]!.layouts].sort());
    }
  });
});
