// Pertahanan berlapis di klien. Server (Rust) adalah penjaga utama (ADR-0007); fungsi ini
// memastikan renderer tetap aman bila suatu saat menerima data yang tidak tervalidasi.

const HEX = /^#[0-9a-fA-F]{6}$/;
const KEY = /^[a-z0-9_-]{1,40}$/;

export function safeColor(c: unknown, fallback = "#2563EB"): string {
  return typeof c === "string" && HEX.test(c) ? c : fallback;
}

/** Luminans relatif WCAG 2.x dari `#RRGGBB`. */
function luminance(hex: string): number {
  const ch = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * ch[0]! + 0.7152 * ch[1]! + 0.0722 * ch[2]!;
}

/** Rasio kontras WCAG antara dua warna `#RRGGBB`. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
}

/**
 * Warna teks di atas warna utama toko (P9-6, WCAG 1.4.3): putih atau hampir-hitam, mana yang kontrasnya lebih
 * tinggi. Pemilik toko bebas memilih warna; tombol tetap terbaca untuk warna terang seperti lavender/kuning.
 */
export function onColor(bg: string): "#ffffff" | "#111111" {
  const c = safeColor(bg);
  return contrast(c, "#ffffff") >= contrast(c, "#111111") ? "#ffffff" : "#111111";
}

/** Variabel CSS warna utama + pasangan teksnya (dipakai Site, StudioSite, editor). */
export function primaryVars(primary: unknown): Record<string, string> {
  const p = safeColor(primary);
  const on = onColor(p);
  return { "--s-primary": p, "--s-on-primary": on, "--s-shade": on === "#ffffff" ? "#000000" : "#ffffff" };
}

/** Sama dengan `is_safe_href` di crates/domain/src/website/schema.rs. */
export function safeHref(s: unknown): string | undefined {
  if (typeof s !== "string" || s.length === 0 || s.length > 500) return undefined;
  if ([...s].some((ch) => ch <= "\u001f" || ch === "\u007f" || ch === " ")) return undefined;
  const lower = s.toLowerCase();
  for (const scheme of ["https://", "http://"]) {
    if (lower.startsWith(scheme)) {
      const host = lower.slice(scheme.length).split(/[/?#]/)[0] ?? "";
      return host && !host.includes("@") && !host.includes("\\") ? s : undefined;
    }
  }
  if (lower.startsWith("mailto:")) return s.length > 7 && s.includes("@") ? s : undefined;
  if (lower.startsWith("tel:")) return /^[0-9+()-]+$/.test(s.slice(4)) ? s : undefined;
  if (s.startsWith("#")) return KEY.test(s.slice(1)) ? s : undefined;
  return s.startsWith("/") && !s.startsWith("//") && !s.includes("\\") ? s : undefined;
}

export const FONT_STACKS: Record<string, string> = {
  inter: "Inter, ui-sans-serif, system-ui, sans-serif",
  poppins: "Poppins, ui-sans-serif, system-ui, sans-serif",
  nunito: "Nunito, ui-rounded, ui-sans-serif, system-ui, sans-serif",
  merriweather: "Merriweather, Georgia, ui-serif, serif",
  system: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
};

export function fontStack(f: unknown): string {
  return (typeof f === "string" && FONT_STACKS[f]) || FONT_STACKS.system!;
}

export function text(v: unknown): string {
  return typeof v === "string" ? v : "";
}

/** Nomor WhatsApp -> tautan wa.me (hanya digit). */
export function waLink(n: string | null | undefined): string | undefined {
  if (!n) return undefined;
  let d = n.replace(/[^0-9]/g, "");
  if (d.startsWith("0")) d = "62" + d.slice(1);
  return d.length >= 8 ? `https://wa.me/${d}` : undefined;
}

/** Lebar varian yang disediakan server (`/files/{id}?…&w=`; sama dengan image_variant::WIDTHS). */
export const IMAGE_WIDTHS = [320, 640, 960, 1280] as const;

/** `srcset` untuk URL file bertanda tangan; URL lain (blob, data, eksternal) → tanpa srcset. */
export function srcSet(url: string | undefined): string | undefined {
  if (!url || !/\/files\/[0-9a-f-]{36}\?/i.test(url)) return undefined;
  return IMAGE_WIDTHS.map((w) => `${url}&w=${w} ${w}w`).join(", ");
}

/** `sizes`: di desktop gambar menempati `px` (lebar slot × porsi), di ponsel selebar layar × `mobile`. */
export const sizesFor = (px: number, mobile = 1) => `(min-width: 720px) ${Math.max(160, Math.round(px))}px, ${Math.round(mobile * 100)}vw`;

