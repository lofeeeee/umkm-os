import { describe, expect, it } from "vitest";

import { clamp, collides, compact, height, insert, mobileOrder, place, type Cell } from "./grid";

const c = (id: string, x: number, y: number, w: number, h: number): Cell => ({ id, x, y, w, h });
const noOverlap = (cells: Cell[]) => cells.every((a) => cells.every((b) => !collides(a, b)));

describe("grid 12 kolom", () => {
  it("clamp menjaga item di dalam 12 kolom dan ukuran minimum", () => {
    expect(clamp(c("a", 11, -2, 4, 1))).toMatchObject({ x: 8, y: 0, w: 4, h: 2 });
    expect(clamp({ ...c("a", 0, 0, 1, 1), minW: 3 })).toMatchObject({ w: 3 });
    expect(clamp(c("a", 0, 0, 20, 3))).toMatchObject({ x: 0, w: 12 });
  });

  it("compact menaikkan item ke celah kosong", () => {
    const out = compact([c("a", 0, 5, 6, 2), c("b", 6, 9, 6, 2)]);
    expect(out).toEqual([c("a", 0, 0, 6, 2), c("b", 6, 0, 6, 2)]);
  });

  it("place mendorong item yang tertabrak dan tidak pernah tumpang tindih", () => {
    const start = [c("a", 0, 0, 12, 4), c("b", 0, 4, 6, 3), c("c", 6, 4, 6, 3)];
    const out = place(start, c("c", 0, 0, 6, 3));
    expect(noOverlap(out)).toBe(true);
    expect(out.find((x) => x.id === "c")).toMatchObject({ x: 0, y: 0 });
    expect(out.find((x) => x.id === "a")!.y).toBeGreaterThanOrEqual(3);
  });

  it("resize melebar menggeser tetangga ke bawah", () => {
    const out = place([c("a", 0, 0, 6, 3), c("b", 6, 0, 6, 3)], c("a", 0, 0, 12, 3));
    expect(noOverlap(out)).toBe(true);
    expect(out.find((x) => x.id === "b")).toMatchObject({ y: 3 });
  });

  it("insert tanpa posisi menaruh di paling bawah", () => {
    const out = insert([c("a", 0, 0, 12, 4)], c("n", 3, 0, 6, 2));
    expect(out.find((x) => x.id === "n")).toMatchObject({ y: 4, x: 0 });
    expect(height(out)).toBe(6);
  });

  it("urutan ponsel atas→bawah lalu kiri→kanan", () => {
    const order = mobileOrder([c("chat", 8, 0, 4, 4), c("banner", 0, 0, 8, 4), c("promo", 0, 4, 4, 3)]).map((x) => x.id);
    expect(order).toEqual(["banner", "chat", "promo"]);
  });

  it("acak: banyak perpindahan tetap tanpa tumpang tindih", () => {
    let cells: Cell[] = Array.from({ length: 8 }, (_, i) => c(`i${i}`, (i * 3) % 12, i * 2, 3 + (i % 4), 2 + (i % 3)));
    cells = compact(cells.map(clamp));
    let seed = 7;
    const rnd = (n: number) => { seed = (seed * 1103515245 + 12345) % 2 ** 31; return seed % n; };
    for (let k = 0; k < 300; k++) {
      const t = cells[rnd(cells.length)]!;
      cells = place(cells, { ...t, x: rnd(12), y: rnd(20), w: 2 + rnd(10), h: 2 + rnd(5) });
      expect(noOverlap(cells)).toBe(true);
    }
  });

  it("item terkunci tidak bergeser saat item lain dipindah, diperbesar, atau dirapatkan", () => {
    const lock = { ...c("L", 0, 4, 12, 3), locked: true };
    let cells: Cell[] = [c("a", 0, 0, 6, 4), c("b", 6, 0, 6, 4), lock, c("d", 0, 7, 12, 3)];
    cells = place(cells, c("a", 0, 0, 6, 10)); // a membesar ke bawah melewati L
    expect(cells.find((x) => x.id === "L")).toMatchObject({ x: 0, y: 4, w: 12, h: 3 });
    expect(noOverlap(cells)).toBe(true);
    cells = place(cells, c("b", 0, 5, 6, 4)); // b dijatuhkan di atas L → turun ke bawah L
    expect(cells.find((x) => x.id === "L")).toMatchObject({ y: 4 });
    expect(cells.find((x) => x.id === "b")!.y).toBeGreaterThanOrEqual(7);
    expect(noOverlap(cells)).toBe(true);
  });

  it("item terkunci tidak bisa dipindah lewat place()", () => {
    const cells: Cell[] = [{ ...c("L", 0, 0, 6, 3), locked: true }, c("a", 6, 0, 6, 3)];
    expect(place(cells, { ...cells[0]!, x: 6 })).toBe(cells);
  });

  it("acak dengan item terkunci: terkunci tetap di tempat, tanpa tumpang tindih", () => {
    const locks = [{ ...c("L1", 0, 6, 6, 3), locked: true }, { ...c("L2", 6, 14, 6, 4), locked: true }];
    let cells: Cell[] = compact([...locks, ...Array.from({ length: 6 }, (_, i) => c(`i${i}`, (i * 4) % 12, i * 3, 4, 3))]);
    let seed = 11;
    const rnd = (n: number) => { seed = (seed * 1103515245 + 12345) % 2 ** 31; return seed % n; };
    for (let k = 0; k < 300; k++) {
      const free = cells.filter((x) => !x.locked);
      const t = free[rnd(free.length)]!;
      cells = place(cells, { ...t, x: rnd(12), y: rnd(24), w: 2 + rnd(10), h: 2 + rnd(5) });
      expect(noOverlap(cells)).toBe(true);
      expect(cells.find((x) => x.id === "L1")).toMatchObject({ x: 0, y: 6, w: 6, h: 3 });
      expect(cells.find((x) => x.id === "L2")).toMatchObject({ x: 6, y: 14, w: 6, h: 4 });
    }
  });
});
