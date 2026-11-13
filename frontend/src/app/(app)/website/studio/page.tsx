"use client";

// Studio website — PROTOTIPE ADR-0017 (status: Proposed). Kanvas grid 12 kolom + katalog plugin resmi.
// Belum menyentuh situs publik: tata letak disimpan di browser (localStorage) sampai ADR disetujui dan
// schema_version 2 + validator server dibangun.

import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  ArrowLeft, ArrowsIn, ArrowsOut, CaretDown, CaretUp, ChatCircleDots, Check, Copy, DesktopTower, DeviceMobile, MagnifyingGlass, Plus, PuzzlePiece, Sparkle, Trash, X,
} from "@phosphor-icons/react/ssr";

import { useUploads } from "@/dashboard/assets";
import { Assets, FieldInput } from "@/dashboard/editor/fields";
import { WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { Site, type GridProduct } from "@/renderer/Site";
import { FIELDS, FONT_PRESETS, defaultData } from "@/renderer/sections";
import type { Business, Json, PageContent, SectionType } from "@/renderer/types";
import { Canvas, DESIGN_WIDTH, GAP, ROW, rowsFor } from "@/studio/Canvas";
import { insert, mobileOrder, place, type Cell } from "@/studio/grid";
import { CELL_STYLES, PALETTES, PLUGINS, pluginDef, type CellStyle, type PluginKind } from "@/studio/plugins";
import { confirm, Sheet } from "@/ui/dialog";
import { Badge } from "@/ui/page";
import { toast } from "@/ui/toast";

interface StudioCell extends Cell {
  kind: PluginKind;
  data: { [k: string]: Json };
  style: CellStyle;
  layout?: string;
  /** Tinggi mengikuti isi (bawaan). Mati bila tinggi diubah manual. */
  autoH?: boolean;
  /** Chatbot: terbuka (maksimal) atau diminimalkan — juga keadaan awal di situs. */
  chatOpen?: boolean;
}
interface Studio {
  primary: string;
  surface: "light" | "cream" | "dark";
  font: string;
  installed: PluginKind[];
  cells: StudioCell[];
  floatingChat: { welcome: string } | null;
}

const EMPTY_BUSINESS: Business = {
  profile: { name: "Nama Toko", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false },
  addresses: [], hours: [], exceptions: [],
};
const key = (t: string) => `umkm-studio-${t}`;
let seq = 0;
const newId = (k: string) => `${k.replace(/_/g, "-")}-${Date.now().toString(36)}${(seq++).toString(36)}`;

function cellFor(kind: PluginKind, data?: { [k: string]: Json }): StudioCell {
  const d = pluginDef(kind);
  return {
    id: newId(kind), kind, x: 0, y: 0, w: d.w, h: d.h, minW: d.minW, minH: d.minH,
    data: data ?? (kind === "chatbot" ? { title: "Tanya kami", welcome: "Halo! Ada yang bisa kami bantu?" } : defaultData(kind as SectionType)),
    style: kind === "promotion" ? "tint" : kind === "chatbot" ? "card" : "plain",
    layout: d.layouts?.[0]?.value,
    autoH: true,
    chatOpen: true,
  };
}

/** Tata letak awal dari draf website yang ada: section bertumpuk → baris penuh, plus chatbot di samping banner. */
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
  return {
    primary: content?.theme.primary ?? "#4F46E5", surface: "light", font: content?.theme.font ?? FONT_PRESETS[0]!,
    installed: [...new Set<PluginKind>(["hero", "product_grid", "chatbot", ...cells.map((c) => c.kind)])], cells, floatingChat: null,
  };
}

export default function StudioPage() {
  return <WithTenant>{(ctx) => <Studio ctx={ctx} />}</WithTenant>;
}

