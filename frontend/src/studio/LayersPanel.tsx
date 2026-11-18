"use client";

// Panel Layer Studio (gaya Photoshop/Canva): daftar semua blok di halaman, dikelompokkan per peran
// (lapisan bebas / blok halaman / melayang), paling atas daftar = paling depan di kanvas.
// - Klik baris → pilih blok di kanvas; baris terpilih otomatis digulir ke terlihat (U4).
// - Ikon mata → sembunyikan/tampilkan blok (field `hidden`, skema v2).
// - Ikon kunci → kunci posisi blok (field `locked`, sudah ada).
// - Tombol ↑/↓ → pindahkan blok maju/mundur (F1): blok grid memakai mesin tata letak yang sama
//   dengan tombol atas/bawah di Inspector (moveBy), lapisan bebas menukar urutan tumpukan.
// - Kotak cari → saring baris (F3); berguna di halaman dengan belasan blok (templat paket).
// - Grup baris (U1): melayang → blok halaman → lapisan bebas, sesuai urutan z-index render.

import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ArrowUp, Eye, EyeSlash, LockSimple, LockSimpleOpen, MagnifyingGlass, Plus, X } from "@phosphor-icons/react/ssr";

import { isEmbed, type StudioCell } from "@/studio/editor-model";
import { pluginDef, type PluginKind } from "@/studio/plugins";
import { PluginIcon } from "@/studio/PluginIcon";
import { Tr } from "@/i18n/react";
import { t as tx } from "@/i18n";

export interface LayersPanelProps {
  cells: StudioCell[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onToggleHidden: (id: string) => void;
  onToggleLock: (id: string) => void;
  /** F1: pindahkan blok satu langkah ("up" = satu baris ke atas di daftar) — satu langkah urungkan. */
  onMove: (id: string, dir: "up" | "down") => void;
  /** U3: aksi "tambah blok" pada keadaan kosong — membuka tab Blok di panel kiri. */
  onAddBlocks: () => void;
  /** F4: tampilkan lagi semua blok yang disembunyikan. */
  onShowAll: () => void;
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

/** Grup baris: melayang (paling depan) → blok grid → lapisan bebas (paling belakang). */
function groupOf(c: StudioCell): "float" | "grid" | "layer" {
  if (isEmbed(c)) return "float";
  if (c.layer) return "layer";
  return "grid";
}

const GROUP_LABEL: Record<"float" | "grid" | "layer", string> = {
  float: tx("Melayang"),
  grid: tx("Blok halaman"),
  layer: tx("Lapisan bebas"),
};

export function LayersPanel({ cells, selectedId, onSelect, onToggleHidden, onToggleLock, onMove, onAddBlocks, onShowAll }: LayersPanelProps) {
  const ordered = useMemo(() => visualOrder(cells), [cells]);
  const [q, setQ] = useState("");
  const listRef = useRef<HTMLDivElement>(null);

  // U4: baris terpilih digulir ke terlihat (blok dipilih di kanvas → panel mengikuti).
  useEffect(() => {
    if (!selectedId || !listRef.current) return;
    listRef.current.querySelector(`[data-layer-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  const needle = q.trim().toLowerCase();
  const shown = needle ? ordered.filter((c) => labelOf(c).toLowerCase().includes(needle) || c.kind.includes(needle)) : ordered;
  const hiddenCount = cells.filter((c) => c.hidden).length;

  const groups = (["float", "grid", "layer"] as const)
    .map((g) => ({ g, items: shown.filter((c) => groupOf(c) === g) }))
    .filter((x) => x.items.length > 0);

  if (cells.length === 0) {
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
        <p>{tx("Klik untuk memilih. Ikon mata menyembunyikan blok, ikon kunci mengunci posisinya.")}</p>
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
            <ul>
              {items.map((c) => {
                const hidden = !!c.hidden;
                return (
                  <li key={c.id} data-layer-id={c.id} className={`sx-layer-item${hidden ? " sx-layer-hidden" : ""}`} aria-current={selectedId === c.id || undefined}>
                    <button
                      type="button"
                      className="sx-layer-select"
                      onClick={() => onSelect(selectedId === c.id ? null : c.id)}
                      title={isEmbed(c) ? tx("Blok melayang (di luar grid)") : undefined}
                    >
                      <span className="sx-layer-icon"><PluginIcon kind={c.kind as PluginKind} size={16} /></span>
                      <span className="sx-layer-name">{labelOf(c)}</span>
                    </button>
                    {/* F1: naik/turun satu langkah dalam grupnya. */}
                    <button
                      type="button"
                      className="sx-layer-eye"
                      onClick={() => onMove(c.id, "up")}
                      disabled={items[0] === c}
                      aria-label={tx("Naikkan {0}", labelOf(c))}
                      title={tx("Naikkan")}
                    >
                      <ArrowUp weight="bold" />
                    </button>
                    <button
                      type="button"
                      className="sx-layer-eye"
                      onClick={() => onMove(c.id, "down")}
                      disabled={items[items.length - 1] === c}
                      aria-label={tx("Turunkan {0}", labelOf(c))}
                      title={tx("Turunkan")}
                    >
                      <ArrowDown weight="bold" />
                    </button>
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
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
        {shown.length === 0 && <p className="d-muted d-small">{tx("Tidak ada blok yang cocok.")}</p>}
      </div>
    </nav>
  );
}
