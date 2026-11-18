// Model editor Studio: tipe keadaan, konstanta plugin, dan pembentuk/pengubah sel (tanpa React).



import { FIELDS, FONT_PRESETS, defaultData } from "@/renderer/sections";
import type { Business,Json,PageContent,SectionType } from "@/renderer/types";
import { insert, place } from "@/studio/grid";
import { EXTRA_DATA, PLUGINS, pluginDef, type PluginKind, type PluginCategory } from "@/studio/plugins";
import { type TplCell } from "@/studio/templates";
import { type StudioState } from "@/studio/doc";
import { t as tx, tmap } from "@/i18n";

export type StudioCell = TplCell;
/** Keadaan Studio; dokumen server = toDoc(Studio) (schema_version 2). */
export type Studio = StudioState;
export type SaveState = "idle" | "saving" | "saved" | "error" | "conflict" | "readonly";
export const CATEGORIES: PluginCategory[] = ["Konten", "Jualan", "Layanan", "Info toko"];
export const CATEGORY_LABEL: Record<PluginCategory, string> = tmap({ Konten: "Konten", Jualan: "Jualan", Layanan: "Layanan", "Info toko": "Info toko" });

export const EMPTY_BUSINESS: Business = {
  profile: { get name() { return tx("Nama Toko"); }, socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false },
  addresses: [], hours: [], exceptions: [],
};
export const storageKey = (t: string) => `umkm-studio-${t}`;
let seq = 0;
export const newId = (k: string) => `${k.replace(/_/g, "-")}-${Date.now().toString(36)}${(seq++).toString(36)}`;
/** Chatbot di luar grid: gelembung bulat atau bilah bawah, keduanya mengikuti layar. */
export const isFloating = (c: StudioCell) => c.kind === "chatbot" && (c.layout === "floating" || c.layout === "bar");
/** WhatsApp melayang: di luar grid seperti chatbot melayang (server: is_embed). */
export const isFloatingWa = (c: StudioCell) => c.kind === "whatsapp" && c.layout === "floating";
/** Blok yang tidak menempati grid. */
export const isEmbed = (c: StudioCell) => isFloating(c) || isFloatingWa(c) || (c.kind === "image" && c.layout === "popup");
/** Plugin di luar renderer section v1 (data & batas sama dengan validator server schema_v2.rs). */
export const EXTRA_LIMITS: Partial<Record<PluginKind, Record<string, { max: number; required?: boolean }>>> = {
  chatbot: { title: { max: 40, required: true }, welcome: { max: 300 } },
  text: { title: { max: 120 }, body: { max: 5000, required: true } },
  whatsapp: { label: { max: 40, required: true }, message: { max: 300 } },
  background: {},
  image: { alt: { max: 160 } },
  shape: {},
  logo: {},
  article_body: { body: { max: 20000, required: true } },
  blog_list: { title: { max: 120 } },
};
/** Satu per halaman (ADR-0019 max_per_page). */
export const SINGLE: PluginKind[] = ["chatbot", "whatsapp"];
/** Plugin yang punya satu foto utama (diganti saat foto dijatuhkan). Galeri: foto ditambahkan. */
export const IMAGE_KEY: Partial<Record<PluginKind, string>> = { hero: "image", about: "image", promotion: "image", background: "image", image: "image" };
/** Lapisan bebas yang boleh menumpuk blok lain (server: `is_layer`). Gambar mode popup ada di luar grid. */
export const isLayerKind = (k: PluginKind, layout?: string) => k === "background" || k === "shape" || k === "logo" || (k === "image" && layout !== "popup");
export const takesPhoto = (k: PluginKind) => k in IMAGE_KEY || k === "gallery";

export function cellFor(kind: PluginKind, data?: { [k: string]: Json }): StudioCell {
  const d = pluginDef(kind);
  return {
    id: newId(kind), kind, x: 0, y: 0, w: d.w, h: d.h, minW: d.minW, minH: d.minH,
    data: data ?? (EXTRA_DATA[kind] ? { ...EXTRA_DATA[kind] } : defaultData(kind as SectionType)),
    style: kind === "promotion" ? "tint" : kind === "chatbot" ? "card" : "plain",
    layout: d.layouts?.[0]?.value, autoH: !isLayerKind(kind, d.layouts?.[0]?.value), chatOpen: true,
    ...(isLayerKind(kind, d.layouts?.[0]?.value) ? { layer: true } : {}),
  };
}

/** Tata letak dari draf website yang ada: section bertumpuk → baris penuh, plus chatbot di samping banner. */
export function fromDraft(content: PageContent | null): Studio {
  let cells: StudioCell[] = [];
  const secs = (content?.sections ?? []).filter((s) => s.enabled && PLUGINS.some((p) => p.kind === s.type));
  for (const s of secs.length ? secs : [{ type: "hero" as const, data: defaultData("hero") }, { type: "product_grid" as const, data: defaultData("product_grid") }]) {
    cells = insert(cells, cellFor(s.type, s.data)) as StudioCell[];
  }
  const hero = cells.find((c) => c.kind === "hero");
  if (hero) {
    cells = place(cells, { ...hero, w: 8 }) as StudioCell[];
    cells = insert(cells, cellFor("chatbot"), { x: 8, y: hero.y }) as StudioCell[];
  }
  return withInstalled({ primary: content?.theme.primary ?? "#4F46E5", surface: "light", font: content?.theme.font ?? FONT_PRESETS[0]!, installed: [], cells });
}
export const withInstalled = (s: Studio): Studio => ({ ...s, installed: [...new Set<PluginKind>([...s.installed, "hero", "product_grid", "chatbot", ...s.cells.map((c) => c.kind)])] });

/** Set nilai pada path data ("title", "cta.label", "items.0.quote") dengan batas panjang dari skema field. */
export function setPath(kind: PluginKind, data: { [k: string]: Json }, path: string, value: string): { [k: string]: Json } | string {
  const parts = path.split(".");
  const extra = EXTRA_LIMITS[kind];
  const fields = extra ? Object.entries(extra).map(([key, f]) => ({ key, ...f, item: undefined })) : FIELDS[kind as SectionType];
  const top = fields.find((f) => f.key === parts[0]);
  const leaf = parts.length === 3 ? top?.item?.find((f) => f.key === parts[2]) : parts.length === 2 && parts[0] === "cta" ? { max: 40, required: true } : top;
  const max = leaf?.max;
  if (leaf?.required && !value) return tx("Isian ini tidak boleh kosong.");
  const v = max ? value.slice(0, max) : value;
  const next = structuredClone(data);
  let cur: Json = next;
  for (let i = 0; i < parts.length - 1; i++) {
    const k = parts[i]!;
    const o = cur as { [k: string]: Json } & Json[];
    if (o[k as never] === undefined || o[k as never] === null) (o as { [k: string]: Json })[k] = {};
    cur = (o as { [k: string]: Json })[k]!;
  }
  (cur as { [k: string]: Json })[parts[parts.length - 1]!] = v;
  return next;
}

/** Terapkan path ke data TERBARU sel (bukan salinan saat render). */
export function setPathOr(c: StudioCell, path: string, value: string): { [k: string]: Json } {
  const r = setPath(c.kind, c.data, path, value);
  return typeof r === "string" ? c.data : r;
}
