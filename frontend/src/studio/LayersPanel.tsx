"use client";

// Panel Layer Studio (gaya Photoshop/Canva): daftar semua blok di halaman, dikelompokkan per peran
// (lapisan bebas / blok halaman / melayang), paling atas daftar = paling depan di kanvas.
// - Klik baris → pilih blok di kanvas; baris terpilih otomatis digulir ke terlihat (U4).
// - Seret baris (grip kiri) → pindahkan blok ke urutan baru — satu langkah urungkan.
// - Desktop (pointer halus): ikon mata/kunci tidak memenuhi baris; klik kanan baris →
//   menu kecil (tooltip) dengan aksi Sembunyikan/Tampilkan dan Kunci/Buka kunci.
// - Layar sentuh: ikon mata & kunci tetap tampil di samping nama (tidak ada klik kanan).
// - Kotak cari → saring baris (F3); berguna di halaman dengan belasan blok (templat paket).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DotsSixVertical, Eye, EyeSlash, LockSimple, LockSimpleOpen, MagnifyingGlass, PaintBrush, Plus, Trash, X } from "@phosphor-icons/react/ssr";

import { isEmbed, type StudioCell } from "@/studio/editor-model";
import { pluginDef, type PluginKind } from "@/studio/plugins";
import { PluginIcon } from "@/studio/PluginIcon";
import type { DrawingStroke } from "@/studio/DrawingLayer";
import { Tr } from "@/i18n/react";
import { t as tx } from "@/i18n";

export interface LayersPanelProps {
  cells: StudioCell[];
  /** Coretan (kuas): tampil sebagai lapisan tersendiri — bisa dihapus, tidak bisa diseret. */
  drawings?: DrawingStroke[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onToggleHidden: (id: string) => void;
  onToggleLock: (id: string) => void;
  /** Seret & letakkan: pindahkan blok `steps` langkah ke arah `dir` (satu langkah urungkan). */
  onReorder: (id: string, dir: "up" | "down", steps: number) => void;
  /** U3: aksi "tambah blok" pada keadaan kosong — membuka tab Blok di panel kiri. */
  onAddBlocks: () => void;
  /** F4: tampilkan lagi semua blok yang disembunyikan. */
  onShowAll: () => void;
  /** Hapus satu coretan (undoable lewat riwayat Studio). */
  onEraseStroke?: (id: string) => void;
}

/** Nama tampilan blok: judul data bila ada, selain itu nama plugin. */
function labelOf(c: StudioCell): string {
  const base = pluginDef(c.kind).name;
  const title = typeof c.data.title === "string" ? c.data.title.trim() : "";
  return title ? `${base} — ${title.slice(0, 40)}` : base;
}

/** Urutan tampil: paling atas daftar = paling depan (y besar lebih dulu, lalu x besar). */
function visualOrder(cells: StudioCell[]): StudioCell[] {
  return [...cells].sort((a, b) => b.y - a.y || b.x - a.x || b.id.localeCompare(b.id));
}

/** Grup baris — tiga jenis objek yang berbeda:
 *  - float : LAPISAN MELAYANG — objek HTML bebas (posisi & z-index mutlak, di luar grid).
 *  - grid  : BLOK HALAMAN — komponen konten yang mengalir di grid (urutan = susunan halaman).
 *  - layer : LAPISAN LATAR — objek visual tembus (z-index, tidak mendorong blok lain). */
function groupOf(c: StudioCell): "float" | "grid" | "layer" {
  if (isEmbed(c)) return "float";
  if (c.layer) return "layer";
  return "grid";
}

const GROUP_LABEL: Record<"float" | "grid" | "layer", string> = {
  float: tx("Lapisan melayang"),
  grid: tx("Blok halaman"),
  layer: tx("Lapisan latar"),
};

/** Keterangan satu baris di bawah tiap label grup — memperjelas perbedaan jenis lapisan. */
const GROUP_HINT: Record<"float" | "grid" | "layer", string> = {
  float: tx("Posisi & tumpukan bebas, di luar grid"),
  grid: tx("Komponen konten, tersusun atas→bawah"),
  layer: tx("Tembus di belakang blok, bisa diubah z-index"),
};

/** Benar di perangkat dengan pointer halus (mouse/trackpad) — klik kanan & hover tersedia. */
function useFinePointer(): boolean {
  const [fine, setFine] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia("(pointer: fine)");
    const apply = () => setFine(mq.matches);
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, []);
  return fine;
}

