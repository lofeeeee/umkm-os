"use client";

// Studio website — PROTOTIPE ADR-0017 (status: Proposed). Kanvas grid 12 kolom + katalog plugin resmi.
// - Mulai dari galeri template yang langsung diisi data toko.
// - Edit langsung: klik plugin untuk memilih, klik teksnya untuk mengetik; seret foto ke plugin.
// - Undo/Redo (Ctrl+Z / Ctrl+Shift+Z) + simpan otomatis.
// Belum menyentuh situs publik: tata letak disimpan di browser (localStorage) sampai ADR disetujui dan
// schema_version 2 + validator server dibangun.

import Link from "@/ui/link";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import {
  LockSimple, LockSimpleOpen, ArrowSquareOut, ClockCounterClockwise, WarningCircle, ArrowClockwise, ArrowCounterClockwise, ArrowLeft, ArrowsIn, ArrowsOut, CaretDown, CaretUp, ChatCircleDots, Check, Copy,
  ArrowDown, ArrowRight, ArrowUp, CornersIn, CornersOut, DesktopTower, DeviceMobile, ImageSquare, MagnifyingGlass, Plus, PuzzlePiece, SidebarSimple, Sparkle, SquaresFour, Trash, WhatsappLogo, X,
  Crop,
} from "@phosphor-icons/react/ssr";

