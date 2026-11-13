// Konversi keadaan Studio ↔ dokumen `schema_version = 2` (ADR-0017). Bentuk dokumen adalah kontrak
// dengan validator server (crates/domain/src/website/schema_v2.rs) — field di luar daftar ditolak server.

import type { Json } from "@/renderer/types";

import manifest from "./plugin-manifest.json";
import type { CellStyle, PluginKind } from "./plugins";
import type { TplCell } from "./templates";

export interface StudioSeo { title?: string; description?: string; image?: string; favicon?: string }
export interface StudioState {
  primary: string;
  surface: "light" | "cream" | "dark";
  font: string;
  textScale?: number;
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
  theme: { primary: string; font: string; surface?: StudioState["surface"]; text_scale?: number };
  seo?: StudioSeo;
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
  const blocks = st.cells.map((c): BlockV2 => {
    let id = safeId(c.id);
    while (used.has(id)) id = `${id.slice(0, 36)}-${used.size}`;
    used.add(id);
    const b: BlockV2 = { id, kind: c.kind, x: c.x, y: c.y, w: c.w, h: c.h, style: c.style, data: c.data, auto_height: c.autoH !== false };
    if (c.locked) b.locked = true;
    if (hasLayouts(c.kind) && c.layout) b.layout = c.layout;
    if (c.kind === "chatbot") b.chat_open = c.chatOpen !== false;
    if (c.hideOn) b.hide_on = c.hideOn;
    return b;
  });
  const doc: PageV2 = {
    schema_version: 2,
    template,
    theme: { primary: st.primary.toUpperCase(), font: st.font, surface: st.surface, text_scale: st.textScale ?? 1 },
    blocks,
  };
  const seo = cleanSeo(st.seo);
  if (seo) doc.seo = seo;
  return doc;
}

export function fromDoc(doc: PageV2, minSize: (k: PluginKind) => { minW: number; minH: number }): StudioState {
  const cells: TplCell[] = doc.blocks.map((b) => ({
    id: b.id, kind: b.kind, x: b.x, y: b.y, w: b.w, h: b.h, ...minSize(b.kind),
    style: b.style, data: b.data, layout: b.layout, autoH: b.auto_height !== false, chatOpen: b.chat_open !== false,
    locked: b.locked || undefined, hideOn: b.hide_on,
  }));
  return {
    primary: doc.theme.primary, font: doc.theme.font, surface: doc.theme.surface ?? "light", textScale: doc.theme.text_scale ?? 1,
    seo: doc.seo, installed: [], cells,
  };
}

export const isV2 = (c: unknown): c is PageV2 => !!c && typeof c === "object" && (c as { schema_version?: unknown }).schema_version === 2;
