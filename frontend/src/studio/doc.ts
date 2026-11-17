// Konversi keadaan Studio ↔ dokumen `schema_version = 2` (ADR-0017). Bentuk dokumen adalah kontrak
// dengan validator server (crates/domain/src/website/schema_v2.rs) — field di luar daftar ditolak server.

import type { Json } from "@/renderer/types";

import { type DrawingStroke } from "./DrawingLayer";
import manifest from "./plugin-manifest.json";
import { PALETTES } from "./plugins";
import { onColor } from "@/renderer/safe";
import type { CellStyle, PluginKind } from "./plugins";
import type { TplCell } from "./templates";

export interface StudioSeo { title?: string; description?: string; image?: string; favicon?: string }
export interface StudioState {
  primary: string;
  /** Warna teks di atas warna utama; dari palet atau diturunkan saat fromDoc (tidak masuk dokumen server). */
  onPrimary?: string;
  surface: "light" | "cream" | "dark";
  font: string;
  textScale?: number;
  navbarStyle?: "top" | "side";
  navbarSticky?: boolean;
  hideBadge?: boolean;
  backgroundImage?: string;
  backgroundSize?: "cover" | "contain" | "repeat";
  backgroundPosition?: string;
  backgroundOpacity?: number; // 0..=100
  drawings?: DrawingStroke[];
  seo?: StudioSeo;
  installed: PluginKind[];
  cells: TplCell[];
}

export interface BlockV2 {
  id: string; kind: PluginKind; x: number; y: number; w: number; h: number;
  locked?: boolean; auto_height?: boolean; style: CellStyle; layout?: string; chat_open?: boolean;
  hide_on?: "mobile" | "desktop";
  data: { [k: string]: Json };
}
export interface PageV2 {
  schema_version: 2;
  template: string;
  theme: {
    primary: string;
    font: string;
    surface?: StudioState["surface"];
    text_scale?: number;
    navbar_style?: "top" | "side";
    navbar_sticky?: boolean;
    /** Sembunyikan lencana "Dibuat dengan UMKM OS" (berlaku hanya untuk paket Pro; ditegakkan server). */
    hide_badge?: boolean;
    background_image?: string;
    background_size?: "cover" | "contain" | "repeat";
    background_position?: string;
    background_opacity?: number;
  };
  seo?: StudioSeo;
  drawings?: DrawingStroke[];
  blocks: BlockV2[];
}

const LAYOUTS = manifest.plugins as Record<string, { layouts: string[] }>;
const hasLayouts = (k: string) => (LAYOUTS[k]?.layouts.length ?? 0) > 0;
/** id blok: `[a-z0-9-]`, 1–40, tanpa garis bawah (aturan server). */
export const safeId = (id: string) => id.toLowerCase().replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40) || "blok";

/** Buang string kosong di SEO agar tidak dikirim (server menolak teks wajib kosong hanya bila required). */
function cleanSeo(seo: StudioSeo | undefined): StudioSeo | undefined {
  if (!seo) return undefined;
  const out: StudioSeo = {};
  for (const k of ["title", "description", "image", "favicon"] as const) {
    const v = seo[k]?.trim();
    if (v) out[k] = v;
  }
  return Object.keys(out).length ? out : undefined;
}

export function toDoc(st: StudioState, template: string): PageV2 {
  const used = new Set<string>();
  /** id sel → id blok di dokumen (safeId bisa mengubahnya); dipakai untuk target coretan. */
  const idMap = new Map<string, string>();
  const blocks = st.cells.map((c): BlockV2 => {
    let id = safeId(c.id);
    while (used.has(id)) id = `${id.slice(0, 36)}-${used.size}`;
    used.add(id);
    idMap.set(c.id, id);
    const b: BlockV2 = { id, kind: c.kind, x: c.x, y: c.y, w: c.w, h: c.h, style: c.style, data: c.data, auto_height: !c.layer && c.autoH !== false };
    if (c.locked) b.locked = true;
    if (hasLayouts(c.kind) && c.layout) b.layout = c.layout;
    if (c.kind === "chatbot") b.chat_open = c.chatOpen !== false;
    if (c.hideOn) b.hide_on = c.hideOn;
    return b;
  });
  const theme: PageV2["theme"] = {
    primary: st.primary.toUpperCase(),
    font: st.font,
    surface: st.surface,
    text_scale: st.textScale ?? 1,
  };
  if (st.navbarStyle) theme.navbar_style = st.navbarStyle;
  if (st.navbarSticky !== undefined) theme.navbar_sticky = st.navbarSticky;
  if (st.hideBadge) theme.hide_badge = true;
  if (st.backgroundImage?.trim()) theme.background_image = st.backgroundImage.trim();
  if (st.backgroundSize) theme.background_size = st.backgroundSize;
  if (st.backgroundPosition?.trim()) theme.background_position = st.backgroundPosition.trim();
  if (st.backgroundOpacity !== undefined) theme.background_opacity = st.backgroundOpacity;

  const doc: PageV2 = {
    schema_version: 2,
    template,
    theme,
    blocks,
  };
  const seo = cleanSeo(st.seo);
  if (seo) doc.seo = seo;
  const drawings = cleanDrawings(st.drawings, idMap);
  if (drawings.length > 0) doc.drawings = drawings;
  return doc;
}