import { useUploads } from "@/dashboard/assets";
import { Assets, FieldInput } from "@/dashboard/editor/fields";
import { WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { EditContext, type EditApi } from "@/renderer/edit";
import { Site, type GridProduct } from "@/renderer/Site";
import { TextBlock, WhatsAppButton } from "@/renderer/StudioSite";
import { fontStack, primaryVars } from "@/renderer/safe";
import { FIELDS, FONT_PRESETS, defaultData } from "@/renderer/sections";
import type { Business, Json, PageContent, SectionType } from "@/renderer/types";
import { Canvas, DESIGN_WIDTH, GAP, ROW, rowsFor } from "@/studio/Canvas";
import { COLS, compact, insert, mobileOrder, moveBy, place } from "@/studio/grid";
import { commit, redo, silent, start, undo, type History } from "@/studio/history";
import { CELL_STYLES, EXTRA_DATA, PALETTES, PLUGINS, needsUpgrade, pluginDef, pluginPlan, type PluginKind, type PluginCategory } from "@/studio/plugins";
import { readiness, type Issue } from "@/studio/readiness";
import { TEMPLATES, type TplCell } from "@/studio/templates";
import { fromDoc, isV2, toDoc, type StudioState } from "@/studio/doc";
import { IconRail, type StudioTab } from "@/studio/IconRail";
import { PagesPanel, type PageItem } from "@/studio/PagesPanel";
import { ThemePanel, type ThemeConfig } from "@/studio/ThemePanel";
import { DrawPanel, type DrawConfig, type DrawTool, type DrawTarget } from "@/studio/DrawPanel";
import { DrawingLayer, type DrawingStroke } from "@/studio/DrawingLayer";
import { ImageCropModal } from "@/ui/ImageCropModal";
import { ApiError, describe } from "@/lib/api";
import { waktu } from "@/lib/labels";
import { confirm, Sheet } from "@/ui/dialog";
import { Badge, Skeleton } from "@/ui/page";
import { dismissToast, toast } from "@/ui/toast";

type StudioCell = TplCell;
/** Keadaan Studio; dokumen server = toDoc(Studio) (schema_version 2). */
type Studio = StudioState;
type SaveState = "idle" | "saving" | "saved" | "error" | "conflict" | "readonly";
const TEXT_SCALES = [{ v: 0.9, l: "Kecil" }, { v: 1, l: "Sedang" }, { v: 1.1, l: "Besar" }];

const CATEGORIES: { label: PluginCategory; icon: typeof PuzzlePiece }[] = [
  { label: "Konten", icon: PuzzlePiece },
  { label: "Jualan", icon: Sparkle },
  { label: "Layanan", icon: ChatCircleDots },
  { label: "Info toko", icon: SquaresFour },
];

const EMPTY_BUSINESS: Business = {
  profile: { name: "Nama Toko", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false },
  addresses: [], hours: [], exceptions: [],
};
const storageKey = (t: string) => `umkm-studio-${t}`;
let seq = 0;
const newId = (k: string) => `${k.replace(/_/g, "-")}-${Date.now().toString(36)}${(seq++).toString(36)}`;
/** Chatbot di luar grid: gelembung bulat atau bilah bawah, keduanya mengikuti layar. */
const isFloating = (c: StudioCell) => c.kind === "chatbot" && (c.layout === "floating" || c.layout === "bar");
/** WhatsApp melayang: di luar grid seperti chatbot melayang (server: is_embed). */
const isFloatingWa = (c: StudioCell) => c.kind === "whatsapp" && c.layout === "floating";
/** Blok yang tidak menempati grid. */
const isEmbed = (c: StudioCell) => isFloating(c) || isFloatingWa(c);
/** Plugin di luar renderer section v1 (data & batas sama dengan validator server schema_v2.rs). */
const EXTRA_LIMITS: Partial<Record<PluginKind, Record<string, { max: number; required?: boolean }>>> = {
  chatbot: { title: { max: 40, required: true }, welcome: { max: 300 } },
  text: { title: { max: 120 }, body: { max: 5000, required: true } },
  whatsapp: { label: { max: 40, required: true }, message: { max: 300 } },
};
/** Satu per halaman (ADR-0019 max_per_page). */
const SINGLE: PluginKind[] = ["chatbot", "whatsapp"];
/** Plugin yang punya satu foto utama (diganti saat foto dijatuhkan). Galeri: foto ditambahkan. */
const IMAGE_KEY: Partial<Record<PluginKind, string>> = { hero: "image", about: "image", promotion: "image" };
const takesPhoto = (k: PluginKind) => k in IMAGE_KEY || k === "gallery";

function cellFor(kind: PluginKind, data?: { [k: string]: Json }): StudioCell {
  const d = pluginDef(kind);
  return {
    id: newId(kind), kind, x: 0, y: 0, w: d.w, h: d.h, minW: d.minW, minH: d.minH,
    data: data ?? (EXTRA_DATA[kind] ? { ...EXTRA_DATA[kind] } : defaultData(kind as SectionType)),
    style: kind === "promotion" ? "tint" : kind === "chatbot" ? "card" : "plain",
    layout: d.layouts?.[0]?.value, autoH: true, chatOpen: true,
  };
}

/** Tata letak dari draf website yang ada: section bertumpuk → baris penuh, plus chatbot di samping banner. */
function fromDraft(content: PageContent | null): Studio {
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
const withInstalled = (s: Studio): Studio => ({ ...s, installed: [...new Set<PluginKind>([...s.installed, "hero", "product_grid", "chatbot", ...s.cells.map((c) => c.kind)])] });

/** Set nilai pada path data ("title", "cta.label", "items.0.quote") dengan batas panjang dari skema field. */
function setPath(kind: PluginKind, data: { [k: string]: Json }, path: string, value: string): { [k: string]: Json } | string {
  const parts = path.split(".");
  const extra = EXTRA_LIMITS[kind];
  const fields = extra ? Object.entries(extra).map(([key, f]) => ({ key, ...f, item: undefined })) : FIELDS[kind as SectionType];
  const top = fields.find((f) => f.key === parts[0]);
  const leaf = parts.length === 3 ? top?.item?.find((f) => f.key === parts[2]) : parts.length === 2 && parts[0] === "cta" ? { max: 40, required: true } : top;
  const max = leaf?.max;
  if (leaf?.required && !value) return "Isian ini tidak boleh kosong.";
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
function setPathOr(c: StudioCell, path: string, value: string): { [k: string]: Json } {
  const r = setPath(c.kind, c.data, path, value);
  return typeof r === "string" ? c.data : r;
}

export default function StudioPage() {
  return <WithTenant>{(ctx) => <StudioEditor ctx={ctx} />}</WithTenant>;
}

function StudioEditor({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [draftAssetsForUploads, setDraftAssetsForUploads] = useState<Record<string, string>>({});
  const assets = useUploads(ctx.tenant.id, ctx.can("website.edit"), draftAssetsForUploads);
  const [hist, setHist] = useState<History<Studio> | null>(null);
  const st = hist?.present ?? null;
  const [business, setBusiness] = useState<Business>(EMPTY_BUSINESS);
  /** Kode paket toko (ADR-0019); null = belum diketahui / tanpa izin baca. */
  const [planCode, setPlanCode] = useState<string | null>(null);
  const [rawProducts, setRawProducts] = useState<{ slug: string; name: string; image?: string; price: string; promo_price: string | null; available: boolean }[]>([]);
  const [draft, setDraft] = useState<PageContent | null>(null);
  const [picking, setPicking] = useState<"first" | "change" | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [panel, setPanel] = useState<"installed" | "catalog">("installed");
  const [q, setQ] = useState("");
  const [device, setDevice] = useState<"desktop" | "mobile">("desktop");
  const [full, setFull] = useState<string | null>(null);
  const canEdit = ctx.can("website.edit");
  const canPublish = ctx.can("website.publish");
  // Sinkronisasi server: template website, revisi draf terakhir, versi terbit.
  const [srv, setSrv] = useState<{ template: string; revision: number; published: number | null; savedDoc: string } | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [saveErr, setSaveErr] = useState<string | null>(null);
  const [versionsOpen, setVersionsOpen] = useState(false);
  /** P9-10: catatan "siap terbit" yang sedang ditampilkan (null = tertutup). */
  const [checks, setChecks] = useState<Issue[] | null>(null);
  const [publishing, setPublishing] = useState(false);
  const [stageW, setStageW] = useState(DESIGN_WIDTH);
  const stageRef = useRef<HTMLDivElement>(null);
  const [clipped, setClipped] = useState<Record<string, boolean>>({});
  const [panelOpen, setPanelOpen] = useState(true);
  const [activeTab, setActiveTab] = useState<StudioTab>("elements");
  const [pages, setPages] = useState<PageItem[]>([]);
  const [activeSlug, setActiveSlug] = useState("home");
  const [drawTool, setDrawTool] = useState<DrawTool>(null);
  const [drawConfig, setDrawConfig] = useState<DrawConfig>({ color: "#EF4444", size: 4, opacity: 1 });
  const [drawTarget, setDrawTarget] = useState<DrawTarget>("background");
  const [cropModal, setCropModal] = useState<{ open: boolean; cellId: string | null; imageSrc: string | null }>({ open: false, cellId: null, imageSrc: null });

  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setStageW(e!.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  });
  const scale = Math.min(1, stageW / DESIGN_WIDTH);

  /* ---------------- Muat data toko & draf server ---------------- */
  useEffect(() => {
    let local: Studio | null = null;
    try { local = JSON.parse(localStorage.getItem(storageKey(ctx.tenant.id)) ?? "null") as Studio | null; } catch { local = null; }
    type Draft = { content: PageContent | Record<string, unknown>; revision: number; published_version: number | null; assets: Record<string, string> };
    // Website belum dibuat → pilih template server pertama (Studio menyusun ulang tampilannya sendiri).
    const ensureWebsite = async () => {
      const w = await api<{ template_key: string; pages?: PageItem[] }>("GET", `${base}/website`).catch((e) => (e instanceof ApiError && e.status === 404 ? null : Promise.reject(e)));
      if (w) {
        if (w.pages) setPages(w.pages);
        return w.template_key;
      }
      const tpls = await api<{ key: string }[]>("GET", `${base}/website/templates`);
      const key = tpls[0]?.key;
      if (!key) throw new Error("Tidak ada template website.");
      const res = await api<{ template_key: string; pages?: PageItem[] }>("PUT", `${base}/website`, { json: { template_key: key } });
      if (res.pages) setPages(res.pages);
      return key;
    };
    (async () => {
      const [b, p, template] = await Promise.all([
        api<Business>("GET", `${base}/business/profile`).catch(() => EMPTY_BUSINESS),
        api<{ items: { slug: string; name: string; available: boolean; images: string[]; variants: { price: string; promo_price: string | null }[] }[] }>("GET", `${base}/products?limit=24&status=active`).catch(() => ({ items: [] })),
        ensureWebsite(),
      ]);
      const d = await api<Draft>("GET", `${base}/website/pages/home/draft`);
      setBusiness(b);
      void api<{ plan: { code: string } }>("GET", `${base}/subscription`).then((x) => setPlanCode(x.plan.code)).catch(() => setPlanCode(null));
      setRawProducts(p.items.map((x) => ({ slug: x.slug, name: x.name, image: x.images[0], price: x.variants[0]?.price ?? "0.00", promo_price: x.variants[0]?.promo_price ?? null, available: x.available })));
      setDraftAssetsForUploads(d.assets);
      const minSize = (k: PluginKind) => ({ minW: pluginDef(k).minW, minH: pluginDef(k).minH });
      if (isV2(d.content)) {
        // Dokumen Studio dari server = sumber kebenaran.
        const st0 = withInstalled(fromDoc(d.content, minSize));
        setHist(start(st0));
        setSrv({ template, revision: d.revision, published: d.published_version, savedDoc: JSON.stringify(toDoc(st0, template)) });
      } else {
        // Website v1: konversi otomatis (ADR-0017). Prototipe yang tersimpan di browser dipakai bila ada.
        const v1 = d.content as PageContent;
        setDraft(v1);
        setSrv({ template, revision: d.revision, published: d.published_version, savedDoc: "" });
        if (local) {
          setHist(start(withInstalled({ ...local, cells: local.cells.map((c) => ({ ...c, ...minSize(c.kind) })) })));
          toast.info("Susunan dari prototipe Studio dipakai dan sedang disimpan ke server.");
        } else {
          setHist(start(fromDraft(v1)));
          setPicking("first");
        }
      }
      if (!canEdit) setSaveState("readonly");
    })().catch((e) => { setSaveState("error"); setSaveErr(describe(e)); });
  }, [base, ctx.tenant.id, canEdit]);
  const products = useMemo<GridProduct[]>(() => rawProducts.map((p) => ({ ...p, image: p.image ? assets.url(p.image) : undefined })), [rawProducts, assets]);
  const noPhoto = rawProducts.filter((p) => !p.image).length;

  /* ---------------- Riwayat & simpan otomatis ---------------- */
  const update = useCallback((fn: (s: Studio) => Studio, key = "edit") => {
    setHist((h) => (h ? commit(h, fn(h.present), key, Date.now()) : h));
  }, []);
  const quiet = useCallback((fn: (s: Studio) => Studio) => setHist((h) => (h ? silent(h, fn(h.present)) : h)), []);
  const doUndo = useCallback(() => setHist((h) => (h ? undo(h) : h)), []);
  const doRedo = useCallback(() => setHist((h) => (h ? redo(h) : h)), []);

  /** Dokumen terakhir yang DITOLAK server: jangan dikirim ulang otomatis sampai isinya berubah. */
  const rejected = useRef<string | null>(null);
  /** Simpan draf ke server (PUT dengan base_revision; 409 = diubah di tempat lain). */
  const saveNow = useCallback(async (): Promise<number | null> => {
    if (!st || !srv || !canEdit) return srv?.revision ?? null;
    const doc = toDoc(st, srv.template);
    const json = JSON.stringify(doc);
    if (json === srv.savedDoc) return srv.revision;
    setSaveState("saving");
    try {
      const r = await api<{ revision: number }>("PUT", `${base}/website/pages/${activeSlug}/draft`, { json: { content: doc, base_revision: srv.revision } });
      setSrv((x) => (x ? { ...x, revision: r.revision, savedDoc: json } : x));
      setSaveState("saved");
      setSaveErr(null);
      try { localStorage.removeItem(storageKey(ctx.tenant.id)); } catch { /* abaikan */ }
      return r.revision;
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        setSaveState("conflict");
        setSaveErr("Draf diubah di tempat lain (tab atau perangkat lain). Muat ulang halaman untuk melihat versi terbaru.");
      } else {
        // 4xx (mis. 422 validasi): kirim ulang dokumen yang sama pasti gagal lagi.
        if (e instanceof ApiError && e.status >= 400 && e.status < 500) rejected.current = json;
        setSaveState("error");
        setSaveErr(describe(e));
      }
      return null;
    }
  }, [st, srv, canEdit, base, activeSlug, ctx.tenant.id]);

  /* ---------------- Multi-Halaman & Tiering ---------------- */
  const switchPage = useCallback(async (slug: string) => {
    if (slug === activeSlug) return;
    await saveNow();
    setSel(null);
    setActiveSlug(slug);
    try {
      type DraftDoc = { content: PageContent | Record<string, unknown>; revision: number; published_version: number | null; assets: Record<string, string> };
      const d = await api<DraftDoc>("GET", `${base}/website/pages/${slug}/draft`);
      setDraftAssetsForUploads(d.assets);
      const minSize = (k: PluginKind) => ({ minW: pluginDef(k).minW, minH: pluginDef(k).minH });
      if (isV2(d.content)) {
        const st0 = withInstalled(fromDoc(d.content, minSize));
        setHist(start(st0));
        setSrv((x) => (x ? { ...x, revision: d.revision, published: d.published_version, savedDoc: JSON.stringify(toDoc(st0, x.template)) } : x));
      } else {
        const v1 = d.content as PageContent;
        setHist(start(fromDraft(v1)));
        setSrv((x) => (x ? { ...x, revision: d.revision, published: d.published_version, savedDoc: "" } : x));
      }
    } catch (e) {
      toast.error(describe(e));
    }
  }, [activeSlug, base, saveNow]);

  const createPage = useCallback(async (slug: string, title: string): Promise<boolean> => {
    try {
      const r = await api<PageItem>("POST", `${base}/website/pages`, { json: { slug, title } });
      setPages((p) => [...p, r]);
      toast.ok(`Halaman ${title} dibuat.`);
      await switchPage(slug);
      return true;
    } catch (e) {
      toast.error(describe(e));
      return false;
    }
  }, [base, switchPage]);

  const deletePage = useCallback(async (slug: string): Promise<boolean> => {
    try {
      await api("DELETE", `${base}/website/pages/${slug}`);
      setPages((p) => p.filter((x) => x.slug !== slug));
      toast.ok("Halaman dihapus.");
      if (activeSlug === slug) await switchPage("home");
      return true;
    } catch (e) {
      toast.error(describe(e));
      return false;
    }
  }, [activeSlug, base, switchPage]);

  /* ---------------- Theme & Background ---------------- */
  const themeConfig: ThemeConfig = useMemo(() => ({
    primary: st?.primary ?? "#4F46E5",
    font: st?.font ?? FONT_PRESETS[0]!,
    surface: st?.surface ?? "light",
    textScale: st?.textScale ?? 1,
    navbarStyle: st?.navbarStyle ?? "top",
    navbarSticky: st?.navbarSticky ?? false,
    backgroundImage: st?.backgroundImage,
    backgroundSize: st?.backgroundSize,
    backgroundPosition: st?.backgroundPosition,
    backgroundOpacity: st?.backgroundOpacity ?? 0,
  }), [st]);

  const updateTheme = useCallback((patch: Partial<ThemeConfig>) => {
    update((s) => ({ ...s, ...patch }), "theme");
  }, [update]);

  const uploadBg = useCallback(async (file: File): Promise<string | null> => {
    const id0 = toast.info("Mengunggah foto latar…");
    const u = await assets.upload(file);
    dismissToast(id0);
    if (!u) return null;
    update((s) => ({ ...s, backgroundImage: u.ref }), "bg");
    toast.ok("Foto latar dipasang.");
    return u.ref;
  }, [assets, update]);

  /* ---------------- Coretan Bebas (Brush & Eraser) ---------------- */
  const addStroke = useCallback((stroke: DrawingStroke) => {
    update((s) => ({ ...s, drawings: [...(s.drawings ?? []), stroke] }), "draw");
  }, [update]);

  const eraseStroke = useCallback((id: string) => {
    update((s) => ({ ...s, drawings: (s.drawings ?? []).filter((d) => d.id !== id) }), "erase");
  }, [update]);

  const clearStrokes = useCallback(() => {
    update((s) => ({
      ...s,
      drawings: (s.drawings ?? []).filter((d) => drawTarget === "background" ? d.target !== "background" : d.target === "background"),
    }), "clear_draw");
    toast.ok("Coretan dibersihkan.");
  }, [drawTarget, update]);

  useEffect(() => {
    if (!st || !srv || picking === "first" || !canEdit || saveState === "conflict") return;
    const json = JSON.stringify(toDoc(st, srv.template));
    if (json === srv.savedDoc || json === rejected.current) return;
    const t = setTimeout(() => void saveNow(), 1200);
    return () => clearTimeout(t);
  }, [st, srv, picking, canEdit, saveState, saveNow]);
  const saved = !!st && !!srv && JSON.stringify(toDoc(st, srv.template)) === srv.savedDoc;

  /** Terbitkan: tampilkan catatan siap-terbit dulu bila ada (tidak memblokir). */
  function publish() {
    if (!st) return;
    const issues = readiness({ cells: st.cells, business, productCount: rawProducts.length, seo: st.seo });
    if (issues.length > 0) { setChecks(issues); return; }
    void doPublish();
  }

  async function doPublish() {
    if (!srv) return;
    setChecks(null);
    setPublishing(true);
    try {
      const rev = await saveNow();
      if (rev === null) { toast.error("Draf belum tersimpan, jadi belum bisa diterbitkan."); return; }
      const r = await api<{ version: number }>("POST", `${base}/website/pages/${activeSlug}/publish`, { json: { revision: rev } });
      setSrv((x) => (x ? { ...x, published: r.version ?? x.published } : x));
      toast.ok("Website diterbitkan. Pengunjung melihat versi baru dalam ±1 menit.");
    } catch (e) {
      toast.error(describe(e));
    } finally {
      setPublishing(false);
    }
  }


  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable]")) return;
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); if (e.shiftKey) doRedo(); else doUndo(); }
      else if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); doRedo(); }
      else if (e.key === "Escape") setSel(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [doUndo, doRedo]);

  useEffect(() => {
    const block = (e: DragEvent) => { if (e.dataTransfer?.types.includes("Files")) e.preventDefault(); };
    window.addEventListener("dragover", block);
    window.addEventListener("drop", block);
    return () => { window.removeEventListener("dragover", block); window.removeEventListener("drop", block); };
  }, []);

  /* ---------------- Tinggi otomatis ---------------- */
  const onMeasure = useCallback((id: string, px: number) => {
    const rows = rowsFor(px);
    let clip = false;
    quiet((s) => {
      const c = s.cells.find((x) => x.id === id);
      if (!c) return s;
      clip = rows > c.h;
      if (c.autoH === false || rows === c.h) return s;
      clip = false;
      // Plugin terkunci tetap boleh menyesuaikan tinggi dengan isinya; yang lain mengalir ulang.
      if (c.locked) return { ...s, cells: compact(s.cells.map((x) => (x.id === id ? { ...x, h: rows } : x))) as StudioCell[] };
      return { ...s, cells: place(s.cells, { ...c, h: rows }) as StudioCell[] };
    });
    setClipped((m) => (m[id] === clip ? m : { ...m, [id]: clip }));
  }, [quiet]);
  const onResizeH = useCallback((id: string) => {
    update((s) => ({ ...s, cells: s.cells.map((c) => (c.id === id && c.autoH !== false ? { ...c, autoH: false } : c)) }), "drag");
  }, [update]);

  /* ---------------- Aksi plugin ---------------- */
  const setCell = useCallback((id: string, patch: Partial<StudioCell>, key = `cell:${id}`) => {
    update((s) => ({ ...s, cells: s.cells.map((c) => (c.id === id ? { ...c, ...patch } : c)) }), key);
  }, [update]);
  const setChatOpen = useCallback((id: string, open: boolean) => setCell(id, { chatOpen: open }, ""), [setCell]);

  function add(kind: PluginKind, at?: { x: number; y: number }) {
    if (SINGLE.includes(kind) && st?.cells.some((c) => c.kind === kind)) { toast.info(`${pluginDef(kind).name} sudah ada di halaman.`); return; }
    const c = cellFor(kind);
    update((s) => ({ ...s, installed: s.installed.includes(kind) ? s.installed : [...s.installed, kind], cells: insert(s.cells, c, at) as StudioCell[] }), "");
    setSel(c.id);
  }
  function remove(id: string) {
    update((s) => ({ ...s, cells: s.cells.filter((c) => c.id !== id) }), "");
    setSel(null);
    toast.info("Plugin dihapus. Tekan Ctrl+Z untuk membatalkan.");
  }
  async function putPhoto(id: string, file: File) {
    const id0 = toast.info("Mengunggah foto…");
    const u = await assets.upload(file); // kompresi & pesan galat ditangani useUploads
    dismissToast(id0);
    if (!u) return;
    update((s) => ({
      ...s,
      cells: s.cells.map((c) => {
        if (c.id !== id) return c;
        if (c.kind === "gallery") {
          const imgs = Array.isArray(c.data.images) ? c.data.images : [];
          return { ...c, data: { ...c.data, images: [...imgs, { asset: u.ref }].slice(0, 24) } };
        }
        const k = IMAGE_KEY[c.kind];
        return k ? { ...c, data: { ...c.data, [k]: u.ref } } : c;
      }),
    }), "");
    toast.ok("Foto dipasang.");
  }
  function applyTemplate(key: string) {
    const t = TEMPLATES.find((x) => x.key === key)!;
    const r = t.build({ name: business.profile.name, description: business.profile.description ?? null });
    const next = withInstalled({ ...r, installed: st?.installed ?? [] });
    if (picking === "first") setHist(start(next));
    else update(() => next, "");
    setPicking(null);
    setSel(null);
    toast.ok(`Template ${t.name} dipakai. Klik teks mana saja untuk mengubahnya.`);
  }

  /* ---------------- Render plugin ---------------- */
  const primary = st?.primary ?? "#4F46E5";
  const font = st?.font ?? FONT_PRESETS[0]!;
  const theme = useMemo(() => ({ primary, font }), [primary, font]);
  const editApi = useCallback((c: StudioCell): EditApi => ({
    commit: (path, value) => {
      const r = setPath(c.kind, c.data, path, value);
      if (typeof r === "string") { toast.error(r); return; }
      update((s) => ({ ...s, cells: s.cells.map((x) => (x.id === c.id ? { ...x, data: setPathOr(x, path, value) } : x)) }), "");
    },
  }), [update]);

  const renderCell = useCallback((c: StudioCell, editable = false, th: { primary: string; font: string } = theme) => (
    <div className={`st-content st-style-${c.style}${c.layout ? ` st-layout-${c.layout}` : ""}`}>
      {c.kind === "chatbot" ? (
        <ChatPreview primary={th.primary} title={String(c.data.title ?? "Tanya kami")} welcome={String(c.data.welcome ?? "")}
          open={c.chatOpen !== false} onToggle={() => setChatOpen(c.id, c.chatOpen === false)} />
      ) : c.kind === "text" || c.kind === "whatsapp" ? (
        <EditContext.Provider value={editable ? editApi(c) : null}>
          <div className="site s-solo" style={{ ...primaryVars(th.primary), ["--s-font" as string]: fontStack(th.font) }}>
            <section className="s"><div className="s-inner">
              {c.kind === "text" ? <TextBlock b={c} /> : <WhatsAppButton b={c} business={business} />}
            </div></section>
          </div>
        </EditContext.Provider>
      ) : (
        <EditContext.Provider value={editable ? editApi(c) : null}>
          <Site
            content={{ schema_version: 1, template: "studio", theme: th, sections: [{ id: c.id, type: c.kind as SectionType, enabled: true, data: c.data }] }}
            business={business}
            products={products.length ? products : undefined}
            assetUrl={assets.url}
          />
        </EditContext.Provider>
      )}
    </div>
  ), [theme, business, products, assets.url, setChatOpen, editApi]);

  const shownPlugins = useMemo(() => {
    const t = q.trim().toLowerCase();
    return PLUGINS.filter((p) => !t || `${p.name} ${p.desc} ${p.keywords} ${p.category}`.toLowerCase().includes(t));
  }, [q]);

  const backHref = `/website?tenant=${ctx.tenant.id}`;
  if (!st || !hist) {
    return (
      <div className="sx">
        <header className="sx-top"><Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" />Website</Link><strong>Studio</strong></header>
        <div className="sx-scroll"><div style={{ padding: 32 }}><span className="d-skel d-skel-block" style={{ height: 480 }} /></div></div>
      </div>
    );
  }

  if (picking === "first") {
    return (
      <Assets.Provider value={assets}>
        <div className="sx">
        <header className="sx-top">
          <Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" />Website</Link>
          <span className="sx-brand"><span className="d-avatar" aria-hidden="true">{(ctx.tenant.name.trim()[0] ?? "T").toUpperCase()}</span>Studio</span>
          <span className="sx-spacer" />
          <FullscreenButton />
        </header>
        <div className="sx-scroll" style={{ padding: "32px 24px" }}>
        <TemplateGallery
          first
          business={business}
          renderCell={renderCell}
          onPick={applyTemplate}
          onCurrent={() => { setPicking(null); toast.ok("Memakai susunan website Anda saat ini."); }}
          surfaceOf={(k) => TEMPLATES.find((t) => t.key === k)!.build({ name: business.profile.name, description: business.profile.description ?? null })}
        />
        </div>
        </div>
      </Assets.Provider>
    );
  }

  const selected = st.cells.find((c) => c.id === sel) ?? null;
  const gridCells = st.cells.filter((c) => !isEmbed(c));
  const floating = st.cells.find(isFloating);
  const floatingWa = st.cells.find(isFloatingWa);
  const canvasRows = Math.max(20, rowsFor(gridCells.reduce((max, c) => Math.max(max, (c.y + c.h) * (ROW + GAP)), 0)));
  const canvasHeight = canvasRows * (ROW + GAP);
  const colW = (DESIGN_WIDTH - GAP * (COLS - 1)) / COLS;
  const pw = (w: number) => w * colW + (w - 1) * GAP;
  const ph = (h: number) => h * ROW + (h - 1) * GAP;

  return (
    <Assets.Provider value={assets}>
      <div className="sx">
      <header className="sx-top">
        <Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" />Website</Link>
        <button type="button" className="d-icon-btn" onClick={() => setPanelOpen(!panelOpen)} aria-pressed={panelOpen} aria-label={panelOpen ? "Sembunyikan panel" : "Tampilkan panel"} title={panelOpen ? "Sembunyikan panel" : "Tampilkan panel"}>
          <SidebarSimple weight={panelOpen ? "fill" : "bold"} />
        </button>
        <span className="sx-brand"><span className="d-avatar" aria-hidden="true">{(ctx.tenant.name.trim()[0] ?? "T").toUpperCase()}</span>{ctx.tenant.name}<em>Studio</em></span>
        {activeSlug !== "home" && <Badge tone="brand">{pages.find((p) => p.slug === activeSlug)?.title || activeSlug}</Badge>}
        <SaveBadge state={saveState} saved={saved} err={saveErr} onRetry={() => { setSaveState("idle"); void saveNow(); }} />
        <span className="sx-spacer" />
        <div className="d-row" style={{ gap: 2 }}>
          <button type="button" className="d-icon-btn" onClick={doUndo} disabled={hist.past.length === 0} aria-label="Urungkan (Ctrl+Z)" title="Urungkan (Ctrl+Z)"><ArrowCounterClockwise weight="bold" /></button>
          <button type="button" className="d-icon-btn" onClick={doRedo} disabled={hist.future.length === 0} aria-label="Ulangi (Ctrl+Shift+Z)" title="Ulangi (Ctrl+Shift+Z)"><ArrowClockwise weight="bold" /></button>
        </div>
        <div className="d-seg" role="group" aria-label="Perangkat">
          <button type="button" aria-pressed={device === "desktop"} onClick={() => setDevice("desktop")}><DesktopTower weight="bold" aria-hidden="true" /> Desktop</button>
          <button type="button" aria-pressed={device === "mobile"} onClick={() => setDevice("mobile")}><DeviceMobile weight="bold" aria-hidden="true" /> Ponsel</button>
        </div>
        <span className="sx-zoom" title="Perbesaran kanvas">{Math.round(scale * 100)}%</span>
        <button className="d-btn d-btn-sm" onClick={() => setPicking("change")}><SquaresFour weight="bold" aria-hidden="true" />Template</button>
        <button className="d-btn d-btn-sm" onClick={() => setVersionsOpen(true)}><ClockCounterClockwise weight="bold" aria-hidden="true" />Riwayat</button>
        <FullscreenButton />
        {ctx.tenant.primary_hostname && srv?.published && (
          <a className="d-btn d-btn-sm d-btn-ghost" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noopener noreferrer"><ArrowSquareOut weight="bold" aria-hidden="true" />Lihat situs</a>
        )}
        <button className="d-btn d-btn-sm d-btn-primary" onClick={publish} disabled={!canPublish || publishing || saveState === "conflict"} title={canPublish ? "Terbitkan ke situs publik" : "Butuh izin terbitkan website"}>
          {publishing ? "Menerbitkan…" : srv?.published ? "Terbitkan perubahan" : "Terbitkan"}
        </button>
      </header>

      <div className="sx-body" data-panel={panelOpen ? "open" : "closed"}>
        <IconRail
          activeTab={selected ? null : activeTab}
          onSelectTab={(t) => {
            setSel(null);
            if (activeTab === t && panelOpen) {
              setPanelOpen(false);
            } else {
              setActiveTab(t);
              setPanelOpen(true);
            }
          }}
        />
        <aside className="st-panel sx-panel" aria-label={selected ? "Pengaturan plugin" : activeTab === "pages" ? "Halaman" : activeTab === "theme" ? "Desain" : activeTab === "draw" ? "Coretan" : "Plugin"} hidden={!panelOpen}>
          {selected ? (
            <Inspector
              c={selected}
              onBack={() => setSel(null)}
              onChange={(patch, key) => setCell(selected.id, patch, key)}
              onWidth={(w) => update((s) => ({ ...s, cells: place(s.cells, { ...selected, w }) as StudioCell[] }), "")}
              onMove={(d) => update((s) => ({ ...s, cells: [...moveBy(s.cells.filter((x) => !isEmbed(x)), selected.id, d) as StudioCell[], ...s.cells.filter(isEmbed)] }), `move:${selected.id}`)}
              onLayout={(l) => {
                if (l === "inline" && isEmbed(selected)) {
                  update((s) => ({ ...s, cells: insert(s.cells.filter((c) => c.id !== selected.id), { ...selected, layout: "inline" } as StudioCell) as StudioCell[] }), "");
                } else setCell(selected.id, { layout: l }, "");
              }}
              onDuplicate={() => { const c = { ...selected, id: newId(selected.kind) }; update((s) => ({ ...s, cells: insert(s.cells, c, { x: selected.x, y: selected.y + selected.h }) as StudioCell[] }), ""); setSel(c.id); }}
              onRemove={() => remove(selected.id)}
              onFull={() => setFull(selected.id)}
              onPhoto={(f) => void putPhoto(selected.id, f)}
              onCrop={() => {
                const k = IMAGE_KEY[selected.kind];
                const ref = k ? (selected.data[k] as string) : null;
                if (ref && assets.url(ref)) {
                  setCropModal({ open: true, cellId: selected.id, imageSrc: assets.url(ref)! });
                } else {
                  toast.error("Belum ada foto yang dipasang untuk dipotong.");
                }
              }}
              aiHref={`/ai?tenant=${ctx.tenant.id}`}
              businessHref={`/business?tenant=${ctx.tenant.id}`}
              upgradeHref={needsUpgrade(pluginPlan(selected.kind), planCode) ? `/subscription?tenant=${ctx.tenant.id}` : null}
            />
          ) : activeTab === "pages" ? (
            <PagesPanel
              pages={pages}
              activeSlug={activeSlug}
              onSelectPage={(slug) => void switchPage(slug)}
              onCreatePage={createPage}
              onDeletePage={deletePage}
              isPro={planCode === "pro"}
              onUpgradeClick={() => window.open(`/subscription?tenant=${ctx.tenant.id}`, "_self")}
            />
          ) : activeTab === "theme" ? (
            <ThemePanel
              config={themeConfig}
              onChange={updateTheme}
              onUploadBackground={uploadBg}
              assetUrl={assets.url}
            />
          ) : activeTab === "draw" ? (
            <DrawPanel
              activeTool={drawTool}
              onSelectTool={setDrawTool}
              target={drawTarget}
              onSelectTarget={setDrawTarget}
              selectedCellId={null}
              config={drawConfig}
              onChangeConfig={setDrawConfig}
              primaryColor={st.primary}
              onClearStrokes={clearStrokes}
            />
          ) : (
            <>
              <div style={{ position: "relative", marginBottom: 12 }}>
                <MagnifyingGlass aria-hidden="true" weight="bold" style={{ position: "absolute", left: 10, top: "50%", translate: "0 -50%", width: 14, height: 14, color: "var(--muted)" }} />
                <input type="search" aria-label="Cari plugin" placeholder="Cari plugin (mis. promo, wa, banner)…" value={q} onChange={(e) => setQ(e.target.value)} style={{ paddingLeft: 32, width: "100%" }} />
              </div>

              {CATEGORIES.map((cat) => {
                const catPlugins = shownPlugins.filter((p) => p.category === cat.label);
                if (catPlugins.length === 0) return null;
                const CatIcon = cat.icon;
                return (
                  <div key={cat.label} className="st-category-section" style={{ marginBottom: 16 }}>
                    <h3 style={{ display: "flex", alignItems: "center", gap: 6, fontSize: "0.8125rem", color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em", margin: "0 0 8px" }}>
                      <CatIcon size={14} weight="bold" /> {cat.label}
                    </h3>
                    <div className="st-drawer-grouped">
                      {catPlugins.map((p) => {
                        const installed = st.installed.includes(p.kind);
                        return (
                          <div
                            key={p.kind}
                            draggable={installed}
                            className={`st-group-card${installed ? " st-draggable" : ""}`}
                            onDragStart={(e) => { e.dataTransfer.setData("application/x-umkm-plugin", p.kind); e.dataTransfer.effectAllowed = "copy"; }}
                          >
                            <div className="d-row d-row-between" style={{ alignItems: "flex-start", width: "100%" }}>
                              <span className="st-plugin-icon" aria-hidden="true">
                                {p.kind === "chatbot" ? <ChatCircleDots weight="duotone" /> : p.kind === "product_grid" ? <Sparkle weight="duotone" /> : <PuzzlePiece weight="duotone" />}
                              </span>
                              {installed ? (
                                <button type="button" className="d-icon-btn d-icon-btn-sm" aria-label={`Tambah ${p.name}`} onClick={() => add(p.kind)} title="Tambah ke halaman">
                                  <Plus weight="bold" size={14} />
                                </button>
                              ) : (
                                <button type="button" className="d-btn d-btn-sm" style={{ padding: "2px 8px", fontSize: "0.6875rem" }} onClick={() => { update((s) => ({ ...s, installed: [...s.installed, p.kind] }), ""); toast.ok(`${p.name} terpasang.`); }}>
                                  Pasang
                                </button>
                              )}
                            </div>
                            <strong style={{ fontSize: "0.8125rem", marginTop: 4 }}>
                              {p.name} {pluginPlan(p.kind) === "pro" && <Badge tone="brand" plain>Pro</Badge>}
                            </strong>
                            <span className="d-muted d-small" style={{ fontSize: "0.6875rem", lineHeight: 1.25, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>
                              {p.desc}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </>
          )}
        </aside>

        <div className="sx-scroll sx-canvas" onPointerDown={(e) => { if (e.target === e.currentTarget) setSel(null); }}>
        <div className="st-stage" data-surface={st.surface} style={{ ["--s-primary" as string]: st.primary, ["--s-size" as string]: st.textScale ?? 1 }}>
          <div ref={stageRef} className="st-measure" />
          {device === "desktop" ? (
            <Canvas
              scale={scale}
              cells={gridCells}
              onChange={(cells) => update((s) => ({ ...s, cells: [...cells, ...s.cells.filter(isEmbed)] }), "drag")}
              onMeasure={onMeasure}
              onResizeH={onResizeH}
              onDropFile={(id, f) => void putPhoto(id, f)}
              acceptsFile={(c) => takesPhoto(c.kind)}
              selected={sel}
              onSelect={setSel}
              onRemove={remove}
              onDropPlugin={(k, at) => add(k as PluginKind, at)}
              render={(c) => renderCell(c, sel === c.id)}
              label={(c) => pluginDef(c.kind).name}
              backgroundLayer={
                <DrawingLayer
                  target="background"
                  strokes={st.drawings ?? []}
                  width={DESIGN_WIDTH}
                  height={canvasHeight}
                  activeTool={activeTab === "draw" && drawTarget === "background" ? drawTool : null}
                  currentConfig={drawConfig}
                  onAddStroke={addStroke}
                  onEraseStroke={eraseStroke}
                />
              }
              cellOverlay={(c) => (
                <DrawingLayer
                  target={c.id}
                  strokes={st.drawings ?? []}
                  width={pw(c.w)}
                  height={ph(c.h)}
                  activeTool={activeTab === "draw" && drawTarget === "cell" && sel === c.id ? drawTool : null}
                  currentConfig={drawConfig}
                  onAddStroke={addStroke}
                  onEraseStroke={eraseStroke}
                />
              )}
              toolbar={(c) => (
                <>
                  {needsUpgrade(pluginPlan(c.kind), planCode) && <span className="st-clip st-pro" title="Tidak tampil di situs sampai toko memakai paket Pro">Butuh paket Pro</span>}
                  {c.kind === "product_grid" && noPhoto > 0 && (
                    <Link className="st-clip st-nudge" href={`/products?tenant=${ctx.tenant.id}`}>{noPhoto} produk tanpa foto →</Link>
                  )}
                  {clipped[c.id] && c.autoH === false && <span className="st-clip" title="Isi lebih panjang dari kotak">Terpotong</span>}
                  {takesPhoto(c.kind) && (
                    <label className="st-tool" title={c.kind === "gallery" ? "Tambah foto" : "Ganti foto"} aria-label={c.kind === "gallery" ? "Tambah foto" : "Ganti foto"}>
                      <ImageSquare weight="bold" />
                      <input type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) void putPhoto(c.id, f); }} />
                    </label>
                  )}
                  {takesPhoto(c.kind) && (
                    <button
                      type="button"
                      className="st-tool"
                      onClick={() => {
                        const k = IMAGE_KEY[c.kind];
                        const ref = k ? (c.data[k] as string) : null;
                        if (ref && assets.url(ref)) {
                          setCropModal({ open: true, cellId: c.id, imageSrc: assets.url(ref)! });
                        } else {
                          toast.error("Belum ada foto yang dipasang untuk dipotong.");
                        }
                      }}
                      aria-label="Potong foto"
                      title="Potong foto"
                    >
                      <Crop weight="bold" />
                    </button>
                  )}
                  {c.kind === "chatbot" && (
                    <button type="button" className="st-tool" onClick={() => setChatOpen(c.id, c.chatOpen === false)} aria-label={c.chatOpen === false ? "Maksimalkan chatbot" : "Minimalkan chatbot"} title={c.chatOpen === false ? "Maksimalkan" : "Minimalkan"}>
                      {c.chatOpen === false ? <CaretUp weight="bold" /> : <CaretDown weight="bold" />}
                    </button>
                  )}
                  <button type="button" className={`st-tool${c.locked ? " st-tool-on" : ""}`} onClick={() => setCell(c.id, { locked: !c.locked }, "")} aria-pressed={!!c.locked} aria-label={c.locked ? "Buka kunci posisi" : "Kunci posisi"} title={c.locked ? "Buka kunci posisi" : "Kunci posisi — plugin lain tidak akan menggeser ini"}>
                    {c.locked ? <LockSimple weight="fill" /> : <LockSimpleOpen weight="bold" />}
                  </button>
                  <button type="button" className="st-tool" onClick={() => setFull(c.id)} aria-label={`Lihat penuh ${pluginDef(c.kind).name}`} title="Lihat penuh"><ArrowsOut weight="bold" /></button>
                </>
              )}
            />
          ) : (
            <div className="st-phone">
              {mobileOrder(gridCells).map((c) => <div key={c.id} className="st-phone-cell">{renderCell(c)}</div>)}
              <div className="st-dock"><FloatingChat cell={floating} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} /><FloatingWa cell={floatingWa} onSelect={setSel} selected={sel} /></div>
            </div>
          )}
          {st.cells.length === 0 && <p className="st-empty">Tarik plugin dari panel kiri ke sini.</p>}
        </div>
        {device === "desktop" && <div className="st-dock" style={{ ["--s-size" as string]: st.textScale ?? 1 }}><FloatingChat cell={floating} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} /><FloatingWa cell={floatingWa} onSelect={setSel} selected={sel} /></div>}
        </div>
      </div>
      </div>

      <Sheet open={!!full} onClose={() => setFull(null)} wide
        title={full ? `Pratinjau penuh — ${pluginDef(st.cells.find((c) => c.id === full)?.kind ?? "hero").name}` : ""}
        description="Seluruh isi plugin tanpa terpotong, pada lebar yang sama dengan di kanvas.">
        {full && (() => {
          const c = st.cells.find((x) => x.id === full);
          if (!c) return null;
          return (
            <div className="st-stage st-full" data-surface={st.surface} style={{ ["--s-primary" as string]: st.primary, ["--s-size" as string]: st.textScale ?? 1 }}>
              <div style={{ width: `${(isFloating(c) ? 4 : c.w) / COLS * 100}%`, minWidth: 320, margin: "0 auto" }}>{renderCell({ ...c, chatOpen: true, layout: c.kind === "whatsapp" ? "inline" : c.layout })}</div>
            </div>
          );
        })()}
      </Sheet>

      <Sheet open={checks !== null} onClose={() => setChecks(null)} title="Sebelum terbit"
        description="Website tetap bisa diterbitkan. Butir di bawah membuat toko lebih meyakinkan bagi pembeli."
        footer={<><button className="d-btn" onClick={() => setChecks(null)}>Perbaiki dulu</button><button className="d-btn d-btn-primary" onClick={() => void doPublish()} disabled={publishing}>Terbitkan sekarang</button></>}>
        <ul className="d-list" aria-label="Catatan siap terbit">
          {(checks ?? []).map((i) => (
            <li key={i.key} className="d-row d-row-between" style={{ gap: 12 }}>
              <span><WarningCircle weight="fill" aria-hidden="true" style={{ color: "var(--warn)", verticalAlign: "-3px", marginRight: 6 }} />{i.message}</span>
              <span className="d-row" style={{ gap: 6, flex: "none" }}>
                {i.cellId && <button type="button" className="d-btn d-btn-sm" onClick={() => { setSel(i.cellId!); setPanelOpen(true); setChecks(null); }}>Tunjukkan</button>}
                {i.area === "business" && <Link className="d-btn d-btn-sm" href={`/business?tenant=${ctx.tenant.id}`}>Profil bisnis</Link>}
                {i.area === "products" && <Link className="d-btn d-btn-sm" href={`/products?tenant=${ctx.tenant.id}`}>Produk</Link>}
                {i.area === "seo" && <button type="button" className="d-btn d-btn-sm" onClick={() => { setSel(null); setPanelOpen(true); setChecks(null); setTimeout(() => document.getElementById("seo-description")?.focus(), 50); }}>Isi deskripsi</button>}
              </span>
            </li>
          ))}
        </ul>
      </Sheet>

      <Sheet open={versionsOpen} onClose={() => setVersionsOpen(false)} title="Riwayat terbit"
        description="Setiap kali menerbitkan, versi lama disimpan. Pulihkan menampilkan versi itu lagi di situs; draf Anda tidak berubah.">
        {versionsOpen && <Versions base={base} canPublish={canPublish} published={srv?.published ?? null} onRestored={(v) => setSrv((x) => (x ? { ...x, published: v } : x))} />}
      </Sheet>

      <Sheet open={picking === "change"} onClose={() => setPicking(null)} wide title="Ganti template" description="Susunan dan warna diganti; Anda bisa membatalkan dengan Ctrl+Z.">
        {picking === "change" && (
          <TemplateGallery
            business={business}
            renderCell={renderCell}
            onPick={async (k) => {
              if (st.cells.length > 0 && !(await confirm({ title: "Ganti template?", body: "Susunan halaman saat ini diganti. Bisa dibatalkan dengan Urungkan (Ctrl+Z).", confirmLabel: "Ganti template" }))) return;
              applyTemplate(k);
            }}
            onCurrent={() => { update(() => fromDraft(draft), ""); setPicking(null); }}
            surfaceOf={(k) => TEMPLATES.find((t) => t.key === k)!.build({ name: business.profile.name, description: business.profile.description ?? null })}
          />
        )}
      </Sheet>

      <ImageCropModal
        open={cropModal.open}
        imageSrc={cropModal.imageSrc ?? ""}
        onClose={() => setCropModal({ open: false, cellId: null, imageSrc: null })}
        onCrop={async (file) => {
          if (cropModal.cellId) {
            await putPhoto(cropModal.cellId, file);
          }
          setCropModal({ open: false, cellId: null, imageSrc: null });
        }}
      />
    </Assets.Provider>
  );
}

/* ============================================================ Status simpan & riwayat */
function SaveBadge({ state, saved, err, onRetry }: { state: SaveState; saved: boolean; err: string | null; onRetry: () => void }) {
  if (state === "readonly") return <Badge plain>Hanya lihat</Badge>;
  if (state === "conflict" || state === "error") {
    return (
      <span className="st-save st-save-bad" role="alert" title={err ?? undefined}>
        <WarningCircle weight="fill" aria-hidden="true" />{state === "conflict" ? "Konflik — muat ulang" : "Gagal menyimpan"}
        {state === "error" && <button type="button" className="d-link d-small" onClick={onRetry}>Coba lagi</button>}
      </span>
    );
  }
  return <span className="st-save" aria-live="polite">{saved ? <><Check weight="bold" aria-hidden="true" />Draf tersimpan</> : "Menyimpan…"}</span>;
}

interface Version { version: number; created_at_unix: number; is_current: boolean }
function Versions({ base, canPublish, published, onRestored }: { base: string; canPublish: boolean; published: number | null; onRestored: (v: number) => void }) {
  const [items, setItems] = useState<Version[] | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const load = useCallback(() => api<Version[]>("GET", `${base}/website/pages/home/versions`).then(setItems, (e) => { toast.error(describe(e)); setItems([]); }), [base]);
  useEffect(() => { void load(); }, [load, published]);
  if (!items) return <Skeleton lines={4} />;
  if (items.length === 0) return <p className="d-muted">Belum pernah diterbitkan.</p>;
  return (
    <ul className="d-list">
      {items.map((v) => (
        <li key={v.version}>
          <div className="d-list-main">
            <strong>Versi {v.version} {v.is_current && <Badge tone="ok">Tampil di situs</Badge>}</strong>
            <span className="d-muted">{waktu(v.created_at_unix * 1000)}</span>
          </div>
          {!v.is_current && (
            <button type="button" className="d-btn d-btn-sm" disabled={!canPublish || busy !== null} onClick={async () => {
              if (!(await confirm({ title: `Pulihkan versi ${v.version}?`, body: "Situs publik akan menampilkan versi ini lagi. Draf yang sedang Anda ubah tidak berubah.", confirmLabel: "Pulihkan" }))) return;
              setBusy(v.version);
              try {
                await api("POST", `${base}/website/pages/home/rollback`, { json: { version: v.version } });
                toast.ok(`Situs kini menampilkan versi ${v.version}.`);
                onRestored(v.version);
                await load();
              } catch (e) { toast.error(describe(e)); } finally { setBusy(null); }
            }}>Pulihkan</button>
          )}
        </li>
      ))}
    </ul>
  );
}

/* ============================================================ Layar penuh */
function subscribeFs(cb: () => void) {
  document.addEventListener("fullscreenchange", cb);
  return () => document.removeEventListener("fullscreenchange", cb);
}
function FullscreenButton() {
  const on = useSyncExternalStore(subscribeFs, () => !!document.fullscreenElement, () => false);
  const toggle = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen?.().catch(() => toast.error("Browser ini tidak mendukung layar penuh."));
  };
  return (
    <button type="button" className="d-icon-btn" onClick={toggle} aria-pressed={on} aria-label={on ? "Keluar dari layar penuh" : "Layar penuh"} title={on ? "Keluar dari layar penuh (Esc)" : "Layar penuh"}>
      {on ? <CornersIn weight="bold" /> : <CornersOut weight="bold" />}
    </button>
  );
}

/* ============================================================ Galeri template */
function TemplateGallery({ first, business, renderCell, onPick, onCurrent, surfaceOf }: {
  first?: boolean;
  business: Business;
  renderCell: (c: StudioCell, editable?: boolean, th?: { primary: string; font: string }) => React.ReactNode;
  onPick: (key: string) => void;
  onCurrent: () => void;
  surfaceOf: (key: string) => { primary: string; surface: string; font: string; cells: StudioCell[] };
}) {
  const previews = useMemo(() => Object.fromEntries(TEMPLATES.map((t) => [t.key, surfaceOf(t.key)])), [surfaceOf]);
  return (
    <div className={first ? "st-gallery st-gallery-first" : "st-gallery"}>
      {first && (
        <div className="st-gallery-head">
          <h1>Pilih gaya awal untuk {business.profile.name}</h1>
          <p>Semua template sudah diisi nama, produk, dan jam buka toko Anda. Teks, foto, warna, dan posisi bisa diubah setelahnya.</p>
        </div>
      )}
      <div className="st-tpls">
        {TEMPLATES.map((t) => {
          const p = previews[t.key]!;
          return (
            <div key={t.key} className="st-tpl" onClick={() => onPick(t.key)}>
              <span className="st-tpl-thumb" data-surface={p.surface} style={{ ["--s-primary" as string]: p.primary }}>
                <MiniLayout cells={p.cells.filter((c) => !isEmbed(c))} render={(c) => renderCell(c, false, { primary: p.primary, font: p.font })} theme={{ primary: p.primary, font: p.font }} />
                {p.cells.some(isFloating) && <span className="st-tpl-float" style={{ background: p.primary }} aria-hidden="true"><ChatCircleDots weight="fill" /></span>}
              </span>
              <span className="st-tpl-meta">
                <span className="d-row d-row-between" style={{ flexWrap: "nowrap" }}>
                  <strong>{t.name}</strong>
                  <button type="button" className="d-btn d-btn-sm d-btn-primary" onClick={(e) => { e.stopPropagation(); onPick(t.key); }}>Pakai {t.name}</button>
                </span>
                <span>{t.desc}</span>
              </span>
            </div>
          );
        })}
      </div>
      <div className="d-row" style={{ justifyContent: "center", marginTop: 16 }}>
        <button type="button" className="d-btn d-btn-ghost" onClick={onCurrent}>Atau mulai dari susunan website saat ini</button>
      </div>
    </div>
  );
}

/** Miniatur tata letak (tanpa interaksi) di lebar desain lalu diperkecil. */
function MiniLayout({ cells, render, theme }: { cells: StudioCell[]; render: (c: StudioCell) => React.ReactNode; theme: { primary: string; font: string } }) {
  const colW = (DESIGN_WIDTH - GAP * (COLS - 1)) / COLS;
  return (
    <span className="st-mini" aria-hidden="true" style={{ ["--s-primary" as string]: theme.primary }}>
      {cells.map((c) => (
        <span key={c.id} className="st-mini-cell" style={{ transform: `translate(${c.x * (colW + GAP)}px, ${c.y * (ROW + GAP)}px)`, width: c.w * colW + (c.w - 1) * GAP, height: c.h * ROW + (c.h - 1) * GAP }}>
          {render(c)}
        </span>
      ))}
    </span>
  );
}

/* ============================================================ Inspektur */
function Inspector({ c, onBack, onChange, onWidth, onMove, onLayout, onDuplicate, onRemove, onFull, onPhoto, onCrop, aiHref, businessHref, upgradeHref }: {
  c: StudioCell;
  onBack: () => void;
  onChange: (patch: Partial<StudioCell>, key?: string) => void;
  onWidth: (w: number) => void;
  onMove: (d: "up" | "down" | "left" | "right") => void;
  onLayout: (l: string) => void;
  onDuplicate: () => void;
  onRemove: () => void;
  onFull: () => void;
  onPhoto: (f: File) => void;
  onCrop?: () => void;
  aiHref: string;
  businessHref: string;
  /** Diisi bila plugin butuh paket lebih tinggi (ADR-0019). */
  upgradeHref: string | null;
}) {
  const def = pluginDef(c.kind);
  return (
    <>
      <div className="d-row d-row-between">
        <button type="button" className="d-back" style={{ margin: 0 }} onClick={onBack}><ArrowLeft weight="bold" aria-hidden="true" />Plugin</button>
        <h2 style={{ margin: 0, fontSize: "0.9375rem", flex: 1 }}>{def.name}</h2>
        <div className="d-row">
          <button type="button" className="d-icon-btn" aria-label="Duplikat" title="Duplikat" disabled={SINGLE.includes(c.kind)} onClick={onDuplicate}><Copy weight="bold" /></button>
          <button type="button" className="d-icon-btn" aria-label="Hapus dari halaman" title="Hapus" onClick={onRemove}><Trash weight="bold" /></button>
        </div>
      </div>
      {upgradeHref && (
        <div className="d-alert d-alert-info" role="note"><div>
          Plugin ini bagian paket Pro: tetap tersimpan, tetapi tidak tampil di situs sampai toko memakai Pro.{" "}
          <Link href={upgradeHref}>Lihat paket</Link>
        </div></div>
      )}
      {c.kind !== "chatbot" && <p className="d-hint" style={{ margin: 0 }}>Klik teks di kanvas untuk mengetik langsung.{takesPhoto(c.kind) && " Seret foto dari komputer ke plugin untuk memasangnya."}</p>}
      {!isEmbed(c) && <div className="st-section">
        <label className="d-switch">
          <input type="checkbox" checked={!!c.locked} onChange={(e) => onChange({ locked: e.target.checked }, "")} />
          <span><strong><LockSimple weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> Kunci posisi</strong><span className="d-muted">Plugin ini tidak bisa diseret, dan plugin lain tidak akan menggesernya.</span></span>
        </label>
        <h3>Posisi</h3>
        {/* WCAG 2.5.7 (P9-6): semua yang bisa diseret juga bisa lewat tombol. */}
        <div className="d-seg" role="group" aria-label="Pindahkan" style={{ width: "100%", marginBottom: 12 }}>
          {([["up", "Pindah ke atas", ArrowUp], ["down", "Pindah ke bawah", ArrowDown], ["left", "Geser ke kiri", ArrowLeft], ["right", "Geser ke kanan", ArrowRight]] as const).map(([d, l, Icon]) => (
            <button key={d} type="button" style={{ flex: 1, minHeight: 32 }} aria-label={l} title={l} disabled={!!c.locked} onClick={() => onMove(d)}><Icon weight="bold" aria-hidden="true" /></button>
          ))}
        </div>
        <h3>Ukuran</h3>
        <div className="d-seg" role="group" aria-label="Lebar" style={{ width: "100%" }}>
          {([[3, "¼"], [4, "⅓"], [6, "½"], [8, "⅔"], [12, "Penuh"]] as const).map(([w, l]) => (
            <button key={w} type="button" style={{ flex: 1 }} aria-pressed={c.w === w} disabled={w < (c.minW ?? 2) || !!c.locked} onClick={() => onWidth(w)}>{l}</button>
          ))}
        </div>
        <label className="d-switch" style={{ marginTop: 10 }}>
          <input type="checkbox" checked={c.autoH !== false} onChange={(e) => onChange({ autoH: e.target.checked }, "")} />
          <span><strong>Tinggi otomatis</strong><span className="d-muted">Kotak mengikuti panjang isi sehingga tidak terpotong. Mati otomatis bila tinggi ditarik manual.</span></span>
        </label>
        <div className="d-row">
          <button type="button" className="d-btn d-btn-sm" onClick={onFull}><ArrowsOut weight="bold" aria-hidden="true" />Lihat penuh</button>
          {takesPhoto(c.kind) && (
            <label className="d-btn d-btn-sm"><ImageSquare weight="bold" aria-hidden="true" />{c.kind === "gallery" ? "Tambah foto" : "Ganti foto"}
              <input type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) onPhoto(f); }} />
            </label>
          )}
          {takesPhoto(c.kind) && onCrop && (
            <button type="button" className="d-btn d-btn-sm" onClick={onCrop} title="Potong foto">
              <Crop weight="bold" aria-hidden="true" /> Potong
            </button>
          )}
        </div>
        {takesPhoto(c.kind) && (
          <div style={{ marginTop: 12 }}>
            <div className="d-field" style={{ margin: 0, marginBottom: 8 }}>
              <span>Tampilan foto</span>
              <div className="d-seg" role="group" aria-label="Tampilan foto" style={{ width: "100%" }}>
                <button type="button" style={{ flex: 1 }} aria-pressed={((c.data.image_fit as string) ?? "cover") === "cover"} onClick={() => onChange({ data: { ...c.data, image_fit: "cover" } }, "image_fit")}>Cover</button>
                <button type="button" style={{ flex: 1 }} aria-pressed={(c.data.image_fit as string) === "contain"} onClick={() => onChange({ data: { ...c.data, image_fit: "contain" } }, "image_fit")}>Contain (Utuh)</button>
              </div>
            </div>
            <div className="d-field" style={{ margin: 0 }}>
              <span>Titik fokus (posisi)</span>
              <div className="d-seg" role="group" aria-label="Posisi foto" style={{ width: "100%" }}>
                {([["top", "Atas"], ["center", "Tengah"], ["bottom", "Bawah"]] as const).map(([pos, lbl]) => (
                  <button key={pos} type="button" style={{ flex: 1 }} aria-pressed={((c.data.image_position as string) ?? "center") === pos} onClick={() => onChange({ data: { ...c.data, image_position: pos } }, "image_pos")}>{lbl}</button>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>}
      <div className="st-section">
        <h3>Gaya</h3>
        <div className="d-seg" role="group" aria-label="Gaya" style={{ width: "100%" }}>
          {CELL_STYLES.map((s) => <button key={s.value} type="button" style={{ flex: 1 }} aria-pressed={c.style === s.value} onClick={() => onChange({ style: s.value }, "")}>{s.label}</button>)}
        </div>
        {def.layouts && (
          <>
            <h3 style={{ marginTop: 12 }}>Tata letak</h3>
            <div className="d-seg" role="group" aria-label="Tata letak" style={{ width: "100%" }}>
              {def.layouts.map((l) => <button key={l.value} type="button" style={{ flex: 1 }} aria-pressed={c.layout === l.value} onClick={() => onLayout(l.value)}>{l.label}</button>)}
            </div>
          </>
        )}
        {c.kind === "chatbot" && (
          <>
            <h3 style={{ marginTop: 12 }}>Keadaan awal</h3>
            <div className="d-seg" role="group" aria-label="Keadaan chatbot" style={{ width: "100%" }}>
              <button type="button" style={{ flex: 1 }} aria-pressed={c.chatOpen !== false} onClick={() => onChange({ chatOpen: true }, "")}><ArrowsOut weight="bold" aria-hidden="true" /> Maksimal</button>
              <button type="button" style={{ flex: 1 }} aria-pressed={c.chatOpen === false} onClick={() => onChange({ chatOpen: false }, "")}><ArrowsIn weight="bold" aria-hidden="true" /> Minimal</button>
            </div>
            {c.layout === "floating" && <p className="d-hint" style={{ marginTop: 6 }}>Gelembung bulat di pojok kanan bawah, ikut saat halaman digulir. Klik gelembungnya untuk membuka/menutup.</p>}
            {c.layout === "bar" && <p className="d-hint" style={{ marginTop: 6 }}>Bilah horizontal selebar layar di bagian bawah, ikut saat halaman digulir. Pembeli bisa langsung mengetik pertanyaan.</p>}
          </>
        )}
      </div>
      <div className="st-section">
        <h3>Isi</h3>
        {c.kind === "chatbot" ? (
          <>
            <label className="d-field"><span>Judul</span><input maxLength={40} value={String(c.data.title ?? "")} onChange={(e) => onChange({ data: { ...c.data, title: e.target.value } })} /></label>
            <label className="d-field"><span>Pesan sambutan</span><textarea rows={3} maxLength={300} value={String(c.data.welcome ?? "")} onChange={(e) => onChange({ data: { ...c.data, welcome: e.target.value } })} /></label>
            <p className="d-hint">Jawaban memakai pengaturan di menu <Link href={aiHref}>Asisten AI</Link>.</p>
          </>
        ) : c.kind === "text" ? (
          <>
            <label className="d-field"><span>Judul (opsional)</span><input maxLength={120} value={String(c.data.title ?? "")} onChange={(e) => onChange({ data: { ...c.data, title: e.target.value } })} /></label>
            <label className="d-field"><span>Isi</span><textarea rows={6} maxLength={5000} value={String(c.data.body ?? "")} onChange={(e) => onChange({ data: { ...c.data, body: e.target.value } })} /><span className="d-hint">Baris kosong memisahkan paragraf.</span></label>
          </>
        ) : c.kind === "whatsapp" ? (
          <>
            <label className="d-field"><span>Tulisan tombol</span><input maxLength={40} value={String(c.data.label ?? "")} onChange={(e) => onChange({ data: { ...c.data, label: e.target.value } })} /></label>
            <label className="d-field"><span>Pesan awal (opsional)</span><textarea rows={3} maxLength={300} value={String(c.data.message ?? "")} onChange={(e) => onChange({ data: { ...c.data, message: e.target.value } })} /><span className="d-hint">Terisi otomatis saat pembeli membuka WhatsApp.</span></label>
            <p className="d-hint">Nomor diambil dari WhatsApp (atau telepon) di <Link href={businessHref}>Profil bisnis</Link>.</p>
          </>
        ) : (
          <>
            {FIELDS[c.kind as SectionType].map((f) => (
              <FieldInput key={f.key} f={f} data={c.data} disabled={false} onChange={(d) => onChange({ data: d })} />
            ))}
            {/* WCAG 1.1.1 (P9-6): field v2 — hanya di Studio; editor v1 tidak mengenalnya. */}
            {c.kind in IMAGE_KEY && (
              <label className="d-field"><span>Teks alternatif foto</span>
                <input maxLength={160} placeholder="mis. Segelas kopi susu gula aren di meja kayu" value={String(c.data.image_alt ?? "")}
                  onChange={(e) => onChange({ data: { ...c.data, image_alt: e.target.value } })} />
                <span className="d-hint">Dibacakan untuk pembeli tunanetra dan membantu Google memahami foto. Kosongkan bila foto hanya hiasan.</span>
              </label>
            )}
          </>
        )}
      </div>
    </>
  );
}

/* ============================================================ WhatsApp melayang */
/** Pratinjau tombol WhatsApp melayang (pojok kanan bawah, di atas gelembung chat seperti situs publik). */
function FloatingWa({ cell, onSelect, selected }: { cell: StudioCell | undefined; onSelect: (id: string) => void; selected: string | null }) {
  if (!cell) return null;
  const label = String(cell.data.label || "Pesan via WhatsApp");
  return (
    <button type="button" className={`st-wa-float${selected === cell.id ? " st-selected" : ""}`} aria-label={`${label} (tombol WhatsApp melayang) — pilih untuk mengatur`} title={label} onClick={() => onSelect(cell.id)}>
      <WhatsappLogo weight="fill" aria-hidden="true" />
    </button>
  );
}

/* ============================================================ Chatbot */
/** Chatbot melayang: gelembung di pojok kanan bawah panggung; klik untuk buka/tutup. */
function FloatingChat({ cell, primary, onToggle, onSelect, selected }: {
  cell: StudioCell | undefined; primary: string; onToggle: (id: string, open: boolean) => void; onSelect: (id: string) => void; selected: string | null;
}) {
  if (!cell) return null;
  const open = cell.chatOpen !== false;
  const title = String(cell.data.title ?? "Tanya kami");
  const welcome = String(cell.data.welcome ?? "");
  if (cell.layout === "bar") {
    return (
      <div className={`st-chatbar${selected === cell.id ? " st-selected" : ""}`} style={{ ["--c" as string]: primary }} onPointerDown={() => onSelect(cell.id)}>
        {open && (
          <div className="st-chatbar-panel">
            <ChatPreview primary={primary} title={title} welcome={welcome} open onToggle={() => onToggle(cell.id, false)} />
          </div>
        )}
        <div className="st-chatbar-bar">
          <span className="st-chatbar-who"><span className="st-chat-dot" aria-hidden="true" />{title}<em>Asisten AI · online</em></span>
          <button type="button" className="st-chatbar-input" style={{ border: 0, textAlign: "left", font: "inherit", cursor: "pointer" }} onClick={() => onToggle(cell.id, true)}>
            {welcome || "Tulis pertanyaan…"}
          </button>
          <button type="button" className="st-chatbar-send" onClick={() => onToggle(cell.id, !open)} aria-label={open ? "Minimalkan chatbot" : "Buka chatbot"}>
            {open ? <CaretDown weight="bold" /> : <ChatCircleDots weight="fill" />}
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className={`st-float${selected === cell.id ? " st-selected" : ""}`} onPointerDown={() => onSelect(cell.id)}>
      {open && (
        <div className="st-float-panel">
          <ChatPreview primary={primary} title={title} welcome={welcome} open onToggle={() => onToggle(cell.id, false)} />
        </div>
      )}
      <button type="button" className="st-float-bubble" style={{ ["--c" as string]: primary }} onClick={() => onToggle(cell.id, !open)} aria-label={open ? "Minimalkan chatbot" : "Buka chatbot"}>
        {open ? <X weight="bold" /> : <ChatCircleDots weight="fill" />}
      </button>
    </div>
  );
}

/** Pratinjau chatbot (statis di editor; di situs publik memakai widget chat sungguhan). */
function ChatPreview({ primary, title, welcome, open, onToggle }: { primary: string; title: string; welcome: string; open: boolean; onToggle: () => void }) {
  return (
    <div className={`st-chat${open ? "" : " st-chat-min"}`} style={{ ["--c" as string]: primary }}>
      <div className="st-chat-head">
        <span className="st-chat-dot" aria-hidden="true" />{title}<em>Asisten AI · online</em>
        <button type="button" data-interactive className="st-chat-toggle" onClick={onToggle} aria-label={open ? "Minimalkan chatbot" : "Maksimalkan chatbot"} title={open ? "Minimalkan" : "Maksimalkan"}>
          {open ? <CaretDown weight="bold" /> : <CaretUp weight="bold" />}
        </button>
      </div>
      {open && (
        <>
          <div className="st-chat-log">
            <p className="st-chat-msg">{welcome}</p>
            <p className="st-chat-msg st-chat-me">Kopi susu masih ada?</p>
            <p className="st-chat-msg">Masih, kak! Kopi Susu Gula Aren tersedia ukuran Regular dan Large 😊</p>
          </div>
          <div className="st-chat-input"><span>Tulis pesan…</span><b aria-hidden="true">➤</b></div>
        </>
      )}
    </div>
  );
}
