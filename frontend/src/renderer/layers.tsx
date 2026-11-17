// Plugin Latar: lapisan "tembus" di grid Studio v2. Selalu di BELAKANG blok lain (tidak pernah menimpa
// plugin), boleh menumpuk blok mana pun, dan bebas diseret/diubah ukurannya di editor.
//
// Desktop/tablet: menempati area grid (x, y, w, h) seperti blok biasa, tetapi z-index di bawah blok.
// Ponsel: grid menjadi satu kolom; latar membentang di belakang blok-blok yang ditumpuknya di desktop
// (baris ponsel pertama s.d. terakhir dari blok tersebut). Latar yang tidak menumpuk blok mana pun
// mendapat barisnya sendiri sesuai urutan atas→bawah.

import type { CSSProperties } from "react";

import { t as tx } from "@/i18n";

import { safeHref } from "./safe";

export interface LayerBlock {
  id: string; kind: string; x: number; y: number; w: number; h: number;
  hide_on?: "mobile" | "desktop";
  /** Gaya blok: `plain`/`flat` = sudut tegas, lainnya = sudut membulat. */
  style?: string;
  data: { [k: string]: unknown };
}

export const isLayer = (b: { kind: string }) => b.kind === "background";

const POSITIONS = ["center", "top", "bottom", "left", "right"];

/** Gambar latar → URL aman (`asset:` lewat assetUrl; selain itu hanya https/relatif yang lolos safeHref). */
export function layerImage(b: LayerBlock, assetUrl: (ref: string) => string | undefined): string | undefined {
  const ref = typeof b.data.image === "string" ? b.data.image.trim() : "";
  if (!ref) return undefined;
  return ref.startsWith("asset:") ? assetUrl(ref) : safeHref(ref) ?? undefined;
}

/** Isi blok Latar. `crop` = selalu memenuhi kotak (sisi lebih dipotong saat ukuran layar berubah);
 *  `stretch` = gambar utuh ikut melar/menyusut mengikuti kotak. */
export function BackgroundLayer({ b, assetUrl, editing }: { b: LayerBlock; assetUrl: (ref: string) => string | undefined; editing?: boolean }) {
  const url = layerImage(b, assetUrl);
  const fit = b.data.fit === "stretch" ? "stretch" : "crop";
  const pos = typeof b.data.position === "string" && POSITIONS.includes(b.data.position) ? b.data.position : "center";
  const op = typeof b.data.opacity === "number" && Number.isFinite(b.data.opacity) ? Math.min(100, Math.max(0, b.data.opacity)) : 100;
  if (!url) {
    return editing
      ? <div className="sv2-bg sv2-bg-empty" data-fit={fit}><span>{tx("Seret foto ke sini untuk latar")}</span></div>
      : null;
  }
  const style: CSSProperties = {
    backgroundImage: `url("${url.replace(/["\\]/g, "")}")`,
    backgroundSize: fit === "stretch" ? "100% 100%" : "cover",
    backgroundPosition: pos,
    opacity: op / 100,
  };
  const round = b.style !== undefined && b.style !== "plain" && b.style !== "flat";
  return <div className="sv2-bg" data-fit={fit} data-round={round || undefined} role="presentation" style={style} />;
}

/** `grid-row` tiap blok di tampilan ponsel (satu kolom). Kunci = id blok. */
export function mobileRows(blocks: LayerBlock[]): Map<string, string> {
  const visible = blocks.filter((b) => b.hide_on !== "mobile");
  const items = visible.filter((b) => !isLayer(b));
  const overlaps = (a: LayerBlock, b: LayerBlock) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
  const covering = new Map<string, LayerBlock[]>();
  const seq: LayerBlock[] = [...items];
  for (const l of visible.filter(isLayer)) {
    const under = items.filter((b) => overlaps(l, b));
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
