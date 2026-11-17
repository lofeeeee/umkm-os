import { describe, expect, it } from "vitest";

import { bandSpans } from "./bands";

describe("bandSpans", () => {
  it("memperpanjang blok pendek sampai tetangga tertinggi di baris yang sama", () => {
    const m = bandSpans([
      { id: "a", x: 0, y: 10, w: 6, h: 6 },
      { id: "b", x: 6, y: 10, w: 6, h: 9 },
    ]);
    expect(m.get("a")).toBe(9);
    expect(m.has("b")).toBe(false);
  });
  it("tidak menimpa blok lain di bawahnya", () => {
    const m = bandSpans([
      { id: "a", x: 0, y: 0, w: 6, h: 6 },
      { id: "b", x: 6, y: 0, w: 6, h: 9 },
      { id: "c", x: 0, y: 6, w: 6, h: 3 },
    ]);
    expect(m.has("a")).toBe(false);
  });
  it("blok bertinggi manual tidak diubah dan tidak menjadi patokan", () => {
    const m = bandSpans([
      { id: "a", x: 0, y: 0, w: 6, h: 4 },
      { id: "b", x: 6, y: 0, w: 6, h: 12, auto_height: false },
    ]);
    expect(m.size).toBe(0);
  });
  it("blok yang mulai di baris berbeda tidak disejajarkan", () => {
    expect(bandSpans([{ id: "a", x: 0, y: 0, w: 6, h: 4 }, { id: "b", x: 6, y: 2, w: 6, h: 9 }]).size).toBe(0);
  });
});