function Studio({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const assets = useUploads(ctx.tenant.id, ctx.can("website.edit"));
  const [st, setSt] = useState<Studio | null>(null);
  const [business, setBusiness] = useState<Business>(EMPTY_BUSINESS);
  const [products, setProducts] = useState<GridProduct[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [panel, setPanel] = useState<"installed" | "catalog">("installed");
  const [q, setQ] = useState("");
  const [device, setDevice] = useState<"desktop" | "mobile">("desktop");
  const [dirty, setDirty] = useState(false);
  const [stageW, setStageW] = useState(DESIGN_WIDTH);
  const stageRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setStageW(e!.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  });
  const scale = Math.min(1, stageW / DESIGN_WIDTH);

  /** Tinggi otomatis: isi diukur oleh kanvas; baris disesuaikan bila berbeda. */
  const [clipped, setClipped] = useState<Record<string, boolean>>({});
  const onMeasure = useCallback((id: string, px: number) => {
    setSt((s) => {
      if (!s) return s;
      const c = s.cells.find((x) => x.id === id);
      if (!c) return s;
      const rows = rowsFor(px);
      setClipped((m) => (m[id] === (rows > c.h) ? m : { ...m, [id]: rows > c.h }));
      if (c.autoH === false || rows === c.h) return s;
      return { ...s, cells: place(s.cells, { ...c, h: rows }) as StudioCell[] };
    });
  }, []);
  const onResizeH = useCallback((id: string) => {
    setSt((s) => (s ? { ...s, cells: s.cells.map((c) => (c.id === id && c.autoH !== false ? { ...c, autoH: false } : c)) } : s));
  }, []);
  const [full, setFull] = useState<string | null>(null);

  useEffect(() => {
    let saved: Studio | null = null;
    try { saved = JSON.parse(localStorage.getItem(key(ctx.tenant.id)) ?? "null") as Studio | null; } catch { saved = null; }
    Promise.all([
      api<Business>("GET", `${base}/business/profile`).catch(() => EMPTY_BUSINESS),
      api<{ items: { slug: string; name: string; status: string; available: boolean; images: string[]; variants: { price: string; promo_price: string | null }[] }[] }>("GET", `${base}/products?limit=24&status=active`).catch(() => ({ items: [] })),
      saved ? Promise.resolve(null) : api<{ content: PageContent }>("GET", `${base}/website/pages/home/draft`).then((d) => d.content, () => null),
    ]).then(([b, p, draft]) => {
      setBusiness(b);
      setProducts(p.items.map((x) => ({ slug: x.slug, name: x.name, image: x.images[0], price: x.variants[0]?.price ?? "0.00", promo_price: x.variants[0]?.promo_price ?? null, available: x.available })));
      // Batas ukuran selalu dari katalog terbaru (layout tersimpan bisa memakai versi lama).
      const norm = saved && { ...saved, cells: saved.cells.map((c) => ({ ...c, minW: pluginDef(c.kind).minW, minH: pluginDef(c.kind).minH })) };
      setSt(norm || fromDraft(draft));
    });
  }, [base, ctx.tenant.id]);

  const update = useCallback((fn: (s: Studio) => Studio) => { setSt((s) => (s ? fn(s) : s)); setDirty(true); }, []);
  const setCells = useCallback((cells: StudioCell[]) => update((s) => ({ ...s, cells })), [update]);


  function add(kind: PluginKind, at?: { x: number; y: number }) {
    if (kind === "chatbot" && st?.cells.some((c) => c.kind === "chatbot")) { toast.info("Chatbot sudah ada di halaman."); return; }
    const c = cellFor(kind);
    update((s) => ({ ...s, installed: s.installed.includes(kind) ? s.installed : [...s.installed, kind], cells: insert(s.cells, c, at) as StudioCell[] }));
    setSel(c.id);
  }
  function remove(id: string) {
    update((s) => ({ ...s, cells: s.cells.filter((c) => c.id !== id) }));
    setSel(null);
  }
  function save() {
    if (!st) return;
    try { localStorage.setItem(key(ctx.tenant.id), JSON.stringify(st)); setDirty(false); toast.ok("Tata letak prototipe tersimpan di browser ini."); } catch { toast.error("Penyimpanan browser tidak tersedia."); }
  }

  const primary = st?.primary ?? "#4F46E5";
  const font = st?.font ?? FONT_PRESETS[0]!;
  const theme = useMemo(() => ({ primary, font }), [primary, font]);
  const setChatOpen = useCallback((id: string, open: boolean) => {
    setSt((s) => (s ? { ...s, cells: s.cells.map((c) => (c.id === id ? { ...c, chatOpen: open } : c)) } : s));
    setDirty(true);
  }, []);
  const renderCell = useCallback((c: StudioCell) => (
    <div className={`st-content st-style-${c.style}${c.layout ? ` st-layout-${c.layout}` : ""}`}>
      {c.kind === "chatbot" ? (
        <ChatPreview primary={theme.primary} title={String(c.data.title ?? "Tanya kami")} welcome={String(c.data.welcome ?? "")}
          open={c.chatOpen !== false} onToggle={() => setChatOpen(c.id, c.chatOpen === false)} />
      ) : (
        <Site
          content={{ schema_version: 1, template: "studio", theme, sections: [{ id: c.id, type: c.kind, enabled: true, data: c.data }] }}
          business={business}
          products={products.length ? products : undefined}
          assetUrl={assets.url}
        />
      )}
    </div>
  ), [theme, business, products, assets.url, setChatOpen]);

  const shownPlugins = useMemo(() => {
    const t = q.trim().toLowerCase();
    return PLUGINS.filter((p) => !t || `${p.name} ${p.desc} ${p.keywords} ${p.category}`.toLowerCase().includes(t));
  }, [q]);

  if (!st) return <div className="d-card"><span className="d-skel d-skel-block" /></div>;
  const selected = st.cells.find((c) => c.id === sel) ?? null;
  const surface = st.surface;

  return (
    <Assets.Provider value={assets}>
      <div className="st-top d-wide">
        <Link className="d-btn d-btn-ghost d-btn-sm" href={`/website?tenant=${ctx.tenant.id}`}><ArrowLeft weight="bold" aria-hidden="true" />Website</Link>
        <h1>Studio</h1>
        <Badge tone="warn" plain>Prototipe — belum tampil di situs publik</Badge>
        <div className="st-top-actions">
          <div className="d-seg" role="group" aria-label="Perangkat">
            <button type="button" aria-pressed={device === "desktop"} onClick={() => setDevice("desktop")}><DesktopTower weight="bold" aria-hidden="true" /> Desktop</button>
            <button type="button" aria-pressed={device === "mobile"} onClick={() => setDevice("mobile")}><DeviceMobile weight="bold" aria-hidden="true" /> Ponsel</button>
          </div>
          <button className="d-btn d-btn-sm" onClick={async () => {
            if (await confirm({ title: "Mulai ulang dari website saat ini?", body: "Tata letak prototipe akan diganti dengan isi draf website.", confirmLabel: "Mulai ulang", danger: true })) {
              const d = await api<{ content: PageContent }>("GET", `${base}/website/pages/home/draft`).then((x) => x.content, () => null);
              setSt(fromDraft(d));
              setDirty(true);
            }
          }}>Mulai ulang</button>
          <button className="d-btn d-btn-sm d-btn-primary" onClick={save} disabled={!dirty}>{dirty ? "Simpan prototipe" : <><Check weight="bold" aria-hidden="true" />Tersimpan</>}</button>
        </div>
      </div>

      <div className="st-shell d-wide">
        {/* ---------------- Panel kiri: plugin ---------------- */}
        <aside className="st-panel" aria-label={selected ? "Pengaturan plugin" : "Plugin"}>
          {selected ? (
            <>
              <div className="d-row d-row-between">
                <button type="button" className="d-back" style={{ margin: 0 }} onClick={() => setSel(null)}><ArrowLeft weight="bold" aria-hidden="true" />Plugin</button>
                <h2 style={{ margin: 0, fontSize: "0.9375rem", flex: 1 }}>{pluginDef(selected.kind).name}</h2>
                <div className="d-row">
                  <button type="button" className="d-icon-btn" aria-label="Duplikat" title="Duplikat" disabled={selected.kind === "chatbot"}
                    onClick={() => { const c = { ...selected, id: newId(selected.kind) }; update((s) => ({ ...s, cells: insert(s.cells, c, { x: selected.x, y: selected.y + selected.h }) as StudioCell[] })); setSel(c.id); }}><Copy weight="bold" /></button>
                  <button type="button" className="d-icon-btn" aria-label="Hapus dari halaman" title="Hapus" onClick={() => remove(selected.id)}><Trash weight="bold" /></button>
                </div>
              </div>
              <div className="st-section">
                <h3>Ukuran</h3>
                <div className="d-seg" role="group" aria-label="Lebar" style={{ width: "100%" }}>
                  {[[3, "¼"], [4, "⅓"], [6, "½"], [8, "⅔"], [12, "Penuh"]].map(([w, l]) => (
                    <button key={w} type="button" style={{ flex: 1 }} aria-pressed={selected.w === w} disabled={(w as number) < (selected.minW ?? 2)}
                      onClick={() => setCells(place(st.cells, { ...selected, w: w as number }) as StudioCell[])}>{l}</button>
                  ))}
                </div>
                <label className="d-switch" style={{ marginTop: 10 }}>
                  <input type="checkbox" checked={selected.autoH !== false} onChange={(e) => setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, autoH: e.target.checked } : c)))} />
                  <span><strong>Tinggi otomatis</strong><span className="d-muted">Kotak mengikuti panjang isi sehingga tidak terpotong. Mati otomatis bila tinggi ditarik manual.</span></span>
                </label>
                <button type="button" className="d-btn d-btn-sm" onClick={() => setFull(selected.id)}><ArrowsOut weight="bold" aria-hidden="true" />Lihat penuh</button>
              </div>
              <div className="st-section">
                <h3>Gaya</h3>
                <div className="d-seg" role="group" aria-label="Gaya" style={{ width: "100%" }}>
                  {CELL_STYLES.map((s) => <button key={s.value} type="button" style={{ flex: 1 }} aria-pressed={selected.style === s.value} onClick={() => setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, style: s.value } : c)))}>{s.label}</button>)}
                </div>
                {pluginDef(selected.kind).layouts && (
                  <>
                    <h3 style={{ marginTop: 12 }}>Tata letak</h3>
                    <div className="d-seg" role="group" aria-label="Tata letak" style={{ width: "100%" }}>
                      {pluginDef(selected.kind).layouts!.map((l) => (
                        <button key={l.value} type="button" style={{ flex: 1 }} aria-pressed={selected.layout === l.value}
                          onClick={() => {
                            if (selected.kind === "chatbot" && l.value === "inline" && selected.layout === "floating") {
                              // Kembali ke grid: taruh di bawah agar tidak menimpa plugin lain.
                              const grid = st.cells.filter((c) => c.id !== selected.id);
                              setCells(insert(grid, { ...selected, layout: "inline" } as StudioCell) as StudioCell[]);
                            } else setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, layout: l.value } : c)));
                          }}>{l.label}</button>
                      ))}
                    </div>
                    {selected.kind === "chatbot" && (
                      <>
                        <h3 style={{ marginTop: 12 }}>Keadaan awal</h3>
                        <div className="d-seg" role="group" aria-label="Keadaan chatbot" style={{ width: "100%" }}>
                          <button type="button" style={{ flex: 1 }} aria-pressed={selected.chatOpen !== false} onClick={() => setChatOpen(selected.id, true)}><ArrowsOut weight="bold" aria-hidden="true" /> Maksimal</button>
                          <button type="button" style={{ flex: 1 }} aria-pressed={selected.chatOpen === false} onClick={() => setChatOpen(selected.id, false)}><ArrowsIn weight="bold" aria-hidden="true" /> Minimal</button>
                        </div>
                        {selected.layout === "floating" && <p className="d-hint" style={{ marginTop: 6 }}>Melayang di pojok kanan bawah halaman, di luar grid. Klik gelembungnya untuk membuka/menutup.</p>}
                      </>
                    )}
                  </>
                )}
              </div>
              <div className="st-section">
                <h3>Isi</h3>
                {selected.kind === "chatbot" ? (
                  <>
                    <label className="d-field"><span>Judul</span><input maxLength={40} value={String(selected.data.title ?? "")} onChange={(e) => setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, data: { ...c.data, title: e.target.value } } : c)))} /></label>
                    <label className="d-field"><span>Pesan sambutan</span><textarea rows={3} maxLength={300} value={String(selected.data.welcome ?? "")} onChange={(e) => setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, data: { ...c.data, welcome: e.target.value } } : c)))} /></label>
                    <p className="d-hint">Jawaban memakai pengaturan di menu <Link href={`/ai?tenant=${ctx.tenant.id}`}>Asisten AI</Link>.</p>
                  </>
                ) : (
                  FIELDS[selected.kind as SectionType].map((f) => (
                    <FieldInput key={f.key} f={f} data={selected.data} disabled={false}
                      onChange={(d) => setCells(st.cells.map((c) => (c.id === selected.id ? { ...c, data: d } : c)))} />
                  ))
                )}
              </div>
            </>
          ) : (
          <>
          <div className="d-seg" role="tablist" aria-label="Plugin" style={{ width: "100%" }}>
            <button type="button" role="tab" aria-selected={panel === "installed"} style={{ flex: 1 }} onClick={() => setPanel("installed")}>Terpasang ({st.installed.length})</button>
            <button type="button" role="tab" aria-selected={panel === "catalog"} style={{ flex: 1 }} onClick={() => setPanel("catalog")}>Cari plugin</button>
          </div>
          {panel === "catalog" && (
            <div className="d-search" style={{ position: "relative" }}>
              <MagnifyingGlass aria-hidden="true" weight="bold" style={{ position: "absolute", left: 10, top: "50%", translate: "0 -50%", width: 14, height: 14, color: "var(--muted)" }} />
              <input type="search" aria-label="Cari plugin" placeholder="mis. chatbot, galeri…" value={q} onChange={(e) => setQ(e.target.value)} style={{ paddingLeft: 32 }} />
            </div>
          )}
          <p className="d-hint" style={{ margin: 0 }}>{panel === "installed" ? "Tarik ke kanvas, atau klik + untuk menaruh di bawah." : "Pasang plugin resmi UMKM OS untuk toko Anda."}</p>
          <ul className="st-plugins">
            {(panel === "installed" ? PLUGINS.filter((p) => st.installed.includes(p.kind)) : shownPlugins).map((p) => {
              const installed = st.installed.includes(p.kind);
              return (
                <li key={p.kind}
                  draggable={installed}
                  onDragStart={(e) => { e.dataTransfer.setData("application/x-umkm-plugin", p.kind); e.dataTransfer.effectAllowed = "copy"; }}
                  className={installed ? "st-draggable" : undefined}>
                  <span className="st-plugin-icon" aria-hidden="true">{p.kind === "chatbot" ? <ChatCircleDots weight="duotone" /> : p.kind === "product_grid" ? <Sparkle weight="duotone" /> : <PuzzlePiece weight="duotone" />}</span>
                  <span className="st-plugin-text"><strong>{p.name}</strong><span>{p.desc}</span></span>
                  {installed
                    ? <button type="button" className="d-icon-btn" aria-label={`Tambah ${p.name} ke halaman`} onClick={() => add(p.kind)}><Plus weight="bold" /></button>
                    : <button type="button" className="d-btn d-btn-sm" onClick={() => { update((s) => ({ ...s, installed: [...s.installed, p.kind] })); toast.ok(`${p.name} terpasang. Tarik ke kanvas untuk memakainya.`); }}>Pasang</button>}
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
                  onClick={() => update((s) => ({ ...s, primary: p.primary, surface: p.surface }))}
                  style={{ ["--sw" as string]: p.primary, ["--sf" as string]: p.surface === "dark" ? "#111118" : p.surface === "cream" ? "#fbf6ee" : "#ffffff" }}>
                  <span aria-hidden="true" /><em>{p.name}</em>
                </button>
              ))}
            </div>
            <div className="d-grid" style={{ marginTop: 10 }}>
              <label className="d-field" style={{ margin: 0 }}><span>Warna bebas</span>
                <span className="d-color"><input type="color" value={st.primary} onChange={(e) => update((s) => ({ ...s, primary: e.target.value.toUpperCase() }))} /><code>{st.primary}</code></span>
              </label>
              <label className="d-field" style={{ margin: 0 }}><span>Latar</span>
                <select value={st.surface} onChange={(e) => update((s) => ({ ...s, surface: e.target.value as Studio["surface"] }))}>
                  <option value="light">Putih</option><option value="cream">Krem</option><option value="dark">Gelap</option>
                </select>
              </label>
            </div>
            <label className="d-field" style={{ marginTop: 10 }}><span>Font</span>
              <select value={st.font} onChange={(e) => update((s) => ({ ...s, font: e.target.value }))}>
                {FONT_PRESETS.map((f) => <option key={f}>{f}</option>)}
              </select>
            </label>
          </div>
          <p className="d-hint" style={{ margin: 0 }}>Tips: tarik pegangan atas plugin untuk memindah, sudut kanan bawah untuk mengubah ukuran. Keyboard: panah & Shift+panah, Delete untuk menghapus.</p>
          </>
          )}
        </aside>

        {/* ---------------- Kanvas ---------------- */}
        <div className="st-stage" data-surface={surface} style={{ ["--s-primary" as string]: st.primary }}>
          <div ref={stageRef} className="st-measure" />
          {device === "desktop" ? (
            <Canvas
              scale={scale}
              cells={st.cells.filter((c) => !(c.kind === "chatbot" && c.layout === "floating"))}
              onMeasure={onMeasure}
              onResizeH={onResizeH}
              toolbar={(c) => (
                <>
                  {clipped[c.id] && c.autoH === false && <span className="st-clip" title="Isi lebih panjang dari kotak">Terpotong</span>}
                  {c.kind === "chatbot" && (
                    <button type="button" className="st-tool" onClick={() => setChatOpen(c.id, c.chatOpen === false)} aria-label={c.chatOpen === false ? "Maksimalkan chatbot" : "Minimalkan chatbot"} title={c.chatOpen === false ? "Maksimalkan" : "Minimalkan"}>
                      {c.chatOpen === false ? <CaretUp weight="bold" /> : <CaretDown weight="bold" />}
                    </button>
                  )}
                  <button type="button" className="st-tool" onClick={() => setFull(c.id)} aria-label={`Lihat penuh ${pluginDef(c.kind).name}`} title="Lihat penuh"><ArrowsOut weight="bold" /></button>
                </>
              )}
              onChange={(cells) => update((s) => ({ ...s, cells: [...cells, ...s.cells.filter((c) => c.kind === "chatbot" && c.layout === "floating")] }))}
              selected={sel}
              onSelect={setSel}
              onRemove={remove}
              onDropPlugin={(k, at) => add(k as PluginKind, at)}
              render={renderCell}
              label={(c) => pluginDef(c.kind).name}
            />
          ) : (
            <div className="st-phone">
              {mobileOrder(st.cells.filter((c) => !(c.kind === "chatbot" && c.layout === "floating"))).map((c) => <div key={c.id} className="st-phone-cell">{renderCell(c)}</div>)}
              <FloatingChat cell={st.cells.find((c) => c.kind === "chatbot" && c.layout === "floating")} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} />
            </div>
          )}
          {device === "desktop" && <FloatingChat cell={st.cells.find((c) => c.kind === "chatbot" && c.layout === "floating")} primary={st.primary} onToggle={setChatOpen} onSelect={setSel} selected={sel} />}
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
            <div className="st-stage st-full" data-surface={surface} style={{ ["--s-primary" as string]: st.primary }}>
              <div style={{ width: `${(c.layout === "floating" ? 4 : c.w) / 12 * 100}%`, minWidth: 320, margin: "0 auto" }}>{renderCell({ ...c, chatOpen: true })}</div>
            </div>
          );
        })()}
      </Sheet>
    </Assets.Provider>
  );
}

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
