// Mesin tata letak grid 12 kolom (prototipe ADR-0017). Murni & deterministik — diuji di grid.test.ts.
// Satuan: kolom (x, w) dan baris (y, h). Aturan: tidak ada tumpang tindih, item "jatuh" ke atas
// (vertical compaction) seperti react-grid-layout, sehingga tidak ada celah kosong yang aneh.

export const COLS = 12;

/** `locked`: posisi & ukuran dikunci — tidak pernah digeser oleh item lain; item lain mengalir di sekitarnya.
 *  `layer`: lapisan "tembus" (plugin Latar) — di belakang blok lain, tidak bertabrakan, tidak dirapatkan, dan
 *  tidak pernah mendorong atau didorong; tetap persis di posisi yang dipilih pengguna. */
export interface Cell { id: string; x: number; y: number; w: number; h: number; minW?: number; minH?: number; locked?: boolean; layer?: boolean }

export function clamp(c: Cell): Cell {
  const minW = c.minW ?? 2;
  const minH = c.minH ?? 2;
  const w = Math.max(minW, Math.min(COLS, Math.round(c.w)));
  const h = Math.max(minH, Math.round(c.h));
  const x = Math.max(0, Math.min(COLS - w, Math.round(c.x)));
  const y = Math.max(0, Math.round(c.y));
  return { ...c, x, y, w, h };
}

