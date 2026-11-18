// Plugin Latar: lapisan "tembus" di grid Studio v2. Selalu di BELAKANG blok lain (tidak pernah menimpa
// plugin), boleh menumpuk blok mana pun, dan bebas diseret/diubah ukurannya di editor.
//
// Desktop/tablet: menempati area grid (x, y, w, h) seperti blok biasa, tetapi z-index di bawah blok.
// Ponsel: grid menjadi satu kolom; latar membentang di belakang blok-blok yang ditumpuknya di desktop
// (baris ponsel pertama s.d. terakhir dari blok tersebut). Latar yang tidak menumpuk blok mana pun
// mendapat barisnya sendiri sesuai urutan atas→bawah.

import type { CSSProperties } from "react";

import { t as tx } from "@/i18n";

import { onImgError, safeHref } from "./safe";

export interface LayerBlock {
  id: string; kind: string; x: number; y: number; w: number; h: number; layout?: string;
  hide_on?: "mobile" | "desktop";
  /** Gaya blok: `plain`/`flat` = sudut tegas, lainnya = sudut membulat. */
  style?: string;
  data: { [k: string]: unknown };
}

/** Lapisan bebas di grid (boleh menumpuk blok). Gambar popup bukan lapisan grid — ia menempel di layar. */
export const isLayer = (b: { kind: string; layout?: string }) => b.kind === "background" || (b.kind === "image" && b.layout !== "popup");
/** Lapisan yang dirender DI BELAKANG blok (Latar, Gambar "di belakang blok"). Sisanya di depan. */
export const isBehind = (b: { kind: string; layout?: string }) => b.kind === "background" || (b.kind === "image" && b.layout === "below");
export const isPopupImage = (b: { kind: string; layout?: string }) => b.kind === "image" && b.layout === "popup";

const POSITIONS = ["center", "top", "bottom", "left", "right"];

/** Gambar latar → URL aman (`asset:` lewat assetUrl; selain itu hanya https/relatif yang lolos safeHref). */
/** Latar tampil di situs publik bila punya gambar atau warna pita. */
export const layerVisible = (b: LayerBlock, assetUrl: (ref: string) => string | undefined) =>
  !!layerImage(b, assetUrl) || (b.kind === "background" && !!bandColor(b.data.color));

export function layerImage(b: LayerBlock, assetUrl: (ref: string) => string | undefined): string | undefined {
  const ref = typeof b.data.image === "string" ? b.data.image.trim() : "";
  if (!ref) return undefined;
  return ref.startsWith("asset:") ? assetUrl(ref) : safeHref(ref) ?? undefined;
}

const EDGE_H = 48;
/** Pemisah tepi (topeng SVG hitam): bagian terisi di atas garis bentuk. `top` = dibalik vertikal. */
const EDGE_PATH: Record<string, [string, string]> = {
  wave: ["M0 0H1200V24C1000 52 800-4 600 24S200 52 0 24Z", "M0 48H1200V24C1000-4 800 52 600 24S200-4 0 24Z"],
  curve: ["M0 0H1200V6Q600 90 0 6Z", "M0 48H1200V42Q600-42 0 42Z"],
  slant: ["M0 0H1200V6L0 48Z", "M0 48H1200V0L0 42Z"],
  zigzag: ["M0 0H1200V24L1150 48 1100 24 1050 48 1000 24 950 48 900 24 850 48 800 24 750 48 700 24 650 48 600 24 550 48 500 24 450 48 400 24 350 48 300 24 250 48 200 24 150 48 100 24 50 48 0 24Z",
    "M0 48H1200V24L1150 0 1100 24 1050 0 1000 24 950 0 900 24 850 0 800 24 750 0 700 24 650 0 600 24 550 0 500 24 450 0 400 24 350 0 300 24 250 0 200 24 150 0 100 24 50 0 0 24Z"],
};
const edgeSvg = (d: string) => `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1200 48' preserveAspectRatio='none'><path d='${d}'/></svg>`)}")`;

