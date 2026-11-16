import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { DrawPanel, type DrawConfig } from "./DrawPanel";
import { DrawingLayer, pointsToPath, type DrawingStroke } from "./DrawingLayer";

describe("DrawingLayer (Studio Enhancement Task 8)", () => {
  const sampleStrokes: DrawingStroke[] = [
    {
      id: "stroke-bg-1",
      target: "background",
      color: "#EF4444",
      size: 4,
      opacity: 1,
      points: [
        [10, 10],
        [20, 20],
        [30, 10],
      ],
    },
    {
      id: "stroke-cell-1",
      target: "cell-header",
      color: "#3B82F6",
      size: 8,
      opacity: 0.4,
      points: [
        [50, 50],
        [60, 60],
        [70, 80],
      ],
    },
  ];

  it("pointsToPath menghasilkan kurva kuadratik bezier halus (M ... Q ...)", () => {
    expect(pointsToPath([])).toBe("");
    expect(pointsToPath([[10, 20]])).toBe("M 10 20 L 10 20");
    
    const twoPointPath = pointsToPath([
      [10, 20],
      [30, 40],
    ]);
    expect(twoPointPath).toContain("M 10 20");
    expect(twoPointPath).toContain("Q");

    const multiPointPath = pointsToPath([
      [0, 0],
      [10, 10],
      [20, 0],
      [30, 10],
    ]);
    expect(multiPointPath.startsWith("M 0 0")).toBe(true);
    expect(multiPointPath).toMatch(/^M 0 0( Q \d+(\.\d+)? \d+(\.\d+)? \d+(\.\d+)? \d+(\.\d+)?)+$/);
  });

  it("merender coretan target background dan memfilter coretan target lain", () => {
    const html = renderToStaticMarkup(
      <DrawingLayer
        target="background"
        strokes={sampleStrokes}
        width={1080}
        height={600}
        activeTool={null}
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );

    // Harus merender stroke-bg-1
    expect(html).toContain('data-stroke-id="stroke-bg-1"');
    expect(html).toContain('stroke="#EF4444"');

    // Tidak boleh merender stroke-cell-1
    expect(html).not.toContain('data-stroke-id="stroke-cell-1"');
    expect(html).not.toContain('stroke="#3B82F6"');
  });

  it("merender coretan saat target adalah cell tertentu", () => {
    const html = renderToStaticMarkup(
      <DrawingLayer
        target="cell-header"
        strokes={sampleStrokes}
        width={400}
        height={200}
        activeTool={null}
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );

    expect(html).toContain('data-stroke-id="stroke-cell-1"');
    expect(html).toContain('stroke="#3B82F6"');
    expect(html).not.toContain('data-stroke-id="stroke-bg-1"');
  });

  it("memverifikasi atribut gaya SVG (color, size, opacity, linecap, linejoin, fill)", () => {
    const html = renderToStaticMarkup(
      <DrawingLayer
        target="cell-header"
        strokes={sampleStrokes}
        width={500}
        height={300}
        activeTool={null}
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );

    // Cek atribut SVG
    expect(html).toContain('stroke="#3B82F6"');
    expect(html).toContain('stroke-width="8"');
    expect(html).toContain('stroke-opacity="0.4"');
    expect(html).toContain('stroke-linecap="round"');
    expect(html).toContain('stroke-linejoin="round"');
    expect(html).toContain('fill="none"');
  });

  it("pointerEvents bernilai none saat static display (activeTool === null)", () => {
    const html = renderToStaticMarkup(
      <DrawingLayer
        target="background"
        strokes={sampleStrokes}
        width={800}
        height={500}
        activeTool={null}
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );

    expect(html).toContain("pointer-events:none");
  });

  it("pointerEvents bernilai all saat activeTool aktif (brush atau eraser)", () => {
    const brushHtml = renderToStaticMarkup(
      <DrawingLayer
        target="background"
        strokes={sampleStrokes}
        width={800}
        height={500}
        activeTool="brush"
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );
    expect(brushHtml).toContain("pointer-events:all");

    const eraserHtml = renderToStaticMarkup(
      <DrawingLayer
        target="background"
        strokes={sampleStrokes}
        width={800}
        height={500}
        activeTool="eraser"
        currentConfig={{ color: "#000000", size: 4, opacity: 1 }}
        onAddStroke={() => {}}
        onEraseStroke={() => {}}
      />
    );
    expect(eraserHtml).toContain("pointer-events:all");
  });
});

describe("DrawPanel (Studio Enhancement Task 8)", () => {
  const defaultConfig: DrawConfig = {
    color: "#EF4444",
    size: 4,
    opacity: 1,
  };

  it("merender kontrol toggle alat: Kuas (Brush) dan Penghapus (Eraser)", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    expect(html).toContain("Kuas");
    expect(html).toContain("Penghapus");
  });

  it("merender pemilih target: Latar Belakang dan Plugin Terpilih", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    expect(html).toContain("Latar Belakang");
    expect(html).toContain("Plugin Terpilih");
  });

  it("merender tombol preset ukuran coretan (Halus 2px, Standar 4px, Tebal 8px, Penanda/Stabilo 18px)", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    expect(html).toContain("Halus");
    expect(html).toContain("2px");
    expect(html).toContain("Standar");
    expect(html).toContain("4px");
    expect(html).toContain("Tebal");
    expect(html).toContain("8px");
    expect(html).toContain("Stabilo");
    expect(html).toContain("18px");
  });

  it("merender palet warna: tema primer, swatch standar, dan input warna kustom", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    // Cek swatch warna standar
    const expectedSwatches = [
      "#4F46E5", // tema primer
      "#000000",
      "#FFFFFF",
      "#EF4444",
      "#F59E0B",
      "#10B981",
      "#3B82F6",
      "#8B5CF6",
    ];
    for (const swatch of expectedSwatches) {
      expect(html).toContain(swatch);
    }

    // Input warna kustom
    expect(html).toContain('type="color"');
  });

  it("merender mode opasitas: Padat (100%) dan Stabilo/Highlighter (40%)", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    expect(html).toContain("Padat");
    expect(html).toContain("100%");
    expect(html).toContain("Stabilo");
    expect(html).toContain("40%");
  });

  it("merender tombol Bersihkan Coretan", () => {
    const html = renderToStaticMarkup(
      <DrawPanel
        activeTool="brush"
        onSelectTool={() => {}}
        target="background"
        onSelectTarget={() => {}}
        config={defaultConfig}
        onChangeConfig={() => {}}
        primaryColor="#4F46E5"
        onClearStrokes={() => {}}
      />
    );

    expect(html).toContain("Bersihkan Coretan");
  });
});

