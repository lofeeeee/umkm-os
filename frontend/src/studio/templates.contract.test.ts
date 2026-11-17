// Kontrak template Studio ↔ validator Rust: setiap template dikonversi ke dokumen v2 dan dibandingkan
// dengan fixture yang divalidasi crates/domain/tests/frontend_contract.rs.
// Perbarui fixture: UPDATE_FIXTURES=1 npx vitest run src/studio/templates.contract.test.ts
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { beforeAll, describe, expect, it } from "vitest";

import { toDoc } from "./doc";
import manifest from "./plugin-manifest.json";
import { EXTRA_DATA, PLUGINS, type PluginKind } from "./plugins";
import { TEMPLATES, type TplCell } from "./templates";
import { getLocale, loadEnglish, setLocale } from "@/i18n";

// Kamus EN dimuat malas (chunk terpisah); tes memuatnya dulu agar setLocale("en") langsung berlaku.
beforeAll(() => loadEnglish());

const FIXTURE = resolve(__dirname, "../../../crates/domain/tests/fixtures/studio_templates.json");
/** Isi templat versi English (antarmuka EN) — divalidasi server juga (batas panjang teks, dsb.). */
const FIXTURE_EN = resolve(__dirname, "../../../crates/domain/tests/fixtures/studio_templates_en.json");

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

  it("fixture English sama dengan keluaran template saat antarmuka EN", () => {
    const before = getLocale();
    let now: string;
    try {
      setLocale("en", false);
      now = JSON.stringify(docs(), null, 2) + "\n";
    } finally {
      setLocale(before, false);
    }
    expect(now).toContain("Free delivery this week");
    if (process.env.UPDATE_FIXTURES) writeFileSync(FIXTURE_EN, now);
    expect(readFileSync(FIXTURE_EN, "utf8")).toBe(now);
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

// Kriteria lulus "keamanan ganti template": mengganti template harus mengubah tata letak & warna,
// tetapi data toko (nama, kontak, produk) tetap utuh karena berada di state terpisah dari dokumen.
describe("ganti template tidak menghilangkan data toko", () => {
  /** State toko yang hidup di luar dokumen website (profil + katalog). */
  const storeProfile = { name: "Kopi Senja", whatsapp: "0812-3456-789", description: "Kedai kopi rumahan" };
  const products = [{ slug: "kopi-susu", name: "Kopi Susu", price: "18000.00" }, { slug: "americano", name: "Americano", price: "15000.00" }];

  it("template apa pun hanya menghasilkan theme + blok; data toko tidak ikut berubah", () => {
    for (const t of TEMPLATES) {
      const r = t.build({ name: storeProfile.name, description: storeProfile.description });
      // Template tidak pernah menyentuh/mengembalikan data toko: hasilnya hanya state Studio.
      expect(Object.keys(r)).not.toContain("products");
      expect(Object.keys(r)).not.toContain("profile");
      // Data toko tetap sama persis setelah template dibangun.
      expect(storeProfile).toEqual({ name: "Kopi Senja", whatsapp: "0812-3456-789", description: "Kedai kopi rumahan" });
      expect(products).toHaveLength(2);
      // Dokumen hasil template tetap valid & punya tema.
      const d = toDoc({ ...r, installed: [] }, "modern_store");
      expect(d.schema_version).toBe(2);
      expect(d.theme.primary).toMatch(/^#[0-9A-Fa-f]{6}$/);
      expect(d.blocks.length).toBeGreaterThan(0);
    }
  });

  it("berpindah antar template mengubah warna/tata letak tetapi bukan data toko", () => {
    const a = toDoc({ ...TEMPLATES[0]!.build({ name: storeProfile.name, description: storeProfile.description }), installed: [] }, "modern_store");
    const b = toDoc({ ...TEMPLATES[1]!.build({ name: storeProfile.name, description: storeProfile.description }), installed: [] }, "modern_store");
    // Tata letak/warna berbeda...
    expect(`${a.theme.primary}|${a.blocks.map((x) => x.kind).join(",")}`).not.toBe(`${b.theme.primary}|${b.blocks.map((x) => x.kind).join(",")}`);
    // ...tetapi data toko tidak tersimpan di dokumen, jadi tidak mungkin hilang.
    for (const d of [a, b]) {
      expect(JSON.stringify(d)).not.toContain("Kopi Susu");
      expect(d.schema_version).toBe(2);
    }
    expect(storeProfile.name).toBe("Kopi Senja");
    expect(products.map((p) => p.slug)).toEqual(["kopi-susu", "americano"]);
  });
});