/** Topeng CSS untuk tepi atas/bawah berbentuk (gelombang, lengkung, miring, gerigi). */
export function edgeMask(top: unknown, bottom: unknown): CSSProperties {
  const t = typeof top === "string" ? EDGE_PATH[top]?.[1] : undefined;
  const b = typeof bottom === "string" ? EDGE_PATH[bottom]?.[0] : undefined;
  if (!t && !b) return {};
  const th = t ? EDGE_H : 0;
  const bh = b ? EDGE_H : 0;
  const layers = [
    { img: "linear-gradient(#000,#000)", size: `100% calc(100% - ${th + bh}px)`, pos: `0 ${th}px` },
    ...(t ? [{ img: edgeSvg(t), size: `100% ${EDGE_H}px`, pos: "0 0" }] : []),
    ...(b ? [{ img: edgeSvg(b), size: `100% ${EDGE_H}px`, pos: "0 100%" }] : []),
  ];
  const v = (k: "img" | "size" | "pos") => layers.map((l) => l[k]).join(", ");
  return { maskImage: v("img"), WebkitMaskImage: v("img"), maskSize: v("size"), WebkitMaskSize: v("size"), maskPosition: v("pos"), WebkitMaskPosition: v("pos"), maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat" };
}

/** Warna pita Latar: `primary`/`accent` mengikuti brand kit, selain itu heks. */
function bandColor(c: unknown): string | undefined {
  if (c === "primary") return "var(--s-primary)";
  if (c === "accent") return "var(--s-accent, var(--s-primary))";
  return typeof c === "string" && /^#[0-9a-fA-F]{6}$/.test(c) ? c : undefined;
}

/** Isi blok Latar. `crop` = selalu memenuhi kotak (sisi lebih dipotong saat ukuran layar berubah);
 *  `stretch` = gambar utuh ikut melar/menyusut mengikuti kotak. */
export function BackgroundLayer({ b, assetUrl, editing }: { b: LayerBlock; assetUrl: (ref: string) => string | undefined; editing?: boolean }) {
  const url = layerImage(b, assetUrl);
  const color = bandColor(b.data.color);
  // Selebar layar: tepi kiri/kanan yang menyentuh tepi grid diperpanjang sampai tepi halaman.
  const bleed = b.data.bleed === true ? `${b.x === 0 ? "l" : ""}${b.x + b.w === 12 ? "r" : ""}` || undefined : undefined;
  const mask = edgeMask(b.data.edge_top, b.data.edge_bottom);
  const fit = b.data.fit === "stretch" ? "stretch" : "crop";
  const pos = typeof b.data.position === "string" && POSITIONS.includes(b.data.position) ? b.data.position : "center";
  const op = typeof b.data.opacity === "number" && Number.isFinite(b.data.opacity) ? Math.min(100, Math.max(0, b.data.opacity)) : 100;
  if (!url && !color) {
    return editing
      ? <div className="sv2-bg sv2-bg-empty" data-fit={fit}><span>{tx("Seret foto ke sini untuk latar, atau pilih warna pita")}</span></div>
      : null;
  }
  const style: CSSProperties = {
    ...mask,
    backgroundColor: color,
    backgroundImage: url ? `url("${url.replace(/["\\]/g, "")}")` : undefined,
    backgroundSize: fit === "stretch" ? "100% 100%" : "cover",
    backgroundPosition: pos,
    opacity: op / 100,
  };
  const round = b.style !== undefined && b.style !== "plain" && b.style !== "flat";
  return <div className="sv2-bg" data-fit={fit} data-round={round && !bleed ? true : undefined} data-bleed={bleed} role="presentation" style={style} />;
}

/** Gambar bebas. `<img>` (bukan latar CSS) supaya PNG/WebP transparan, teks alternatif, dan tautan berfungsi.
 *  `contain` = utuh (stiker/PNG transparan), `crop` = penuhi kotak, `stretch` = ikut melar. */
export function FreeImage({ b, assetUrl, editing }: { b: LayerBlock; assetUrl: (ref: string) => string | undefined; editing?: boolean }) {
  const url = layerImage(b, assetUrl);
  if (!url) {
    return editing
      ? <div className="sv2-img sv2-bg-empty" data-mode={b.layout ?? "above"}><span>{tx("Seret foto (PNG transparan juga bisa) ke sini")}</span></div>
      : null;
  }
  const fit = b.data.fit === "crop" ? "cover" : b.data.fit === "stretch" ? "fill" : "contain";
  const pos = typeof b.data.position === "string" && POSITIONS.includes(b.data.position) ? b.data.position : "center";
  const op = typeof b.data.opacity === "number" && Number.isFinite(b.data.opacity) ? Math.min(100, Math.max(0, b.data.opacity)) : 100;
  const alt = typeof b.data.alt === "string" ? b.data.alt : "";
  const round = b.style !== undefined && b.style !== "plain" && b.style !== "flat";
  const shape = typeof b.data.shape === "string" && b.data.shape !== "none" ? b.data.shape : undefined;
  const filter = typeof b.data.filter === "string" && b.data.filter !== "none" ? b.data.filter : undefined;
  // eslint-disable-next-line @next/next/no-img-element -- aset tenant bertanda tangan; ukuran bebas
  const img = <img src={url} alt={alt} loading="lazy" decoding="async" draggable={false} style={{ objectFit: fit, objectPosition: pos, opacity: op / 100 }} data-round={round && !shape ? true : undefined} onError={onImgError} />;
  const href = editing ? undefined : safeHref(b.data.href);
  return (
    <div className="sv2-img" data-mode={b.layout ?? "above"} data-shape={shape} data-filter={filter}>
      {href ? <a href={href} className="sv2-img-link" aria-label={alt || tx("Buka tautan gambar")}>{img}</a> : img}
    </div>
  );
}

/** `grid-row` tiap blok di tampilan ponsel (satu kolom). Kunci = id blok. */
export function mobileRows(blocks: LayerBlock[]): Map<string, string> {
  const visible = blocks.filter((b) => b.hide_on !== "mobile");
  const items = visible.filter((b) => !isLayer(b));
  const overlaps = (a: LayerBlock, b: LayerBlock) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
  const covering = new Map<string, LayerBlock[]>();
  const seq: LayerBlock[] = [...items];
  for (const l of visible.filter(isLayer)) {
    // Hanya lapisan di BELAKANG yang membentang di belakang blok di ponsel. Gambar di depan/menggantung
    // mendapat barisnya sendiri supaya tidak menutupi isi di layar sempit.
    const under = isBehind(l) ? items.filter((b) => overlaps(l, b)) : [];
    if (under.length) covering.set(l.id, under);
    else seq.push(l);
  }
  seq.sort((a, b) => a.y - b.y || a.x - b.x || a.id.localeCompare(b.id));
  const out = new Map<string, string>();
  seq.forEach((b, i) => out.set(b.id, String(i + 1)));
  for (const [id, under] of covering) {
    const rows = under.map((b) => Number(out.get(b.id)));
    out.set(id, `${Math.min(...rows)} / ${Math.max(...rows) + 1}`);
  }
  return out;
}
