// Pertahanan berlapis di klien. Server (Rust) adalah penjaga utama (ADR-0007); fungsi ini
// memastikan renderer tetap aman bila suatu saat menerima data yang tidak tervalidasi.

const HEX = /^#[0-9a-fA-F]{6}$/;
const KEY = /^[a-z0-9_-]{1,40}$/;

export function safeColor(c: unknown, fallback = "#2563EB"): string {
  return typeof c === "string" && HEX.test(c) ? c : fallback;
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