export function collides(a: Cell, b: Cell): boolean {
  return a.id !== b.id && !a.layer && !b.layer && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

const byPos = (a: Cell, b: Cell) => a.y - b.y || a.x - b.x || a.id.localeCompare(b.id);

/** Naikkan setiap item setinggi mungkin tanpa bertabrakan. `pinned` tidak dipindah. */
export function compact(cells: Cell[], pinned?: string): Cell[] {
  // Item terkunci & item yang sedang dipindah adalah penghalang tetap.
  const placed: Cell[] = cells.filter((c) => c.locked || c.layer || c.id === pinned);
  for (const c of [...cells].filter((c) => !c.locked && !c.layer && c.id !== pinned).sort(byPos)) {
    let next = { ...c, y: 0 };
    // Turunkan sampai tidak bertabrakan dengan item yang sudah diletakkan.
    for (;;) {
      const hit = placed.find((p) => collides(next, p));
      if (!hit) break;
      next = { ...next, y: hit.y + hit.h };
    }
    placed.push(next);
  }
  return cells.map((c) => placed.find((p) => p.id === c.id)!);
}

/** Pindah/ubah ukuran satu item lalu rapikan sisanya. Item yang digeser tetap di posisi pilihan pengguna. */
export function place(cells: Cell[], next: Cell): Cell[] {
  const prev = cells.find((c) => c.id === next.id);
  if (prev?.locked) return cells;
  let moved = clamp(next);
  // Lapisan bebas ditaruh di mana saja (menumpuk blok lain); susunan blok lain tidak berubah.
  if (moved.layer) return prev ? cells.map((c) => (c.id === moved.id ? moved : c)) : [...cells, moved];
  const others = cells.filter((c) => c.id !== moved.id);
  // Tidak boleh menimpa item terkunci: turunkan sampai lewat di bawahnya.
  for (;;) {
    const lock = others.find((c) => c.locked && collides(c, moved));
    if (!lock) break;
    moved = { ...moved, y: lock.y + lock.h };
  }
  // Dorong item (yang tidak terkunci) yang tertabrak ke bawah item yang dipindah, lalu rapatkan ke atas.
  const pushed = others.map((c) => (!c.locked && collides(c, moved) ? { ...c, y: moved.y + moved.h } : c));
  const all = [...pushed, moved];
  const out = compact(all, moved.id);
  return cells.some((c) => c.id === moved.id) ? cells.map((c) => out.find((o) => o.id === c.id)!) : out;
}

/** Kolom dua item beririsan (untuk mencari tetangga atas/bawah). */
const sharesColumns = (a: Cell, b: Cell) => a.x < b.x + b.w && b.x < a.x + a.w;

/**
 * Pindah tanpa menyeret (WCAG 2.5.7, P9-6): kiri/kanan satu kolom; atas/bawah bertukar dengan tetangga
 * terdekat yang berbagi kolom. Item terkunci tidak bergerak dan tidak ikut ditukar — tetangga terkunci
 * dilewati, sehingga item bergerak menukar dengan blok tidak-terkunci berikutnya.
 */
export function moveBy(cells: Cell[], id: string, dir: "up" | "down" | "left" | "right"): Cell[] {
  const c = cells.find((x) => x.id === id);
  if (!c || c.locked) return cells;
  if (c.layer) return place(cells, { ...c, x: c.x + (dir === "left" ? -1 : dir === "right" ? 1 : 0), y: c.y + (dir === "up" ? -1 : dir === "down" ? 1 : 0) });
  if (dir === "left" || dir === "right") return place(cells, { ...c, x: c.x + (dir === "left" ? -1 : 1) });
  const others = cells.filter((x) => x.id !== id && !x.layer && !x.locked && sharesColumns(x, c));
  if (dir === "up") {
    const above = others.filter((x) => x.y + x.h <= c.y).sort((a, b) => b.y + b.h - (a.y + a.h))[0];
    if (!above) return cells;
    return place(cells, { ...c, y: above.y });
  }
  const below = others.filter((x) => x.y >= c.y + c.h).sort((a, b) => a.y - b.y)[0];
  if (!below) return cells;
  return place(cells, { ...below, y: c.y });
}

/** Tambah item baru di posisi tertentu (atau paling bawah bila `at` kosong). */
export function insert(cells: Cell[], cell: Cell, at?: { x: number; y: number }): Cell[] {
  const bottom = cells.reduce((m, c) => Math.max(m, c.y + c.h), 0);
  // Lapisan baru tanpa posisi: tutupi seluruh halaman dari atas (latar halaman penuh).
  if (cell.layer && !at) return place(cells, { ...cell, x: 0, y: 0, w: COLS, h: Math.max(cell.h, bottom) });
  return place([...cells, clamp({ ...cell, ...(at ?? { x: 0, y: bottom }) })], clamp({ ...cell, ...(at ?? { x: 0, y: bottom }) }));
}

/** Urutan tampil di ponsel: atas→bawah, kiri→kanan. */
export function mobileOrder<T extends Cell>(cells: T[]): T[] {
  return [...cells].sort(byPos);
}

export function height(cells: Cell[]): number {
  return cells.reduce((m, c) => Math.max(m, c.y + c.h), 0);
}

/* ---------------------------------------------------------------- ukuran desain (px) */
/** Lebar ACUAN grid untuk satuan koordinat coretan (bukan lebar tampil: `.sv2-grid` = --sv2-max; coretan berskala lewat viewBox). */
export const DESIGN_WIDTH = 1080;
export const GAP = 12;
export const ROW = 24;
/** Lebar satu kolom pada lebar desain. */
export const COL_W = (DESIGN_WIDTH - GAP * (COLS - 1)) / COLS;
/** Lebar `w` kolom (termasuk celah di antaranya) pada lebar desain. */
export const spanWidth = (w: number) => w * COL_W + (w - 1) * GAP;
/** px tinggi isi → jumlah baris grid. */
export const rowsFor = (px: number) => Math.max(2, Math.ceil((px + GAP) / (ROW + GAP)));

/* ---------------------------------------------------------------- urutan ponsel */
/** Pindahkan blok `id` ke posisi `to` di urutan ponsel; semua blok grid (bukan lapisan) diberi `mOrder`
 *  berurutan sehingga susunan ponsel tersimpan utuh. Susunan desktop (x/y) tidak berubah. */
export function reorderMobile<T extends Cell & { mOrder?: number }>(cells: T[], id: string, to: number): T[] {
  const natural = [...cells].filter((c) => !c.layer).sort(byPos);
  const order = natural.sort((a, b) => (a.mOrder ?? 1000 + natural.indexOf(a)) - (b.mOrder ?? 1000 + natural.indexOf(b)));
  const from = order.findIndex((c) => c.id === id);
  if (from < 0) return cells;
  const next = [...order];
  const [moved] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(next.length, to)), 0, moved!);
  const rank = new Map(next.map((c, i) => [c.id, i]));
  return cells.map((c) => (rank.has(c.id) ? { ...c, mOrder: rank.get(c.id) } : c));
}

/** Posisi blok di urutan ponsel saat ini. */
export function mobileIndex<T extends Cell & { mOrder?: number }>(cells: T[], id: string): number {
  const natural = [...cells].filter((c) => !c.layer).sort(byPos);
  return natural.sort((a, b) => (a.mOrder ?? 1000 + natural.indexOf(a)) - (b.mOrder ?? 1000 + natural.indexOf(b))).findIndex((c) => c.id === id);
}
