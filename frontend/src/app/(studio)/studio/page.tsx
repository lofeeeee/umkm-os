"use client";

// Studio website (ADR-0017). Satu kanvas = renderer situs asli + manipulasi langsung (LiveCanvas).
// - Mulai dari galeri template yang langsung diisi data toko.
// - Klik blok untuk memilih, klik teksnya untuk mengetik, seret blok untuk memindah, tarik pegangan
//   kanan/bawah untuk mengubah ukuran; seret foto dari komputer ke blok.
// - Undo/Redo (Ctrl+Z / Ctrl+Shift+Z) + simpan otomatis ke draf server.

import Link from "@/ui/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  LockSimple, LockSimpleOpen, ArrowSquareOut, ClockCounterClockwise, WarningCircle, ArrowClockwise, ArrowCounterClockwise, ArrowLeft, Check, Copy, Desktop, DeviceMobile, DeviceTablet, ImageSquare, MagnifyingGlass, Plus, SidebarSimple, SquaresFour, Trash, X, Crop, CaretDown, Files,
} from "@phosphor-icons/react/ssr";

import { useUploads } from "@/dashboard/assets";
import { Assets } from "@/dashboard/editor/fields";
import { WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { Site, type GridProduct } from "@/renderer/Site";
import { normalizePageV2 } from "@/renderer/normalize";
import { ChatPreview } from "@/chat/widget";
import { TextBlock, WhatsAppButton } from "@/renderer/StudioSite";
import { fontStack, primaryVars, safeColor } from "@/renderer/safe";
import { FONT_PRESETS } from "@/renderer/sections";
import type { Business,PageContent,SectionType } from "@/renderer/types";
import { DEVICE_LABEL, DEVICE_WIDTH, type DeviceViewport } from "@/studio/DeviceFrame";
import { COLS, insert, moveBy, place } from "@/studio/grid";
import { commit, redo, start, undo, type History } from "@/studio/history";
import { PLUGINS, needsUpgrade, pluginDef, pluginPlan, type PluginKind } from "@/studio/plugins";
import { PluginIcon } from "@/studio/PluginIcon";
import { LiveCanvas, PLUGIN_MIME, setPaletteDrag } from "@/studio/LiveCanvas";
import { readiness, type Issue } from "@/studio/readiness";
import { TEMPLATES } from "@/studio/templates";
import { fromDoc, isV2, toDoc } from "@/studio/doc";
import { IconRail, type StudioTab } from "@/studio/IconRail";
import { PagesPanel, type PageItem } from "@/studio/PagesPanel";
import { ThemePanel, type ThemeConfig } from "@/studio/ThemePanel";
import { DrawPanel, type DrawConfig, type DrawTool, type DrawTarget } from "@/studio/DrawPanel";
import type { DrawingStroke } from "@/studio/DrawingLayer";
import { ImageCropModal } from "@/ui/ImageCropModal";
import "@/studio/studio.css";
import { ApiError, describe } from "@/lib/api";
import { confirm, Sheet } from "@/ui/dialog";
import { dismissToast, toast } from "@/ui/toast";
import { t as tx } from "@/i18n";
import { Tr } from "@/i18n/react";
import {
  type StudioCell, type Studio, type SaveState, CATEGORIES, CATEGORY_LABEL, EMPTY_BUSINESS, storageKey, newId, isFloating, isEmbed, SINGLE, IMAGE_KEY, takesPhoto, cellFor, fromDraft, withInstalled, setPath, setPathOr,
} from "@/studio/editor-model";
import { FullscreenButton, SaveBadge, Versions } from "@/studio/EditorChrome";
import { TemplateGallery } from "@/studio/TemplateGallery";
import { Inspector } from "@/studio/Inspector";
import { RowMenu } from "@/ui/menu";

/** Layar sempit (ponsel): Studio dibuka dengan kanvas, bukan panel. */
const narrowScreen = () => typeof window !== "undefined" && window.matchMedia("(max-width: 720px)").matches;

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
  const [q, setQ] = useState("");
  /** Perangkat pratinjau: lebar BINGKAI menentukan breakpoint @container, bukan lebar jendela. */
  const [device, setDevice] = useState<DeviceViewport>(() => (narrowScreen() ? "mobile" : "desktop"));
  /** Perbesaran pratinjau: bingkai selalu selebar layar asli perangkat, lalu diperkecil agar muat. */
  const [zoom, setZoom] = useState(1);
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
  // Ponsel: kanvas lebih dulu (panel tertutup) dengan pratinjau ponsel — banyak pemilik UMKM hanya punya HP.
  const [panelOpen, setPanelOpen] = useState(() => !narrowScreen());
  const [activeTab, setActiveTab] = useState<StudioTab>("elements");
  const [pages, setPages] = useState<PageItem[]>([]);
  const [activeSlug, setActiveSlug] = useState("home");
  const [drawTool, setDrawTool] = useState<DrawTool>(null);
  const [drawConfig, setDrawConfig] = useState<DrawConfig>({ color: "#EF4444", size: 4, opacity: 1 });
  const [drawTarget, setDrawTarget] = useState<DrawTarget>("cell");
  const [cropModal, setCropModal] = useState<{ open: boolean; cellId: string | null; imageSrc: string | null }>({ open: false, cellId: null, imageSrc: null });
  /** Status chatbot sebenarnya dari `/ai/config`: pratinjau tidak boleh menjanjikan chat yang tidak aktif. */
  const [chatReady, setChatReady] = useState<boolean | null>(null);

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
      if (!key) throw new Error(tx("Tidak ada templat website."));
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
      // Status chat nyata: plugin Chatbot di kanvas hanya tampil di publik bila chatbot diaktifkan
      // di menu Asisten AI (mode "Hanya staf" tidak butuh penyedia AI).
      void api<{ config: { enabled: boolean; mode: string; primary_provider_id: string | null } }>("GET", `${base}/ai/config`)
        .then((x) => setChatReady(x.config.enabled && (x.config.mode === "human_only" || !!x.config.primary_provider_id)))
        .catch(() => setChatReady(null));
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
          toast.info(tx("Susunan dari prototipe Studio dipakai dan sedang disimpan ke server."));
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
        setSaveErr(tx("Draf diubah di tempat lain (tab atau perangkat lain). Muat ulang halaman untuk melihat versi terbaru."));
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
      toast.ok(tx("Halaman {0} dibuat.", title));
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
      toast.ok(tx("Halaman dihapus."));
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
    hideBadge: st?.hideBadge ?? false,
    backgroundImage: st?.backgroundImage,
    backgroundSize: st?.backgroundSize,
    backgroundPosition: st?.backgroundPosition,
    backgroundOpacity: st?.backgroundOpacity ?? 0,
  }), [st]);

  const updateTheme = useCallback((patch: Partial<ThemeConfig>) => {
    update((s) => ({ ...s, ...patch }), "theme");
  }, [update]);

  const uploadBg = useCallback(async (file: File): Promise<string | null> => {
    const id0 = toast.info(tx("Mengunggah foto latar…"));
    const u = await assets.upload(file);
    dismissToast(id0);
    if (!u) return null;
    update((s) => ({ ...s, backgroundImage: u.ref }), "bg");
    toast.ok(tx("Foto latar dipasang."));
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
    update((s) => ({ ...s, drawings: [] }), "clear_draw");
    toast.ok(tx("Semua coretan dihapus. Tekan Ctrl+Z untuk membatalkan."));
  }, [update]);

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
    const issues = readiness({ cells: st.cells, business, productCount: rawProducts.length, seo: st.seo, chatReady });
    if (issues.length > 0) { setChecks(issues); return; }
    void doPublish();
  }

  async function doPublish() {
    if (!srv) return;
    setChecks(null);
    setPublishing(true);
    try {
      const rev = await saveNow();
      if (rev === null) { toast.error(tx("Draf belum tersimpan, jadi belum bisa diterbitkan.")); return; }
      const r = await api<{ version: number }>("POST", `${base}/website/pages/${activeSlug}/publish`, { json: { revision: rev } });
      setSrv((x) => (x ? { ...x, published: r.version ?? x.published } : x));
      toast.ok(tx("Website diterbitkan. Pengunjung melihat versi baru dalam ±1 menit."));
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

  /* ---------------- Aksi plugin ---------------- */
  const setCell = useCallback((id: string, patch: Partial<StudioCell>, key = `cell:${id}`) => {
    update((s) => ({ ...s, cells: s.cells.map((c) => (c.id === id ? { ...c, ...patch } : c)) }), key);
  }, [update]);
  const setChatOpen = useCallback((id: string, open: boolean) => setCell(id, { chatOpen: open }, ""), [setCell]);

  function add(kind: PluginKind, at?: { x: number; y: number }) {
    if (SINGLE.includes(kind) && st?.cells.some((c) => c.kind === kind)) { toast.info(tx("{0} sudah ada di halaman.", pluginDef(kind).name)); return; }
    const c = cellFor(kind);
    update((s) => ({ ...s, installed: s.installed.includes(kind) ? s.installed : [...s.installed, kind], cells: insert(s.cells, c, at) as StudioCell[] }), "");
    setSel(c.id);
  }
  function remove(id: string) {
    update((s) => ({ ...s, cells: s.cells.filter((c) => c.id !== id) }), "");
    setSel(null);
    toast.info(tx("Blok dihapus. Tekan Ctrl+Z untuk membatalkan."));
  }
  async function putPhoto(id: string, file: File) {
    const id0 = toast.info(tx("Mengunggah foto…"));
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
    toast.ok(tx("Foto dipasang."));
  }
  function applyTemplate(key: string) {
    const t = TEMPLATES.find((x) => x.key === key)!;
    const r = t.build({ name: business.profile.name, description: business.profile.description ?? null });
    const next = withInstalled({ ...r, installed: st?.installed ?? [] });
    if (picking === "first") setHist(start(next));
    else update(() => next, "");
    setPicking(null);
    setSel(null);
    toast.ok(tx("Templat {0} dipakai. Klik teks mana saja untuk mengubahnya.", t.name));
  }

  /* ---------------- Render plugin (galeri template & pratinjau penuh) ---------------- */
  const primary = st?.primary ?? "#4F46E5";
  const font = st?.font ?? FONT_PRESETS[0]!;
  const theme = useMemo(() => ({ primary, font }), [primary, font]);

  const renderCell = useCallback((c: StudioCell, _editable = false, th: { primary: string; font: string } = theme) => (
    <div className={`st-content st-style-${c.style}${c.layout ? ` st-layout-${c.layout}` : ""}`}>
      {c.kind === "chatbot" ? (
        <ChatPreview variant="inline" primary={safeColor(th.primary)} title={String(c.data.title ?? "")} welcome={String(c.data.welcome ?? "")} open={c.chatOpen !== false} ready={chatReady} />
      ) : c.kind === "text" || c.kind === "whatsapp" ? (
        <div className="site s-solo" style={{ ...primaryVars(th.primary), ["--s-font" as string]: fontStack(th.font) }}>
          <section className="s"><div className="s-inner">
            {c.kind === "text" ? <TextBlock b={c} /> : <WhatsAppButton b={c} business={business} />}
          </div></section>
        </div>
      ) : (
        <Site
          content={{ schema_version: 1, template: "studio", theme: th, sections: [{ id: c.id, type: c.kind as SectionType, enabled: true, data: c.data }] }}
          business={business}
          products={products.length ? products : undefined}
          assetUrl={assets.url}
        />
      )}
    </div>
  ), [theme, business, products, assets.url, chatReady]);

  const shownPlugins = useMemo(() => {
    const t = q.trim().toLowerCase();
    return PLUGINS.filter((p) => !t || `${p.name} ${p.desc} ${p.keywords} ${tx(p.keywords)} ${p.category} ${CATEGORY_LABEL[p.category]}`.toLowerCase().includes(t));
  }, [q]);

  const backHref = `/website?tenant=${ctx.tenant.id}`;
  /**
   * Dokumen yang sama dengan yang diterbitkan, dirender oleh renderer yang sama (`StudioSite`) di dalam
   * LiveCanvas: hasil di kanvas persis seperti situs publik; hanya pelapis seleksi/pegangan yang berbeda.
   */
  const previewDoc = useMemo(() => normalizePageV2(st ? toDoc(st, srv?.template ?? "studio") : null), [st, srv?.template]);

  const onEditBlock = useCallback((id: string, path: string, value: string) => {
    setHist((h) => {
      if (!h) return h;
      const c = h.present.cells.find((x) => x.id === id);
      if (!c) return h;
      const r = setPath(c.kind, c.data, path, value);
      if (typeof r === "string") { toast.error(r); return h; }
      return commit(h, { ...h.present, cells: h.present.cells.map((x) => (x.id === id ? { ...x, data: setPathOr(x, path, value) } : x)) }, "", Date.now());
    });
  }, []);

  /** Mode kuas: alat dipilih di panel Coretan dan panel itu sedang terbuka. */
  const drawActive = activeTab === "draw" && panelOpen && drawTool !== null;

  /* Pintasan keyboard untuk blok terpilih: panah = geser, Delete = hapus, Esc = lepas / keluar kuas. */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable]")) return;
      if (e.key === "Escape" && drawTool) { setDrawTool(null); return; }
      if (!sel || e.metaKey || e.ctrlKey || e.altKey) return;
      const dir = ({ ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" } as const)[e.key as "ArrowUp"];
      if (dir) {
        e.preventDefault();
        update((s) => ({ ...s, cells: [...moveBy(s.cells.filter((x) => !isEmbed(x)), sel, dir) as StudioCell[], ...s.cells.filter(isEmbed)] }), `move:${sel}`);
      } else if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        remove(sel);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!st || !hist) {
    return (
      <div className="sx">
        <header className="sx-top"><Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" />{tx("Website")}</Link><strong>{tx("Studio")}</strong></header>
        <div className="sx-scroll"><div className="u-p-8"><span className="d-skel d-skel-block" style={{ height: 480 }} /></div></div>
      </div>
    );
  }

  if (picking === "first") {
    return (
      <Assets.Provider value={assets}>
        <div className="sx">
        <header className="sx-top">
          <Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" />{tx("Website")}</Link>
          <span className="sx-brand"><span className="d-avatar" aria-hidden="true">{(ctx.tenant.name.trim()[0] ?? "T").toUpperCase()}</span>{tx("Studio")}</span>
          <span className="sx-spacer" />
          <FullscreenButton />
        </header>
        <div className="sx-scroll" style={{ padding: "32px 24px" }}>
        <TemplateGallery
          first
          business={business}
          renderCell={renderCell}
          onPick={applyTemplate}
          onCurrent={() => { setPicking(null); toast.ok(tx("Memakai susunan website Anda saat ini.")); }}
          surfaceOf={(k) => TEMPLATES.find((t) => t.key === k)!.build({ name: business.profile.name, description: business.profile.description ?? null })}
        />
        </div>
        </div>
      </Assets.Provider>
    );
  }

  const selected = st.cells.find((c) => c.id === sel) ?? null;
  const gridCells = st.cells.filter((c) => !isEmbed(c));
  const activePage = pages.find((p) => p.slug === activeSlug);

  const cropOf = (c: StudioCell) => {
    const k = IMAGE_KEY[c.kind];
    const ref = k ? (c.data[k] as string) : null;
    if (ref && assets.url(ref)) setCropModal({ open: true, cellId: c.id, imageSrc: assets.url(ref)! });
    else toast.error(tx("Belum ada foto yang dipasang untuk dipotong."));
  };
  const duplicate = (c: StudioCell) => {
    const copy = { ...c, id: newId(c.kind), locked: undefined };
    update((s) => ({ ...s, cells: insert(s.cells, copy, { x: c.x, y: c.y + c.h }) as StudioCell[] }), "");
    setSel(copy.id);
  };

  /** Bilah aksi di atas blok terpilih (di kanvas). */
  const blockToolbar = (id: string) => {
    const c = st.cells.find((x) => x.id === id);
    if (!c) return null;
    return (
      <>
        {needsUpgrade(pluginPlan(c.kind), planCode) && <span className="lc-chip lc-chip-pro" title={tx("Tidak tampil di situs sampai toko memakai paket Pro")}>{tx("Pro")}</span>}
        {c.kind === "product_grid" && noPhoto > 0 && (
          <Link className="lc-chip lc-chip-warn" href={`/products?tenant=${ctx.tenant.id}`}>{noPhoto}{" "}{tx("produk tanpa foto")}</Link>
        )}
        {takesPhoto(c.kind) && (
          <label className="lc-tool" title={c.kind === "gallery" ? tx("Tambah foto") : tx("Ganti foto")} aria-label={c.kind === "gallery" ? tx("Tambah foto") : tx("Ganti foto")}>
            <ImageSquare weight="bold" />
            <input type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) void putPhoto(c.id, f); }} />
          </label>
        )}
        {c.kind in IMAGE_KEY && (
          <button type="button" className="lc-tool" onClick={() => cropOf(c)} aria-label={tx("Potong foto")} title={tx("Potong foto")}><Crop weight="bold" /></button>
        )}
        <button type="button" className={`lc-tool${c.locked ? " lc-tool-on" : ""}`} onClick={() => setCell(c.id, { locked: !c.locked }, "")} aria-pressed={!!c.locked}
          aria-label={c.locked ? tx("Buka kunci posisi") : tx("Kunci posisi")} title={c.locked ? tx("Buka kunci posisi") : tx("Kunci posisi")}>
          {c.locked ? <LockSimple weight="fill" /> : <LockSimpleOpen weight="bold" />}
        </button>
        {!SINGLE.includes(c.kind) && <button type="button" className="lc-tool" onClick={() => duplicate(c)} aria-label={tx("Duplikat")} title={tx("Duplikat")}><Copy weight="bold" /></button>}
        <button type="button" className="lc-tool lc-tool-danger" onClick={() => remove(c.id)} aria-label={tx("Hapus {0}", pluginDef(c.kind).name)} title={tx("Hapus (Delete)")}><Trash weight="bold" /></button>
      </>
    );
  };

  const selectTab = (t: StudioTab) => {
    setSel(null);
    // Menutup panel hanya bila tab itu memang sedang tampil (bukan tertutup inspektur blok terpilih).
    if (activeTab === t && panelOpen && !sel) setPanelOpen(false);
    else { setActiveTab(t); setPanelOpen(true); }
  };

  return (
    <Assets.Provider value={assets}>
      <div className="sx">
      <header className="sx-top">
        <Link className="d-btn d-btn-ghost d-btn-sm" href={backHref}><ArrowLeft weight="bold" aria-hidden="true" /><span className="sx-hide-sm">{tx("Website")}</span></Link>
        <button type="button" className="d-icon-btn sx-hide-sm" onClick={() => setPanelOpen(!panelOpen)} aria-pressed={panelOpen} aria-label={panelOpen ? tx("Sembunyikan panel") : tx("Tampilkan panel")} title={panelOpen ? tx("Sembunyikan panel") : tx("Tampilkan panel")}>
          <SidebarSimple weight={panelOpen ? "fill" : "bold"} />
        </button>
        <span className="sx-sep" aria-hidden="true" />
        <button type="button" className="sx-page" onClick={() => selectTab("pages")} title={tx("Ganti atau tambah halaman")}>
          <Files weight="duotone" aria-hidden="true" />
          <span className="sx-page-name">{activePage?.title || (activeSlug === "home" ? tx("Beranda") : activeSlug)}</span>
          <span className="sx-page-path">{activeSlug === "home" ? "/" : `/${activeSlug}`}</span>
          <CaretDown weight="bold" aria-hidden="true" />
        </button>
        <SaveBadge state={saveState} saved={saved} err={saveErr} onRetry={() => { setSaveState("idle"); void saveNow(); }} />
        <span className="sx-spacer" />
        <div className="sx-group" role="group" aria-label={tx("Riwayat ubahan")}>
          <button type="button" className="d-icon-btn" onClick={doUndo} disabled={hist.past.length === 0} aria-label={tx("Urungkan (Ctrl+Z)")} title={tx("Urungkan (Ctrl+Z)")}><ArrowCounterClockwise weight="bold" /></button>
          <button type="button" className="d-icon-btn sx-hide-sm" onClick={doRedo} disabled={hist.future.length === 0} aria-label={tx("Ulangi (Ctrl+Shift+Z)")} title={tx("Ulangi (Ctrl+Shift+Z)")}><ArrowClockwise weight="bold" /></button>
        </div>
        <div className="sx-devices" role="group" aria-label={tx("Perangkat pratinjau")}>
          {([["desktop", Desktop], ["tablet", DeviceTablet], ["mobile", DeviceMobile]] as const).map(([d, Icon]) => (
            <button key={d} type="button" aria-pressed={device === d} onClick={() => setDevice(d)} title={DEVICE_LABEL[d]} aria-label={DEVICE_LABEL[d]}>
              <Icon weight={device === d ? "fill" : "bold"} aria-hidden="true" />
            </button>
          ))}
        </div>
        <span className="sx-zoom" title={tx("Pratinjau {0} selebar layar asli ({1} px), diperkecil agar muat", DEVICE_LABEL[device], DEVICE_WIDTH[device])}>{Math.round(zoom * 100)}%</span>
        <span className="sx-sep" aria-hidden="true" />
        <button type="button" className="d-btn d-btn-sm d-btn-ghost sx-hide-sm" onClick={() => setPicking("change")}><SquaresFour weight="bold" aria-hidden="true" /><span className="sx-hide-md">{tx("Templat")}</span></button>
        <button type="button" className="d-btn d-btn-sm d-btn-ghost sx-hide-sm" onClick={() => setVersionsOpen(true)}><ClockCounterClockwise weight="bold" aria-hidden="true" /><span className="sx-hide-md">{tx("Riwayat")}</span></button>
        <span className="sx-hide-sm"><FullscreenButton /></span>
        {ctx.tenant.primary_hostname && srv?.published && (
          <a className="d-btn d-btn-sm d-btn-ghost sx-hide-sm" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noopener noreferrer"><ArrowSquareOut weight="bold" aria-hidden="true" /><span className="sx-hide-md">{tx("Lihat situs")}</span></a>
        )}
        {/* Ponsel: aksi sekunder dalam satu menu agar tombol Terbitkan selalu terlihat. */}
        <span className="sx-show-sm">
          <RowMenu label={tx("Aksi lain")} items={[
            { label: tx("Ganti templat"), run: () => setPicking("change") },
            { label: tx("Riwayat versi"), run: () => setVersionsOpen(true) },
            ...(ctx.tenant.primary_hostname && srv?.published ? [{ label: tx("Lihat situs"), run: () => { window.open(`https://${ctx.tenant.primary_hostname}`, "_blank", "noopener,noreferrer"); } }] : []),
          ]} />
        </span>
        <button type="button" className="d-btn d-btn-sm d-btn-primary" onClick={publish} disabled={!canPublish || publishing || saveState === "conflict"} title={canPublish ? tx("Terbitkan ke situs publik") : tx("Butuh izin terbitkan website")}>
          {publishing ? tx("Menerbitkan…") : srv?.published ? <><span className="sx-hide-sm">{tx("Terbitkan perubahan")}</span><span className="sx-show-sm">{tx("Terbitkan")}</span></> : tx("Terbitkan")}
        </button>
      </header>

      <div className="sx-body" data-panel={panelOpen ? "open" : "closed"}>
        <IconRail activeTab={selected ? null : activeTab} onSelectTab={selectTab} />
        <aside className="st-panel sx-panel" aria-label={selected ? tx("Pengaturan blok") : activeTab === "pages" ? tx("Halaman") : activeTab === "theme" ? tx("Desain") : activeTab === "draw" ? tx("Coretan") : tx("Blok")} hidden={!panelOpen}>
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
              onDuplicate={() => duplicate(selected)}
              onRemove={() => remove(selected.id)}
              onFull={() => setFull(selected.id)}
              onPhoto={(f) => void putPhoto(selected.id, f)}
              onCrop={() => cropOf(selected)}
              aiHref={`/ai?tenant=${ctx.tenant.id}`}
              businessHref={`/business?tenant=${ctx.tenant.id}`}
              chatReady={chatReady}
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
            <ThemePanel config={themeConfig} onChange={updateTheme} onUploadBackground={uploadBg} assetUrl={assets.url}
              isPro={planCode === "pro"} onUpgradeClick={() => window.open(`/subscription?tenant=${ctx.tenant.id}`, "_self")} />
          ) : activeTab === "draw" ? (
            <DrawPanel
              activeTool={drawTool}
              onSelectTool={(t) => { setDrawTool(t); if (t) setSel(null); }}
              target={drawTarget}
              onSelectTarget={setDrawTarget}
              config={drawConfig}
              onChangeConfig={setDrawConfig}
              primaryColor={st.primary}
              strokeCount={(st.drawings ?? []).length}
              onClearStrokes={clearStrokes}
            />
          ) : (
            <div className="st-catalog">
              <div className="st-panel-head">
                <h2>{tx("Tambah blok")}</h2>
                <p>{tx("Klik untuk menambah di bawah halaman, atau seret ke posisi yang Anda mau.")}</p>
              </div>
              <label className="st-search">
                <MagnifyingGlass aria-hidden="true" weight="bold" />
                <input type="search" aria-label={tx("Cari blok")} placeholder={tx("Cari: promo, wa, banner…")} value={q} onChange={(e) => setQ(e.target.value)} />
                {q && <button type="button" className="st-search-x" onClick={() => setQ("")} aria-label={tx("Hapus pencarian")}><X weight="bold" /></button>}
              </label>
              {shownPlugins.length === 0 && <p className="d-muted d-small"><Tr s="Tidak ada blok yang cocok dengan “{0}”." a={[q]} /></p>}
              {CATEGORIES.map((cat) => {
                const list = shownPlugins.filter((p) => p.category === cat);
                if (list.length === 0) return null;
                return (
                  <section key={cat} className="st-cat">
                    <h3>{CATEGORY_LABEL[cat]}</h3>
                    <div className="st-cards">
                      {list.map((p) => {
                        const onPage = st.cells.some((c) => c.kind === p.kind);
                        const single = SINGLE.includes(p.kind) && onPage;
                        return (
                          <button
                            key={p.kind}
                            type="button"
                            draggable={!single}
                            className="st-card"
                            aria-label={tx("Tambah {0}", p.name)}
                            aria-disabled={single || undefined}
                            title={single ? tx("{0} sudah ada di halaman ini", p.name) : tx("Tambah {0} — atau seret ke kanvas", p.name)}
                            onClick={() => add(p.kind)}
                            onDragStart={(e) => { e.dataTransfer.setData(PLUGIN_MIME, p.kind); e.dataTransfer.effectAllowed = "copy"; setPaletteDrag({ kind: p.kind, w: p.w }); }}
                            onDragEnd={() => setPaletteDrag(null)}
                          >
                            <span className="st-card-icon"><PluginIcon kind={p.kind} size={20} /></span>
                            <span className="st-card-text">
                              <strong>{p.name}{pluginPlan(p.kind) === "pro" && <em className="st-pro">{tx("Pro")}</em>}</strong>
                              <span>{p.desc}</span>
                            </span>
                            <span className="st-card-add" aria-hidden="true">{single ? <Check weight="bold" /> : <Plus weight="bold" />}</span>
                          </button>
                        );
                      })}
                    </div>
                  </section>
                );
              })}
            </div>
          )}
        </aside>

        <main className="sx-canvas" aria-label={tx("Kanvas halaman")}>
          {drawActive && (
            <div className="sx-drawbar" role="status">
              <span className="sx-drawbar-dot" style={{ background: drawTool === "eraser" ? "#71717a" : drawConfig.color }} />
              {drawTool === "brush" ? (drawTarget === "cell" ? tx("Kuas: coretan menempel pada blok yang Anda gambar") : tx("Kuas: menggambar di latar halaman")) : tx("Penghapus: sapukan pada coretan")}
              <button type="button" className="d-btn d-btn-sm" onClick={() => setDrawTool(null)}>{tx("Selesai")}{" "}<kbd>{tx("Esc")}</kbd></button>
            </div>
          )}
          <LiveCanvas
            device={device}
            doc={previewDoc}
            business={business}
            products={products}
            assetUrl={assets.url}
            pages={pages.map((p) => ({ slug: p.slug, title: p.title, current: p.slug === activeSlug }))}
            cells={gridCells}
            selectedId={sel}
            onSelect={setSel}
            onEditBlock={onEditBlock}
            onCellsChange={(cells) => update((s) => ({ ...s, cells: [...cells, ...s.cells.filter(isEmbed)] }), "drag")}
            onDropPlugin={(k, at) => add(k as PluginKind, at)}
            acceptsFile={(id) => { const c = st.cells.find((x) => x.id === id); return !!c && takesPhoto(c.kind); }}
            onDropFile={(id, f) => void putPhoto(id, f)}
            chatReady={chatReady}
            onChatOpen={setChatOpen}
            toolbar={blockToolbar}
            draw={{ tool: drawActive ? drawTool : null, config: drawConfig, target: drawTarget }}
            strokes={st.drawings ?? []}
            onAddStroke={addStroke}
            onEraseStroke={eraseStroke}
            onHint={(m) => toast.info(m)}
            onZoom={setZoom}
          />
          {st.cells.length === 0 && (
            <div className="sx-empty">
              <strong>{tx("Halaman ini masih kosong")}</strong>
              <span>{tx("Tambahkan blok dari panel kiri, atau pilih templat siap pakai.")}</span>
              <span className="d-row u-justify-center">
                <button type="button" className="d-btn d-btn-sm d-btn-primary" onClick={() => selectTab("elements")}><Plus weight="bold" aria-hidden="true" />{tx("Tambah blok")}</button>
                <button type="button" className="d-btn d-btn-sm" onClick={() => setPicking("change")}><SquaresFour weight="bold" aria-hidden="true" />{tx("Pilih templat")}</button>
              </span>
            </div>
          )}
        </main>
      </div>
      </div>

      <Sheet open={!!full} onClose={() => setFull(null)} wide
        title={full ? tx("Pratinjau penuh — {0}", pluginDef(st.cells.find((c) => c.id === full)?.kind ?? "hero").name) : ""}
        description={tx("Seluruh isi blok tanpa terpotong, pada lebar yang sama dengan di kanvas.")}>
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

      <Sheet open={checks !== null} onClose={() => setChecks(null)} title={tx("Sebelum terbit")}
        description={tx("Website tetap bisa diterbitkan. Butir di bawah membuat toko lebih meyakinkan bagi pembeli.")}
        footer={<><button type="button" className="d-btn" onClick={() => setChecks(null)}>{tx("Perbaiki dulu")}</button><button type="button" className="d-btn d-btn-primary" onClick={() => void doPublish()} disabled={publishing}>{tx("Terbitkan sekarang")}</button></>}>
        <ul className="d-list" aria-label={tx("Catatan siap terbit")}>
          {(checks ?? []).map((i) => (
            <li key={i.key} className="d-row d-row-between u-gap-3">
              <span><WarningCircle weight="fill" aria-hidden="true" className="u-warn u-va-icon u-mr-2" />{i.message}</span>
              <span className="d-row u-gap-2 u-flex-none">
                {i.cellId && <button type="button" className="d-btn d-btn-sm" onClick={() => { setSel(i.cellId!); setPanelOpen(true); setChecks(null); }}>{tx("Tunjukkan")}</button>}
                {i.area === "business" && <Link className="d-btn d-btn-sm" href={`/business?tenant=${ctx.tenant.id}`}>{tx("Profil bisnis")}</Link>}
                {i.area === "products" && <Link className="d-btn d-btn-sm" href={`/products?tenant=${ctx.tenant.id}`}>{tx("Produk")}</Link>}
                {i.area === "seo" && <button type="button" className="d-btn d-btn-sm" onClick={() => { setSel(null); setPanelOpen(true); setChecks(null); setTimeout(() => document.getElementById("seo-description")?.focus(), 50); }}>{tx("Isi deskripsi")}</button>}
                {i.area === "ai" && <Link className="d-btn d-btn-sm" href={`/ai?tenant=${ctx.tenant.id}`}>{tx("Asisten AI")}</Link>}
              </span>
            </li>
          ))}
        </ul>
      </Sheet>

      <Sheet open={versionsOpen} onClose={() => setVersionsOpen(false)} title={tx("Riwayat terbit")}
        description={tx("Setiap kali menerbitkan, versi lama disimpan. Pulihkan menampilkan versi itu lagi di situs; draf Anda tidak berubah.")}>
        {versionsOpen && <Versions base={base} canPublish={canPublish} published={srv?.published ?? null} onRestored={(v) => setSrv((x) => (x ? { ...x, published: v } : x))} />}
      </Sheet>

      <Sheet open={picking === "change"} onClose={() => setPicking(null)} wide title={tx("Ganti templat")} description={tx("Susunan dan warna diganti; Anda bisa membatalkan dengan Ctrl+Z.")}>
        {picking === "change" && (
          <TemplateGallery
            business={business}
            renderCell={renderCell}
            onPick={async (k) => {
              if (st.cells.length > 0 && !(await confirm({ title: tx("Ganti templat?"), body: tx("Susunan halaman saat ini diganti. Bisa dibatalkan dengan Urungkan (Ctrl+Z)."), confirmLabel: tx("Ganti templat") }))) return;
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
