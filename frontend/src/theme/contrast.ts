// Verifikasi kontras: memastikan token situs & dasbor memenuhi WCAG AA di semua kombinasi
// tema yang mungkin muncul pada halaman publik.
//
// Skenario yang diuji:
//  1. Latar putih  + teks terang  → kontras RENDAH (bug lama: mode gelap OS di atas toko putih).
//  2. Latar putih  + teks gelap   → kontras TINGGI (harus).
//  3. Latar gelap  + teks terang  → kontras TINGGI (harus).
//  4. Latar gelap  + teks gelap   → kontras RENDAH (harus pakai token lain).

function parseHex(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const s = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return [
    parseInt(s.slice(0, 2), 16),
    parseInt(s.slice(2, 4), 16),
    parseInt(s.slice(4, 6), 16),
  ];
}

function luminance(hex: string): number {
  const [r, g, b] = parseHex(hex);
  const lin = (v: number) => {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Rasio kontras WCAG (1..21). */
export function contrast(fg: string, bg: string): number {
  const a = luminance(fg);
  const b = luminance(bg);
  const [hi, lo] = a > b ? [a, b] : [b, a];
  return (hi + 0.05) / (lo + 0.05);
}

export function hexToRgb(hex: string): string {
  const [r, g, b] = parseHex(hex);
  return `rgb(${r}, ${g}, ${b})`;
}
