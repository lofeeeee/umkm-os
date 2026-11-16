import { describe, expect, it } from "vitest";

import { COLS, MAX_BLOCK_H, normalizePageV2 } from "./normalize";

const block = (over: Record<string, unknown> = {}) => ({
  id: "hero-1", kind: "hero", x: 0, y: 0, w: 12, h: 8, style: "plain", data: { title: "Halo" }, ...over,
});

describe("normalisasi dokumen v2", () => {
  it("menerima dokumen yang sudah sah tanpa mengubah isinya", () => {
    const doc = normalizePageV2({
      schema_version: 2, template: "hangat",
      theme: { primary: "#C2410C", font: "poppins", surface: "cream", text_scale: 0.9, navbar_style: "side", navbar_sticky: true },
      blocks: [block()],
    });
    expect(doc.schema_version).toBe(2);
    expect(doc.template).toBe("hangat");
    expect(doc.theme).toMatchObject({ primary: "#C2410C", font: "poppins", surface: "cream", text_scale: 0.9, navbar_style: "side", navbar_sticky: true });
    expect(doc.blocks).toHaveLength(1);
    expect(doc.blocks[0]).toMatchObject({ id: "hero-1", kind: "hero", x: 0, y: 0, w: 12, h: 8, style: "plain" });
  });

  it("bentuk tak terduga tidak melempar: null, array, string, angka", () => {
    for (const raw of [null, undefined, [], "x", 42, true]) {
      const doc = normalizePageV2(raw);
      expect(doc.schema_version).toBe(2);
      expect(doc.blocks).toEqual([]);
      expect(doc.theme.primary).toBeTruthy();
      expect(doc.theme.font).toBeTruthy();
    }
  });

  it("plugin tak dikenal dibuang (fail-closed), bukan dirender", () => {
    const doc = normalizePageV2({ blocks: [block({ kind: "formulir-baru" }), block({ id: "b2", kind: "text" })] });
    expect(doc.blocks.map((b) => b.kind)).toEqual(["text"]);
  });

  it("blok bukan objek dibuang", () => {
    const doc = normalizePageV2({ blocks: [null, "x", 7, block()] });
    expect(doc.blocks).toHaveLength(1);
  });

  it("koordinat grid di-clamp agar blok tidak keluar kanvas 12 kolom", () => {
    const doc = normalizePageV2({ blocks: [block({ x: 10, w: 8 })] });
    expect(doc.blocks[0]!.w).toBe(8);
    expect(doc.blocks[0]!.x).toBe(COLS - 8);
    expect(doc.blocks[0]!.x + doc.blocks[0]!.w).toBeLessThanOrEqual(COLS);
  });

  it("lebar/tinggi tak sah jatuh ke nilai aman; tinggi dibatasi", () => {
    const doc = normalizePageV2({ blocks: [block({ w: 0, h: 9999 }), block({ id: "b2", w: 99, h: -3 })] });
    expect(doc.blocks[0]!.w).toBeGreaterThanOrEqual(1);
    expect(doc.blocks[0]!.h).toBe(MAX_BLOCK_H);
    expect(doc.blocks[1]!.w).toBe(COLS);
    expect(doc.blocks[1]!.h).toBeGreaterThanOrEqual(1);
  });

  it("x negatif jadi 0 dan y negatif jadi 0", () => {
    const doc = normalizePageV2({ blocks: [block({ x: -5, y: -9 })] });
    expect(doc.blocks[0]).toMatchObject({ x: 0, y: 0 });
  });

  it("id duplikat diberi sufiks agar kunci React unik", () => {
    const doc = normalizePageV2({ blocks: [block({ id: "sama" }), block({ id: "sama" }), block({ id: "sama" })] });
    expect(doc.blocks.map((b) => b.id)).toEqual(["sama", "sama-2", "sama-3"]);
  });

  it("id tak sah dibersihkan (huruf kecil, tanpa garis bawah/spasi)", () => {
    const doc = normalizePageV2({ blocks: [block({ id: "Hero_2 Besar!" })] });
    expect(doc.blocks[0]!.id).toBe("hero-2-besar");
  });

  it("gaya tak dikenal jatuh ke plain", () => {
    const doc = normalizePageV2({ blocks: [block({ style: "neon" })] });
    expect(doc.blocks[0]!.style).toBe("plain");
  });

  it("layout hanya diterima bila plugin memang punya varian itu", () => {
    const doc = normalizePageV2({
      blocks: [
        block({ id: "c1", kind: "chatbot", layout: "bar" }),
        block({ id: "c2", kind: "chatbot", layout: "melayang-aneh" }),
        block({ id: "c3", kind: "hero", layout: "grid" }),
      ],
    });
    expect(doc.blocks[0]!.layout).toBe("bar");
    expect(doc.blocks[1]!.layout).toBeUndefined();
    expect(doc.blocks[2]!.layout).toBeUndefined();
  });

  it("hide_on di luar mobile/desktop dibuang", () => {
    const doc = normalizePageV2({ blocks: [block({ hide_on: "mobile" }), block({ id: "b2", hide_on: "tablet" })] });
    expect(doc.blocks[0]!.hide_on).toBe("mobile");
    expect(doc.blocks[1]!.hide_on).toBeUndefined();
  });

  it("permukaan & skala teks di luar daftar server dibuang, bukan diteruskan", () => {
    const doc = normalizePageV2({ theme: { surface: "ungu", text_scale: 1.5 } });
    expect(doc.theme.surface).toBeUndefined();
    expect(doc.theme.text_scale).toBeUndefined();
  });

  it("skala teks yang sah dipertahankan (termasuk 1.0)", () => {
    expect(normalizePageV2({ theme: { text_scale: 1.1 } }).theme.text_scale).toBe(1.1);
    expect(normalizePageV2({ theme: { text_scale: 1 } }).theme.text_scale).toBe(1);
  });

  it("opasitas latar di-clamp 0..100", () => {
    expect(normalizePageV2({ theme: { background_opacity: 250 } }).theme.background_opacity).toBe(100);
    expect(normalizePageV2({ theme: { background_opacity: -20 } }).theme.background_opacity).toBe(0);
  });

  it("navbar_style tak dikenal dibuang sehingga renderer memakai default atas", () => {
    expect(normalizePageV2({ theme: { navbar_style: "bawah" } }).theme.navbar_style).toBeUndefined();
    expect(normalizePageV2({ theme: { navbar_style: "top" } }).theme.navbar_style).toBe("top");
  });

  it("data blok yang bukan objek jadi objek kosong (renderer tetap aman)", () => {
    const doc = normalizePageV2({ blocks: [block({ data: "bukan objek" })] });
    expect(doc.blocks[0]!.data).toEqual({});
  });

  it("seo kosong dibuang; seo berisi dipertahankan", () => {
    expect(normalizePageV2({ seo: { title: "  " } }).seo).toBeUndefined();
    expect(normalizePageV2({ seo: { title: "Kopi", description: "Enak" } }).seo).toEqual({ title: "Kopi", description: "Enak" });
  });

  it("blocks bukan array jadi daftar kosong", () => {
    expect(normalizePageV2({ blocks: { a: 1 } }).blocks).toEqual([]);
  });

  it("template tak sah dibersihkan ke kunci aman", () => {
    expect(normalizePageV2({ template: "../../etc/passwd" }).template).toBe("etc-passwd");
  });
});

describe("normalisasi: sifat yang dibutuhkan renderer", () => {
  it("idempoten: normalisasi dua kali menghasilkan dokumen yang sama", () => {
    const once = normalizePageV2({ theme: { primary: "#111111" }, blocks: [block({ x: 20, w: 20 })] });
    expect(normalizePageV2(once)).toEqual(once);
  });

  it("tidak membocorkan key tema di luar daftar server (publish akan ditolak Rust)", () => {
    const doc = normalizePageV2({ theme: { radiusScale: "pill", density: "compact", primary: "#123456" } });
    expect(Object.keys(doc.theme).sort()).toEqual(["font", "primary"]);
  });

  it("urutan blok dipertahankan (tata letak grid bergantung padanya)", () => {
    const doc = normalizePageV2({ blocks: [block({ id: "a" }), block({ id: "b" }), block({ id: "c" })] });
    expect(doc.blocks.map((b) => b.id)).toEqual(["a", "b", "c"]);
  });

  it("dokumen besar tetap dinormalisasi tanpa melempar", () => {
    const many = Array.from({ length: 200 }, (_, i) => block({ id: `b${i}` }));
    expect(normalizePageV2({ blocks: many }).blocks).toHaveLength(200);
  });
});
