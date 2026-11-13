"use client";

// Studio website — PROTOTIPE ADR-0017 (status: Proposed). Kanvas grid 12 kolom + katalog plugin resmi.
// - Mulai dari galeri template yang langsung diisi data toko.
// - Edit langsung: klik plugin untuk memilih, klik teksnya untuk mengetik; seret foto ke plugin.
// - Undo/Redo (Ctrl+Z / Ctrl+Shift+Z) + simpan otomatis.
// Belum menyentuh situs publik: tata letak disimpan di browser (localStorage) sampai ADR disetujui dan
// schema_version 2 + validator server dibangun.

import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  ArrowClockwise, ArrowCounterClockwise, ArrowLeft, ArrowsIn, ArrowsOut, CaretDown, CaretUp, ChatCircleDots, Check, Copy,
  DesktopTower, DeviceMobile, ImageSquare, MagnifyingGlass, Plus, PuzzlePiece, Sparkle, SquaresFour, Trash, X,
} from "@phosphor-icons/react/ssr";

import { useUploads } from "@/dashboard/assets";
import { Assets, FieldInput } from "@/dashboard/editor/fields";
import { WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { EditContext, type EditApi } from "@/renderer/edit";
import { Site, type GridProduct } from "@/renderer/Site";
import { FIELDS, FONT_PRESETS, defaultData } from "@/renderer/sections";
import type { Business, Json, PageContent, SectionType } from "@/renderer/types";
import { Canvas, DESIGN_WIDTH, GAP, ROW, rowsFor } from "@/studio/Canvas";
import { COLS, insert, mobileOrder, place } from "@/studio/grid";
import { commit, redo, silent, start, undo, type History } from "@/studio/history";
import { CELL_STYLES, PALETTES, PLUGINS, pluginDef, type PluginKind } from "@/studio/plugins";
import { TEMPLATES, type TplCell } from "@/studio/templates";
import { confirm, Sheet } from "@/ui/dialog";
import { Badge } from "@/ui/page";
import { toast } from "@/ui/toast";

type StudioCell = TplCell;
interface Studio {
  primary: string;
  surface: "light" | "cream" | "dark";
  font: string;
  installed: PluginKind[];
  cells: StudioCell[];
}

const EMPTY_BUSINESS: Business = {
  profile: { name: "Nama Toko", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false },
  addresses: [], hours: [], exceptions: [],
};
const storageKey = (t: string) => `umkm-studio-${t}`;
let seq = 0;
const newId = (k: string) => `${k.replace(/_/g, "-")}-${Date.now().toString(36)}${(seq++).toString(36)}`;
const isFloating = (c: StudioCell) => c.kind === "chatbot" && c.layout === "floating";
/** Plugin yang punya satu foto utama (diganti saat foto dijatuhkan). Galeri: foto ditambahkan. */
const IMAGE_KEY: Partial<Record<PluginKind, string>> = { hero: "image", about: "image", promotion: "image" };
const takesPhoto = (k: PluginKind) => k in IMAGE_KEY || k === "gallery";

function cellFor(kind: PluginKind, data?: { [k: string]: Json }): StudioCell {
  const d = pluginDef(kind);
  return {
    id: newId(kind), kind, x: 0, y: 0, w: d.w, h: d.h, minW: d.minW, minH: d.minH,
    data: data ?? (kind === "chatbot" ? { title: "Tanya kami", welcome: "Halo! Ada yang bisa kami bantu?" } : defaultData(kind as SectionType)),
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
  const fields = kind === "chatbot" ? [] : FIELDS[kind as SectionType];
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
  const assets = useUploads(ctx.tenant.id, ctx.can("website.edit"));
  const [hist, setHist] = useState<History<Studio> | null>(null);
  const st = hist?.present ?? null;
  const [business, setBusiness] = useState<Business>(EMPTY_BUSINESS);
  const [rawProducts, setRawProducts] = useState<{ slug: string; name: string; image?: string; price: string; promo_price: string | null; available: boolean }[]>([]);
  const [draft, setDraft] = useState<PageContent | null>(null);
  const [picking, setPicking] = useState<"first" | "change" | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [panel, setPanel] = useState<"installed" | "catalog">("installed");
  const [q, setQ] = useState("");
  const [device, setDevice] = useState<"desktop" | "mobile">("desktop");
  const [full, setFull] = useState<string | null>(null);
  const [savedRef, setSavedRef] = useState<Studio | null>(null);
  const [stageW, setStageW] = useState(DESIGN_WIDTH);
  const stageRef = useRef<HTMLDivElement>(null);
  const [clipped, setClipped] = useState<Record<string, boolean>>({});

  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setStageW(e!.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  });
  const scale = Math.min(1, stageW / DESIGN_WIDTH);

  /* ---------------- Muat data toko ---------------- */
  useEffect(() => {
    let saved: Studio | null = null;
    try { saved = JSON.parse(localStorage.getItem(storageKey(ctx.tenant.id)) ?? "null") as Studio | null; } catch { saved = null; }
    Promise.all([
      api<Business>("GET", `${base}/business/profile`).catch(() => EMPTY_BUSINESS),
      api<{ items: { slug: string; name: string; available: boolean; images: string[]; variants: { price: string; promo_price: string | null }[] }[] }>("GET", `${base}/products?limit=24&status=active`).catch(() => ({ items: [] })),
      api<{ content: PageContent }>("GET", `${base}/website/pages/home/draft`).then((d) => d.content, () => null),
    ]).then(([b, p, d]) => {
      setBusiness(b);
      setDraft(d);
      setRawProducts(p.items.map((x) => ({ slug: x.slug, name: x.name, image: x.images[0], price: x.variants[0]?.price ?? "0.00", promo_price: x.variants[0]?.promo_price ?? null, available: x.available })));
      if (saved) {
        // Batas ukuran selalu dari katalog terbaru (layout tersimpan bisa memakai versi lama).
        const norm = withInstalled({ ...saved, cells: saved.cells.map((c) => ({ ...c, minW: pluginDef(c.kind).minW, minH: pluginDef(c.kind).minH })) });
        setHist(start(norm));
        setSavedRef(norm);
      } else {
        setHist(start(fromDraft(d)));
        setPicking("first");
      }
    });
  }, [base, ctx.tenant.id]);
  const products = useMemo<GridProduct[]>(() => rawProducts.map((p) => ({ ...p, image: p.image ? assets.url(p.image) : undefined })), [rawProducts, assets]);
  const noPhoto = rawProducts.filter((p) => !p.image).length;

  /* ---------------- Riwayat & simpan otomatis ---------------- */
  const update = useCallback((fn: (s: Studio) => Studio, key = "edit") => {
    setHist((h) => (h ? commit(h, fn(h.present), key, Date.now()) : h));
  }, []);
  const quiet = useCallback((fn: (s: Studio) => Studio) => setHist((h) => (h ? silent(h, fn(h.present)) : h)), []);
  const doUndo = useCallback(() => setHist((h) => (h ? undo(h) : h)), []);
  const doRedo = useCallback(() => setHist((h) => (h ? redo(h) : h)), []);

  useEffect(() => {
    if (!st || picking === "first") return;
    const t = setTimeout(() => {
      try { localStorage.setItem(storageKey(ctx.tenant.id), JSON.stringify(st)); setSavedRef(st); } catch { /* penyimpanan diblokir */ }
    }, 600);
    return () => clearTimeout(t);
  }, [st, picking, ctx.tenant.id]);
  const saved = st !== null && savedRef === st;

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
    if (kind === "chatbot" && st?.cells.some((c) => c.kind === "chatbot")) { toast.info("Chatbot sudah ada di halaman."); return; }
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
    if (!file.type.startsWith("image/")) { toast.error("Hanya file gambar (PNG, JPEG, WebP)."); return; }
    const u = await assets.upload(file);
    if (!u) { toast.error("Unggahan ditolak. Pastikan gambar PNG/JPEG/WebP ≤ 2 MB."); return; }
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

  if (!st || !hist) return <div className="d-card"><span className="d-skel d-skel-block" /></div>;

  if (picking === "first") {
    return (
      <Assets.Provider value={assets}>
        <TemplateGallery
          first
          business={business}
          renderCell={renderCell}
          onPick={applyTemplate}
          onCurrent={() => { setPicking(null); toast.ok("Memakai susunan website Anda saat ini."); }}
          surfaceOf={(k) => TEMPLATES.find((t) => t.key === k)!.build({ name: business.profile.name, description: business.profile.description ?? null })}
        />
      </Assets.Provider>
    );
  }

  const selected = st.cells.find((c) => c.id === sel) ?? null;
  const gridCells = st.cells.filter((c) => !isFloating(c));
  const floating = st.cells.find(isFloating);

  return (
    <Assets.Provider value={assets}>
      <div className="st-top d-wide">
        <Link className="d-btn d-btn-ghost d-btn-sm" href={`/website?tenant=${ctx.tenant.id}`}><ArrowLeft weight="bold" aria-hidden="true" />Website</Link>
        <h1>Studio</h1>
        <Badge tone="warn" plain>Prototipe — belum tampil di situs publik</Badge>
        <span className="st-save" aria-live="polite">{saved ? <><Check weight="bold" aria-hidden="true" />Tersimpan otomatis</> : "Menyimpan…"}</span>
        <div className="st-top-actions">
          <div className="d-row" style={{ gap: 2 }}>
            <button type="button" className="d-icon-btn" onClick={doUndo} disabled={hist.past.length === 0} aria-label="Urungkan (Ctrl+Z)" title="Urungkan (Ctrl+Z)"><ArrowCounterClockwise weight="bold" /></button>
            <button type="button" className="d-icon-btn" onClick={doRedo} disabled={hist.future.length === 0} aria-label="Ulangi (Ctrl+Shift+Z)" title="Ulangi (Ctrl+Shift+Z)"><ArrowClockwise weight="bold" /></button>
          </div>
          <button className="d-btn d-btn-sm" onClick={() => setPicking("change")}><SquaresFour weight="bold" aria-hidden="true" />Template</button>
          <div className="d-seg" role="group" aria-label="Perangkat">
            <button type="button" aria-pressed={device === "desktop"} onClick={() => setDevice("desktop")}><DesktopTower weight="bold" aria-hidden="true" /> Desktop</button>
            <button type="button" aria-pressed={device === "mobile"} onClick={() => setDevice("mobile")}><DeviceMobile weight="bold" aria-hidden="true" /> Ponsel</button>
          </div>
        </div>
      </div>

      <div className="st-shell d-wide">
        <aside className="st-panel" aria-label={selected ? "Pengaturan plugin" : "Plugin"}>
          {selected ? (
            <Inspector
              c={selected}
              onBack={() => setSel(null)}
              onChange={(patch, key) => setCell(selected.id, patch, key)}
              onWidth={(w) => update((s) => ({ ...s, cells: place(s.cells, { ...selected, w }) as StudioCell[] }), "")}
              onLayout={(l) => {
                if (selected.kind === "chatbot" && l === "inline" && isFloating(selected)) {
                  update((s) => ({ ...s, cells: insert(s.cells.filter((c) => c.id !== selected.id), { ...selected, layout: "inline" } as StudioCell) as StudioCell[] }), "");
                } else setCell(selected.id, { layout: l }, "");
              }}
              onDuplicate={() => { const c = { ...selected, id: newId(selected.kind) }; update((s) => ({ ...s, cells: insert(s.cells, c, { x: selected.x, y: selected.y + selected.h }) as StudioCell[] }), ""); setSel(c.id); }}
              onRemove={() => remove(selected.id)}
              onFull={() => setFull(selected.id)}
              onPhoto={(f) => void putPhoto(selected.id, f)}
              aiHref={`/ai?tenant=${ctx.tenant.id}`}
            />
          ) : (
            <>
              <div className="d-seg" role="tablist" aria-label="Plugin" style={{ width: "100%" }}>
                <button type="button" role="tab" aria-selected={panel === "installed"} style={{ flex: 1 }} onClick={() => setPanel("installed")}>Terpasang ({st.installed.length})</button>
                <button type="button" role="tab" aria-selected={panel === "catalog"} style={{ flex: 1 }} onClick={() => setPanel("catalog")}>Cari plugin</button>
              </div>
              {panel === "catalog" && (
                <div style={{ position: "relative" }}>
                  <MagnifyingGlass aria-hidden="true" weight="bold" style={{ position: "absolute", left: 10, top: "50%", translate: "0 -50%", width: 14, height: 14, color: "var(--muted)" }} />
                  <input type="search" aria-label="Cari plugin" placeholder="mis. chatbot, galeri…" value={q} onChange={(e) => setQ(e.target.value)} style={{ paddingLeft: 32 }} />
                </div>
              )}
              <p className="d-hint" style={{ margin: 0 }}>{panel === "installed" ? "Tarik ke kanvas, atau klik + untuk menaruh di bawah." : "Pasang plugin resmi UMKM OS untuk toko Anda."}</p>
              <ul className="st-plugins">
                {(panel === "installed" ? PLUGINS.filter((p) => st.installed.includes(p.kind)) : shownPlugins).map((p) => {
                  const installed = st.installed.includes(p.kind);
                  return (
                    <li key={p.kind} draggable={installed} className={installed ? "st-draggable" : undefined}
                      onDragStart={(e) => { e.dataTransfer.setData("application/x-umkm-plugin", p.kind); e.dataTransfer.effectAllowed = "copy"; }}>
                      <span className="st-plugin-icon" aria-hidden="true">{p.kind === "chatbot" ? <ChatCircleDots weight="duotone" /> : p.kind === "product_grid" ? <Sparkle weight="duotone" /> : <PuzzlePiece weight="duotone" />}</span>
                      <span className="st-plugin-text"><strong>{p.name}</strong><span>{p.desc}</span></span>
                      {installed
                        ? <button type="button" className="d-icon-btn" aria-label={`Tambah ${p.name} ke halaman`} onClick={() => add(p.kind)}><Plus weight="bold" /></button>
                        : <button type="button" className="d-btn d-btn-sm" onClick={() => { update((s) => ({ ...s, installed: [...s.installed, p.kind] }), ""); toast.ok(`${p.name} terpasang. Tarik ke kanvas untuk memakainya.`); }}>Pasang</button>}
                    </li>
                  );
                })}
                {panel === "catalog" && shownPlugins.length === 0 && <li className="d-muted d-small">Tidak ada plugin untuk “{q}”.</li>}
              </ul>

              <div className="st-section">
                <h3>Warna</h3>
                <div className="st-palettes">
                  {PALETTES.map((p) => (
                    <button key={p.name} type="button" className="st-swatch" aria-pressed={st.primary === p.primary && st.surface === p.surface} title={p.name}
                      onClick={() => update((s) => ({ ...s, primary: p.primary, surface: p.surface }), "")}
                      style={{ ["--sw" as string]: p.primary, ["--sf" as string]: p.surface === "dark" ? "#111118" : p.surface === "cream" ? "#fbf6ee" : "#ffffff" }}>
                      <span aria-hidden="true" /><em>{p.name}</em>
                    </button>
                  ))}
                </div>
                <div className="d-grid" style={{ marginTop: 10 }}>
                  <label className="d-field" style={{ margin: 0 }}><span>Warna bebas</span>
                    <span className="d-color"><input type="color" value={st.primary} onChange={(e) => update((s) => ({ ...s, primary: e.target.value.toUpperCase() }), "color")} /><code>{st.primary}</code></span>
                  </label>
                  <label className="d-field" style={{ margin: 0 }}><span>Latar</span>
                    <select value={st.surface} onChange={(e) => update((s) => ({ ...s, surface: e.target.value as Studio["surface"] }), "")}>
                      <option value="light">Putih</option><option value="cream">Krem</option><option value="dark">Gelap</option>
                    </select>
                  </label>
                </div>
                <label className="d-field" style={{ marginTop: 10 }}><span>Font</span>
                  <select value={st.font} onChange={(e) => update((s) => ({ ...s, font: e.target.value }), "")}>
                    {FONT_PRESETS.map((f) => <option key={f}>{f}</option>)}
                  </select>
                </label>
              </div>
              <p className="d-hint" style={{ margin: 0 }}>Tips: klik plugin untuk memilih, klik teksnya untuk mengetik, seret foto dari komputer ke plugin. Ctrl+Z untuk membatalkan.</p>
            </>
          )}
        </aside>

        <div className="st-stage" data-surface={st.surface} style={{ ["--s-primary" as string]: st.primary }}>
          <div ref={stageRef} className="st-measure" />
          {device === "desktop" ? (
            <Canvas
              scale={scale}
              cells={gridCells}
              onChange={(cells) => update((s) => ({ ...s, cells: [...cells, ...s.cells.filter(isFloating)] }), "drag")}
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
              toolbar={(c) => (
                <>
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
                  {c.kind === "chatbot" && (
                    <button type="button" className="st-tool" onClick={() => setChatOpen(c.id, c.chatOpen === false)} aria-label={c.chatOpen === false ? "Maksimalkan chatbot" : "Minimalkan chatbot"} title={c.chatOpen === false ? "Maksimalkan" : "Minimalkan"}>
                      {c.chatOpen === false ? <CaretUp weight="bold" /> : <CaretDown weight="bold" />}
                    </button>
                  )}
                  <button type="button" className="st-tool" onClick={() => setFull(c.id)} aria-label={`Lihat penuh ${pluginDef(c.kind).name}`} title="Lihat penuh"><ArrowsOut weight="bold" /></button>
                </>
              )}
            />
          ) : (
            <div className="st-phone">
              {mobileOrder(gridCells).map((c) => <div key={c.id} className="st-phone-cell">{renderCell(c)}</div>)}
              <FloatingChat cell={floating} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} />
            </div>
          )}
          {device === "desktop" && <FloatingChat cell={floating} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} />}
          {st.cells.length === 0 && <p className="st-empty">Tarik plugin dari panel kiri ke sini.</p>}
        </div>
      </div>

      <Sheet open={!!full} onClose={() => setFull(null)} wide
        title={full ? `Pratinjau penuh — ${pluginDef(st.cells.find((c) => c.id === full)?.kind ?? "hero").name}` : ""}
        description="Seluruh isi plugin tanpa terpotong, pada lebar yang sama dengan di kanvas.">
        {full && (() => {
          const c = st.cells.find((x) => x.id === full);
          if (!c) return null;
          return (
            <div className="st-stage st-full" data-surface={st.surface} style={{ ["--s-primary" as string]: st.primary }}>
              <div style={{ width: `${(isFloating(c) ? 4 : c.w) / COLS * 100}%`, minWidth: 320, margin: "0 auto" }}>{renderCell({ ...c, chatOpen: true })}</div>
            </div>
          );
        })()}
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
    </Assets.Provider>
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
                <MiniLayout cells={p.cells.filter((c) => !isFloating(c))} render={(c) => renderCell(c, false, { primary: p.primary, font: p.font })} theme={{ primary: p.primary, font: p.font }} />
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
function Inspector({ c, onBack, onChange, onWidth, onLayout, onDuplicate, onRemove, onFull, onPhoto, aiHref }: {
  c: StudioCell;
  onBack: () => void;
  onChange: (patch: Partial<StudioCell>, key?: string) => void;
  onWidth: (w: number) => void;
  onLayout: (l: string) => void;
  onDuplicate: () => void;
  onRemove: () => void;
  onFull: () => void;
  onPhoto: (f: File) => void;
  aiHref: string;
}) {
  const def = pluginDef(c.kind);
  return (
    <>
      <div className="d-row d-row-between">
        <button type="button" className="d-back" style={{ margin: 0 }} onClick={onBack}><ArrowLeft weight="bold" aria-hidden="true" />Plugin</button>
        <h2 style={{ margin: 0, fontSize: "0.9375rem", flex: 1 }}>{def.name}</h2>
        <div className="d-row">
          <button type="button" className="d-icon-btn" aria-label="Duplikat" title="Duplikat" disabled={c.kind === "chatbot"} onClick={onDuplicate}><Copy weight="bold" /></button>
          <button type="button" className="d-icon-btn" aria-label="Hapus dari halaman" title="Hapus" onClick={onRemove}><Trash weight="bold" /></button>
        </div>
      </div>
      {c.kind !== "chatbot" && <p className="d-hint" style={{ margin: 0 }}>Klik teks di kanvas untuk mengetik langsung.{takesPhoto(c.kind) && " Seret foto dari komputer ke plugin untuk memasangnya."}</p>}
      <div className="st-section">
        <h3>Ukuran</h3>
        <div className="d-seg" role="group" aria-label="Lebar" style={{ width: "100%" }}>
          {([[3, "¼"], [4, "⅓"], [6, "½"], [8, "⅔"], [12, "Penuh"]] as const).map(([w, l]) => (
            <button key={w} type="button" style={{ flex: 1 }} aria-pressed={c.w === w} disabled={w < (c.minW ?? 2)} onClick={() => onWidth(w)}>{l}</button>
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
        </div>
      </div>
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
            {c.layout === "floating" && <p className="d-hint" style={{ marginTop: 6 }}>Melayang di pojok kanan bawah halaman, di luar grid. Klik gelembungnya untuk membuka/menutup.</p>}
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
        ) : (
          FIELDS[c.kind as SectionType].map((f) => (
            <FieldInput key={f.key} f={f} data={c.data} disabled={false} onChange={(d) => onChange({ data: d })} />
          ))
        )}
      </div>
    </>
  );
}

/* ============================================================ Chatbot */
/** Chatbot melayang: gelembung di pojok kanan bawah panggung; klik untuk buka/tutup. */
function FloatingChat({ cell, primary, onToggle, onSelect, selected }: {
  cell: StudioCell | undefined; primary: string; onToggle: (id: string, open: boolean) => void; onSelect: (id: string) => void; selected: string | null;
}) {
  if (!cell) return null;
  const open = cell.chatOpen !== false;
  return (
    <div className={`st-float${selected === cell.id ? " st-selected" : ""}`} onPointerDown={() => onSelect(cell.id)}>
      {open && (
        <div className="st-float-panel">
          <ChatPreview primary={primary} title={String(cell.data.title ?? "Tanya kami")} welcome={String(cell.data.welcome ?? "")} open onToggle={() => onToggle(cell.id, false)} />
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
