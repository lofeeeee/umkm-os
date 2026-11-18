import { describe, expect, it } from "vitest";

import { clamp, collides, compact, height, insert, mobileIndex, mobileOrder, moveBy, place, reorderMobile, type Cell } from "./grid";

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

describe("moveBy — pindah tanpa menyeret (WCAG 2.5.7)", () => {
  const A = { id: "a", x: 0, y: 0, w: 12, h: 4 };
  const B = { id: "b", x: 0, y: 4, w: 12, h: 4 };
  const C = { id: "c", x: 0, y: 8, w: 6, h: 3 };
  const at = (cells: { id: string; x: number; y: number }[], id: string) => cells.find((c) => c.id === id)!;
  it("atas/bawah menukar dengan tetangga terdekat", () => {
    const up = moveBy([A, B, C], "b", "up");
    expect(at(up, "b").y).toBe(0);
    expect(at(up, "a").y).toBe(4);
    const down = moveBy([A, B, C], "a", "down");
    expect(at(down, "b").y).toBe(0);
    expect(at(down, "a").y).toBe(4);
  });
  it("kiri/kanan satu kolom, dibatasi tepi grid", () => {
    expect(at(moveBy([A, B, C], "c", "right"), "c").x).toBe(1);
    expect(at(moveBy([A, B, C], "c", "left"), "c").x).toBe(0);
    expect(at(moveBy([A, B, C], "a", "right"), "a").x).toBe(0);
  });
  it("tanpa tetangga atau terkunci: tidak berubah", () => {
    expect(moveBy([A, B], "a", "up")).toEqual([A, B]);
    const locked = [A, { ...B, locked: true }];
    expect(moveBy(locked, "b", "up")).toEqual(locked);
    // tetangga bawah terkunci: item melompat ke bawahnya, item terkunci tetap
    const r = moveBy([A, { ...B, locked: true }], "a", "down");
    expect(at(r, "b").y).toBe(4);
    expect(at(r, "a").y).toBe(8);
  });
});


describe("lapisan tembus (plugin Latar)", () => {
  const blocks = [
    { id: "a", x: 0, y: 0, w: 6, h: 4 },
    { id: "b", x: 6, y: 0, w: 6, h: 6 },
    { id: "c", x: 0, y: 6, w: 12, h: 4 },
  ];
  const bg = { id: "bg", x: 0, y: 0, w: 12, h: 3, layer: true };
  it("tidak bertabrakan, tidak mendorong, dan tidak dirapatkan", () => {
    const placed = place(blocks, { ...bg, y: 2 });
    expect(placed.find((c) => c.id === "bg")).toMatchObject({ x: 0, y: 2, w: 12, h: 3 });
    for (const b of blocks) expect(placed.find((c) => c.id === b.id)).toMatchObject(b);
    expect(compact([...blocks, { ...bg, y: 9 }]).find((c) => c.id === "bg")!.y).toBe(9);
  });
  it("blok lain yang dipindah tidak terhalang lapisan", () => {
    const out = place([...blocks, bg], { id: "c", x: 0, y: 0, w: 12, h: 4 });
    expect(out.find((c) => c.id === "bg")).toMatchObject({ x: 0, y: 0, h: 3 });
    expect(out.find((c) => c.id === "c")!.y).toBe(0);
  });
  it("tanpa posisi menutupi seluruh halaman; tombol pindah menggeser satu langkah", () => {
    const ins = insert(blocks, { ...bg, h: 2 });
    expect(ins.find((c) => c.id === "bg")).toMatchObject({ x: 0, y: 0, w: 12, h: 10 });
    const moved = moveBy([...blocks, { ...bg, w: 6 }], "bg", "down");
    expect(moved.find((c) => c.id === "bg")).toMatchObject({ x: 0, y: 1 });
    expect(moveBy([...blocks, { ...bg, w: 6 }], "bg", "right").find((c) => c.id === "bg")!.x).toBe(1);
  });
});

describe("urutan ponsel", () => {
  const cells = [
    { id: "a", x: 0, y: 0, w: 6, h: 4 },
    { id: "b", x: 6, y: 0, w: 6, h: 4 },
    { id: "c", x: 0, y: 4, w: 12, h: 4 },
    { id: "bg", x: 0, y: 0, w: 12, h: 8, layer: true },
  ];
  it("memindah blok di urutan ponsel tanpa mengubah x/y; lapisan tidak ikut diberi urutan", () => {
    const out = reorderMobile(cells, "c", 0);
    expect(out.map((c) => [c.id, c.mOrder])).toEqual([["a", 1], ["b", 2], ["c", 0], ["bg", undefined]]);
    for (const c of cells) expect(out.find((o) => o.id === c.id)).toMatchObject({ x: c.x, y: c.y });
    expect(mobileIndex(out, "c")).toBe(0);
    expect(mobileIndex(reorderMobile(out, "a", 2), "a")).toBe(2);
  });
});
