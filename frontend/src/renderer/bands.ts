// Blok berdampingan yang mulai di baris yang sama dibuat sama tinggi. Tanpa ini kartu pendek
// (mis. "Area layanan") berdiri di samping kartu tinggi ("Jam layanan") dan menyisakan ruang kosong
// besar di bawahnya. Caranya: rentang baris blok auto-tinggi diperpanjang sampai baris akhir tetangga
// tertinggi — hanya bila area tambahan itu kosong (tidak pernah menimpa blok lain). Murni tampilan;
// dokumen (y/h) tidak diubah.

export interface BandCell { id: string; x: number; y: number; w: number; h: number; auto_height?: boolean }

export function bandSpans(cells: BandCell[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const b of cells) {
    if (b.auto_height === false) continue;
    const end = b.y + b.h;
    const target = Math.max(...cells.filter((c) => c.y === b.y && c.auto_height !== false).map((c) => c.y + c.h));
    if (target <= end) continue;
    const blocked = cells.some((c) => c.id !== b.id
      && c.x < b.x + b.w && b.x < c.x + c.w // kolom beririsan
      && c.y < target && end < c.y + c.h); // baris beririsan dengan area tambahan [end, target)
    if (!blocked) out.set(b.id, target - b.y);
  }
  return out;
}