/** Batas validator server (schema_v2.rs). */
const MAX_STROKES = 200;
const MAX_POINTS = 1000;

/**
 * Coretan yang pasti diterima server: target `background` atau `cell:<id blok yang ada>` (draf lama
 * menyimpan id tanpa awalan), titik bilangan bulat ≥ 0, maksimal 1000 titik, maksimal 200 coretan.
 */
function cleanDrawings(list: DrawingStroke[] | undefined, idMap: Map<string, string>): DrawingStroke[] {
  const out: DrawingStroke[] = [];
  for (const d of list ?? []) {
    let target: string | null = null;
    if (d.target === "background") target = "background";
    else {
      const raw = d.target.startsWith("cell:") ? d.target.slice(5) : d.target;
      const id = idMap.get(raw);
      if (id) target = `cell:${id}`;
    }
    if (!target || !Array.isArray(d.points) || d.points.length === 0) continue;
    const points = d.points.slice(0, MAX_POINTS).map(([x, y]) => [Math.max(0, Math.round(x)), Math.max(0, Math.round(y))] as [number, number]);
    out.push({ ...d, target, points, size: Math.max(1, Math.min(64, d.size)), opacity: Math.max(0, Math.min(1, d.opacity)) });
    if (out.length >= MAX_STROKES) break;
  }
  return out;
}

export function fromDoc(doc: PageV2, minSize: (k: PluginKind) => { minW: number; minH: number }): StudioState {
  const cells: TplCell[] = doc.blocks.map((b) => ({
    id: b.id, kind: b.kind, x: b.x, y: b.y, w: b.w, h: b.h, ...minSize(b.kind),
    style: b.style, data: b.data, layout: b.layout, autoH: b.auto_height !== false, chatOpen: b.chat_open !== false,
    locked: b.locked || undefined, hideOn: b.hide_on, ...(b.kind === "background" || (b.kind === "image" && b.layout !== "popup") ? { layer: true, autoH: false } : {}),
  }));
  return {
    primary: doc.theme.primary,
    onPrimary: onPrimaryOf(doc.theme.primary),
    font: doc.theme.font,
    surface: doc.theme.surface ?? "light",
    textScale: doc.theme.text_scale ?? 1,
    navbarStyle: doc.theme.navbar_style,
    navbarSticky: doc.theme.navbar_sticky,
    hideBadge: doc.theme.hide_badge,
    backgroundImage: doc.theme.background_image,
    backgroundSize: doc.theme.background_size,
    backgroundPosition: doc.theme.background_position,
    backgroundOpacity: doc.theme.background_opacity,
    drawings: doc.drawings,
    seo: doc.seo,
    installed: [],
    cells,
  };
}

/**
 * `onPrimary` tidak disimpan di dokumen (skema server belum mengenal field itu — Fix 2e step_1).
 * Dokumen lama/dari server: ambil token palet bila cocok, selain itu turunkan dari luminans
 * (hasilnya selalu AA karena mengikuti aturan onColor). Setelah skema mengenal field ini,
 * baca doc.theme.on_primary lebih dulu di sini.
 */
function onPrimaryOf(primary: string): string {
  const p = primary.toUpperCase();
  return PALETTES.find((x) => x.primary.toUpperCase() === p)?.onPrimary ?? onColor(p);
}

export const isV2 = (c: unknown): c is PageV2 => !!c && typeof c === "object" && (c as { schema_version?: unknown }).schema_version === 2;
