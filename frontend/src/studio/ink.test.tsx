import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { InkLayer, cellTarget, pointsToPath } from "@/renderer/ink";
import { StudioSite } from "@/renderer/StudioSite";
import type { Business } from "@/renderer/types";

import { toDoc, type PageV2, type StudioState } from "./doc";
import { DrawPanel, type DrawConfig } from "./DrawPanel";
import type { DrawingStroke } from "./DrawingLayer";

const stroke = (id: string, target: string, points: [number, number][] = [[10, 10], [20, 20], [30, 10]]): DrawingStroke =>
  ({ id, target, color: "#EF4444", size: 4, opacity: 1, points });

describe("pointsToPath", () => {
  it("menghasilkan kurva kuadratik bezier halus (M ... Q ...)", () => {
    expect(pointsToPath([])).toBe("");
    expect(pointsToPath([[10, 20]])).toBe("M 10 20 L 10 20");
    expect(pointsToPath([[10, 20], [30, 40]])).toMatch(/^M 10 20 Q/);
    expect(pointsToPath([[0, 0], [10, 10], [20, 0], [30, 10]])).toMatch(/^M 0 0( Q \d+(\.\d+)? \d+(\.\d+)? \d+(\.\d+)? \d+(\.\d+)?)+$/);
  });
});

describe("InkLayer", () => {
  it("viewBox persegi selebar wadah: skala x = y dan coretan tidak gepeng", () => {
    const html = renderToStaticMarkup(<InkLayer strokes={[stroke("a", "background")]} viewWidth={1080} />);
    expect(html).toContain('viewBox="0 0 1080 1080"');
    expect(html).toContain('preserveAspectRatio="xMinYMin meet"');
    expect(html).toContain('stroke="#EF4444"');
    expect(html).toContain('stroke-linecap="round"');
  });
  it("tidak merender apa pun tanpa coretan", () => {
    expect(renderToStaticMarkup(<InkLayer strokes={[]} viewWidth={1080} />)).toBe("");
  });
});

describe("coretan di situs terbit", () => {
  const biz: Business = { profile: { name: "Toko", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false }, addresses: [], hours: [], exceptions: [] };
  const doc: PageV2 = {
    schema_version: 2, template: "t", theme: { primary: "#4F46E5", font: "inter" },
    blocks: [{ id: "hero", kind: "text", x: 0, y: 0, w: 6, h: 4, style: "plain", data: { body: "Halo" } }],
    drawings: [stroke("bg-1", "background"), stroke("c-1", cellTarget("hero")), stroke("legacy", "hero")],
  };
  it("mode pelanggan merender coretan latar & coretan blok (termasuk target lama tanpa awalan)", () => {
    const html = renderToStaticMarkup(<StudioSite doc={doc} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain('data-stroke-id="bg-1"');
    expect(html).toContain('data-stroke-id="c-1"');
    expect(html).toContain('data-stroke-id="legacy"');
    // Coretan blok memakai lebar desain blok 6 kolom: 6×79 + 5×12 = 534.
    expect(html).toContain('viewBox="0 0 534 534"');
  });
});

describe("toDoc: target coretan selalu sah bagi server", () => {
  const st = (drawings: DrawingStroke[]): StudioState => ({
    primary: "#4F46E5", surface: "light", font: "inter", installed: [], drawings,
    cells: [{ id: "hero-1", kind: "text", x: 0, y: 0, w: 6, h: 4, style: "plain", data: { body: "x" } }],
  });
  it("target lama `<id>` menjadi `cell:<id>`; coretan blok yang sudah dihapus dibuang", () => {
    const d = toDoc(st([stroke("a", "hero-1"), stroke("b", "cell:hero-1"), stroke("c", "cell:hilang"), stroke("d", "background")]), "t");
    expect(d.drawings?.map((x) => `${x.id}:${x.target}`)).toEqual(["a:cell:hero-1", "b:cell:hero-1", "d:background"]);
  });
  it("titik dibulatkan, tidak negatif, maksimal 1000", () => {
    const pts = Array.from({ length: 1500 }, (_, i) => [i + 0.4, -3] as [number, number]);
    const d = toDoc(st([stroke("a", "background", pts)]), "t");
    expect(d.drawings![0]!.points.length).toBe(1000);
    expect(d.drawings![0]!.points[0]).toEqual([0, 0]);
  });
});

describe("DrawPanel", () => {
  const config: DrawConfig = { color: "#EF4444", size: 4, opacity: 1 };
  const html = renderToStaticMarkup(
    <DrawPanel activeTool="brush" onSelectTool={() => {}} target="cell" onSelectTarget={() => {}} config={config}
      onChangeConfig={() => {}} primaryColor="#4F46E5" strokeCount={3} onClearStrokes={() => {}} />,
  );
  it("alat, target, ketebalan, warna, dan hapus semua", () => {
    for (const t of ["Kuas", "Penghapus", "Blok di bawahnya", "Latar halaman", "Halus", "Stabilo", "Padat", "Transparan", "Hapus semua coretan (3)"]) {
      expect(html).toContain(t);
    }
    expect(html).toContain("#4F46E5");
    expect(html).toContain('type="color"');
  });
});
