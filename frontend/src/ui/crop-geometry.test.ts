import { describe, expect, it } from "vitest";

import { MIN_CROP_SIZE, moveCrop, resizeCrop } from "./crop-geometry";

const bounds = { x: 20, y: 20, w: 400, h: 300 };
const start = { x: 100, y: 100, w: 100, h: 80 };

describe("crop-geometry", () => {
  it("geser tetap di dalam gambar", () => {
    expect(moveCrop(start, 10, 5, bounds)).toEqual({ x: 110, y: 105, w: 100, h: 80 });
    expect(moveCrop(start, -500, -500, bounds)).toEqual({ x: 20, y: 20, w: 100, h: 80 });
    expect(moveCrop(start, 900, 900, bounds)).toEqual({ x: 320, y: 240, w: 100, h: 80 });
  });

  it("ubah ukuran bebas dari sudut kanan bawah, dibatasi tepi gambar", () => {
    expect(resizeCrop("br", start, 50, 20, bounds, null)).toEqual({ x: 100, y: 100, w: 150, h: 100 });
    const r = resizeCrop("br", start, 900, 0, bounds, null);
    expect(r.x + r.w).toBe(bounds.x + bounds.w);
  });

  it("sudut kiri atas menjaga sudut kanan bawah tetap", () => {
    const r = resizeCrop("tl", start, -30, -20, bounds, null);
    expect(r.x + r.w).toBe(start.x + start.w);
    expect(r.y + r.h).toBe(start.y + start.h);
  });

  it("rasio terkunci dipertahankan", () => {
    const r = resizeCrop("br", start, 60, 0, bounds, 16 / 9);
    expect(r.w / r.h).toBeCloseTo(16 / 9, 5);
    const k = resizeCrop("tr", start, 40, 0, bounds, 1);
    expect(k.w).toBeCloseTo(k.h, 5);
  });

  it("tidak pernah lebih kecil dari ukuran minimum", () => {
    const r = resizeCrop("br", start, -500, -500, bounds, null);
    expect(r.w).toBeGreaterThanOrEqual(MIN_CROP_SIZE);
    expect(r.h).toBeGreaterThanOrEqual(MIN_CROP_SIZE);
  });
});