interface MenuState { id: string; x: number; y: number; stroke?: boolean }

export function LayersPanel({ cells, drawings = [], selectedId, onSelect, onToggleHidden, onToggleLock, onReorder, onAddBlocks, onShowAll, onEraseStroke }: LayersPanelProps) {
  const ordered = useMemo(() => visualOrder(cells), [cells]);
  const [q, setQ] = useState("");
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dropAt, setDropAt] = useState<{ id: string; after: boolean } | null>(null);
  const dragId = useRef<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const fine = useFinePointer();

  // U4: baris terpilih digulir ke terlihat (blok dipilih di kanvas → panel mengikuti).
  useEffect(() => {
    if (!selectedId || !listRef.current) return;
    listRef.current.querySelector(`[data-layer-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  // Menu klik-kanan tertutup bila klik di mana pun / gulir / Esc.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", esc);
    };
  }, [menu]);

  const needle = q.trim().toLowerCase();
  const shown = needle ? ordered.filter((c) => labelOf(c).toLowerCase().includes(needle) || c.kind.includes(needle)) : ordered;
  const hiddenCount = cells.filter((c) => c.hidden).length;

  const groups = (["float", "grid", "layer"] as const)
    .map((g) => ({ g, items: shown.filter((c) => groupOf(c) === g) }))
    .filter((x) => x.items.length > 0);

  const onDropRow = useCallback((target: StudioCell, after: boolean) => {
    const from = dragId.current;
    dragId.current = null;
    setDropAt(null);
    if (!from || from === target.id) return;
    const items = shown.filter((c) => groupOf(c) === groupOf(target));
    const fromIdx = items.findIndex((c) => c.id === from);
    let toIdx = items.findIndex((c) => c.id === target.id) + (after ? 1 : 0);
    if (fromIdx < 0) return;
    if (toIdx > fromIdx) toIdx -= 1; // mengeluarkan diri sendiri menggeser indeks
    const steps = Math.abs(toIdx - fromIdx);
    if (steps > 0) onReorder(from, toIdx < fromIdx ? "up" : "down", steps);
  }, [shown, onReorder]);

  const menuCell = menu && !menu.stroke ? cells.find((c) => c.id === menu.id) : null;

  if (cells.length === 0 && drawings.length === 0) {
    // U3: keadaan kosong membawa aksinya sendiri, bukan hanya instruksi teks.
    return (
      <div className="st-panel-head">
        <h2>{tx("Layer")}</h2>
        <div className="sx-layers-empty">
          <p>{tx("Belum ada blok. Tambahkan blok dari panel kiri.")}</p>
          <button type="button" className="d-btn d-btn-sm d-btn-primary" onClick={onAddBlocks}>
            <Plus weight="bold" aria-hidden="true" />{tx("Tambah blok")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <nav className="sx-layers-list" aria-label={tx("Layer halaman")}>
      <div className="st-panel-head">
        <h2>{tx("Layer")}</h2>
        <p>{fine
          ? tx("Seret baris untuk mengatur urutan. Klik kanan untuk menyembunyikan atau mengunci blok.")
          : tx("Ketuk untuk memilih. Ikon mata menyembunyikan blok, ikon kunci mengunci posisinya.")}</p>
        {/* F4: terlihat sekali lihat bahwa ada blok tersembunyi — mencegah terbit kehilangan blok. */}
        {hiddenCount > 0 && (
          <button type="button" className="sx-layers-unhide" onClick={onShowAll}>
            <EyeSlash weight="bold" aria-hidden="true" />
            <span><Tr s="{0} blok disembunyikan" a={[hiddenCount]} />{" "}<span className="d-link d-small">{tx("Tampilkan semua")}</span></span>
          </button>
        )}
        {/* F3: saring baris. */}
        <label className="st-search sx-layers-search">
          <MagnifyingGlass aria-hidden="true" weight="bold" />
          <input type="search" aria-label={tx("Cari blok")} placeholder={tx("Cari blok…")} value={q} onChange={(e) => setQ(e.target.value)} />
          {q && <button type="button" className="st-search-x" onClick={() => setQ("")} aria-label={tx("Hapus pencarian")}><X weight="bold" /></button>}
        </label>
      </div>
      <div className="sx-layers-items" ref={listRef}>
        {groups.map(({ g, items }) => (
          <section key={g} className="sx-layers-group" data-group={g}>
            <h3 className="sx-layers-group-label">{GROUP_LABEL[g]}</h3>
            <p className="sx-layers-group-hint">{GROUP_HINT[g]}</p>
            <ul>
              {items.map((c) => {
                const hidden = !!c.hidden;
                const dropping = dropAt?.id === c.id;
                return (
                  <li
                    key={c.id}
                    data-layer-id={c.id}
                    className={[
                      "sx-layer-item",
                      hidden ? "sx-layer-hidden" : "",
                      dropping ? (dropAt!.after ? "sx-drop-after" : "sx-drop-before") : "",
                    ].filter(Boolean).join(" ")}
                    aria-current={selectedId === c.id || undefined}
                    draggable={!needle}
                    onDragStart={(e) => { dragId.current = c.id; e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", c.id); }}
                    onDragOver={(e) => {
                      if (!dragId.current || dragId.current === c.id) return;
                      e.preventDefault();
                      const r = e.currentTarget.getBoundingClientRect();
                      setDropAt({ id: c.id, after: e.clientY > r.top + r.height / 2 });
                    }}
                    onDragLeave={() => setDropAt((d) => (d?.id === c.id ? null : d))}
                    onDrop={(e) => { e.preventDefault(); onDropRow(c, e.clientY > e.currentTarget.getBoundingClientRect().top + e.currentTarget.getBoundingClientRect().height / 2); }}
                    onDragEnd={() => { dragId.current = null; setDropAt(null); }}
                    onContextMenu={fine ? (e) => { e.preventDefault(); onSelect(c.id); setMenu({ id: c.id, x: e.clientX, y: e.clientY }); } : undefined}
                  >
                    <span className="sx-layer-grip" aria-hidden="true" title={tx("Seret untuk mengatur urutan")}>
                      <DotsSixVertical weight="bold" />
                    </span>
                    <button
                      type="button"
                      className="sx-layer-select"
                      onClick={() => onSelect(selectedId === c.id ? null : c.id)}
                      title={fine
                        ? `${labelOf(c)} — ${tx("klik kanan untuk aksi lain")}${isEmbed(c) ? ` (${tx("blok melayang")})` : ""}`
                        : isEmbed(c) ? tx("Blok melayang (di luar grid)") : undefined}
                    >
                      <span className="sx-layer-icon"><PluginIcon kind={c.kind as PluginKind} size={16} /></span>
                      <span className="sx-layer-name">{labelOf(c)}</span>
                    </button>
                    {/* Layar sentuh: aksi tetap sebagai tombol di samping nama. */}
                    {!fine && (
                      <>
                        <button
                          type="button"
                          className={`sx-layer-eye${hidden ? " sx-layer-eye-off" : ""}`}
                          onClick={() => onToggleHidden(c.id)}
                          aria-pressed={hidden}
                          aria-label={hidden ? tx("Tampilkan blok {0}", labelOf(c)) : tx("Sembunyikan blok {0}", labelOf(c))}
                          title={hidden ? tx("Tampilkan") : tx("Sembunyikan")}
                        >
                          {hidden ? <EyeSlash weight="bold" /> : <Eye weight="bold" />}
                        </button>
                        <button
                          type="button"
                          className="sx-layer-eye"
                          onClick={() => onToggleLock(c.id)}
                          aria-pressed={!!c.locked}
                          aria-label={c.locked ? tx("Buka kunci posisi blok {0}", labelOf(c)) : tx("Kunci posisi blok {0}", labelOf(c))}
                          title={c.locked ? tx("Buka kunci posisi") : tx("Kunci posisi")}
                        >
                          {c.locked ? <LockSimple weight="fill" /> : <LockSimpleOpen weight="bold" />}
                        </button>
                      </>
                    )}
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
        {shown.length === 0 && drawings.length === 0 && <p className="d-muted d-small">{tx("Tidak ada blok yang cocok.")}</p>}
        {/* Coretan (kuas): lapisan tersendiri — tidak bisa diseret/disembunyikan/dikunci, hanya dihapus.
            Coretan latar berada di belakang semua blok; coretan blok menempel pada bloknya. */}
        {drawings.length > 0 && (
          <section className="sx-layers-group" data-group="draw">
            <h3 className="sx-layers-group-label">{tx("Coretan")}</h3>
            <p className="sx-layers-group-hint">{tx("Guratan kuas; tidak ikut grid, hanya bisa dihapus")}</p>
            <ul>
              {drawings.map((s) => (
                <li
                  key={s.id}
                  className="sx-layer-item sx-layer-stroke"
                  onContextMenu={fine && onEraseStroke ? (e) => { e.preventDefault(); setMenu({ id: s.id, x: e.clientX, y: e.clientY, stroke: true }); } : undefined}
                >
                  <span className="sx-layer-icon" aria-hidden="true"><PaintBrush weight="bold" /></span>
                  <span className="sx-layer-name">
                    {tx("Coretan {0}", s.target === "background" ? tx("latar") : tx("blok"))}
                    <span className="sx-layer-swatch" style={{ background: s.color }} aria-hidden="true" />
                  </span>
                  {onEraseStroke && (
                    <button
                      type="button"
                      className="sx-layer-eye"
                      onClick={() => onEraseStroke(s.id)}
                      aria-label={tx("Hapus coretan")}
                      title={tx("Hapus coretan")}
                    >
                      <Trash weight="bold" />
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
      {/* Menu klik-kanan (desktop): aksi mata & kunci untuk blok; hapus untuk coretan. */}
      {menu && menu.stroke && onEraseStroke && (
        <div
          className="sx-layer-menu"
          role="menu"
          style={{ left: Math.min(menu.x, window.innerWidth - 200), top: Math.min(menu.y, window.innerHeight - 96) }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <button type="button" role="menuitem" onClick={() => { onEraseStroke(menu.id); setMenu(null); }}>
            <Trash weight="bold" />
            {tx("Hapus coretan")}
          </button>
        </div>
      )}
      {menu && !menu.stroke && menuCell && (
        <div
          className="sx-layer-menu"
          role="menu"
          style={{ left: Math.min(menu.x, window.innerWidth - 200), top: Math.min(menu.y, window.innerHeight - 96) }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <button type="button" role="menuitem" onClick={() => { onToggleHidden(menu.id); setMenu(null); }}>
            {menuCell.hidden ? <Eye weight="bold" /> : <EyeSlash weight="bold" />}
            {menuCell.hidden ? tx("Tampilkan blok") : tx("Sembunyikan blok")}
          </button>
          <button type="button" role="menuitem" onClick={() => { onToggleLock(menu.id); setMenu(null); }}>
            {menuCell.locked ? <LockSimpleOpen weight="bold" /> : <LockSimple weight="bold" />}
            {menuCell.locked ? tx("Buka kunci posisi") : tx("Kunci posisi")}
          </button>
        </div>
      )}
    </nav>
  );
}
