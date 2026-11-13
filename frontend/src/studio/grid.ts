// Mesin tata letak grid 12 kolom (prototipe ADR-0017). Murni & deterministik — diuji di grid.test.ts.
// Satuan: kolom (x, w) dan baris (y, h). Aturan: tidak ada tumpang tindih, item "jatuh" ke atas
// (vertical compaction) seperti react-grid-layout, sehingga tidak ada celah kosong yang aneh.

export const COLS = 12;

export interface Cell { id: string; x: number; y: number; w: number; h: number; minW?: number; minH?: number }

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
  return a.id !== b.id && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

const byPos = (a: Cell, b: Cell) => a.y - b.y || a.x - b.x || a.id.localeCompare(b.id);

/** Naikkan setiap item setinggi mungkin tanpa bertabrakan. `pinned` tidak dipindah. */
export function compact(cells: Cell[], pinned?: string): Cell[] {
  const placed: Cell[] = [];
  const pin = cells.find((c) => c.id === pinned);
  if (pin) placed.push(pin);
  for (const c of [...cells].filter((c) => c.id !== pinned).sort(byPos)) {
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
  const moved = clamp(next);
  const others = cells.filter((c) => c.id !== moved.id);
  // Dorong item yang tertabrak ke bawah item yang dipindah, lalu rapatkan ke atas.
  const pushed = others.map((c) => (collides(c, moved) ? { ...c, y: moved.y + moved.h } : c));
  const all = [...pushed, moved];
  const out = compact(all, moved.id);
  return cells.some((c) => c.id === moved.id) ? cells.map((c) => out.find((o) => o.id === c.id)!) : out;
}

/** Tambah item baru di posisi tertentu (atau paling bawah bila `at` kosong). */
export function insert(cells: Cell[], cell: Cell, at?: { x: number; y: number }): Cell[] {
  const bottom = cells.reduce((m, c) => Math.max(m, c.y + c.h), 0);
  return place([...cells, clamp({ ...cell, ...(at ?? { x: 0, y: bottom }) })], clamp({ ...cell, ...(at ?? { x: 0, y: bottom }) }));
}

/** Urutan tampil di ponsel: atas→bawah, kiri→kanan. */
export function mobileOrder<T extends Cell>(cells: T[]): T[] {
  return [...cells].sort(byPos);
}

export function height(cells: Cell[]): number {
  return cells.reduce((m, c) => Math.max(m, c.y + c.h), 0);
}
