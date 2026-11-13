"use client";

// Kanvas grid 12 kolom: geser (pegangan atas), ubah ukuran (sudut kanan bawah), jatuhkan plugin dari
// katalog. Keyboard: panah = geser, Shift+panah = ubah ukuran, Delete = hapus. Item lain menyesuaikan
// secara langsung (vertical compaction, lihat grid.ts).

import { useRef, useState, type ReactNode } from "react";

import { ArrowsOutCardinal } from "@phosphor-icons/react/ssr";

import { COLS, height, place, type Cell } from "./grid";

export const ROW = 24;
export const GAP = 12;
/** Lebar desain kanvas (px). Ditampilkan dengan zoom agar muat di layar; tata letak tetap realistis. */
export const DESIGN_WIDTH = 1080;

interface Drag { id: string; mode: "move" | "resize"; sx: number; sy: number; orig: Cell; base: Cell[] }

export function Canvas<T extends Cell>({
  cells, onChange, selected, onSelect, onDropPlugin, onRemove, render, label, scale = 1,
}: {
  scale?: number;
  cells: T[];
  onChange: (cells: T[]) => void;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onDropPlugin: (kind: string, at: { x: number; y: number }) => void;
  onRemove: (id: string) => void;
  render: (c: T) => ReactNode;
  label: (c: T) => string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const width = DESIGN_WIDTH;
  const [drag, setDrag] = useState<Drag | null>(null);
  const [offset, setOffset] = useState({ dx: 0, dy: 0 });
  const [ghost, setGhost] = useState<{ x: number; y: number } | null>(null);

  const colW = (width - GAP * (COLS - 1)) / COLS;
  const px = (x: number) => x * (colW + GAP);
  const py = (y: number) => y * (ROW + GAP);
  const pw = (w: number) => w * colW + (w - 1) * GAP;
  const ph = (h: number) => h * ROW + (h - 1) * GAP;
  const toGrid = (clientX: number, clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    return { x: Math.max(0, Math.floor((clientX - r.left) / scale / (colW + GAP))), y: Math.max(0, Math.floor((clientY - r.top) / scale / (ROW + GAP))) };
  };

  function start(e: React.PointerEvent, c: T, mode: Drag["mode"]) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelect(c.id);
    setDrag({ id: c.id, mode, sx: e.clientX, sy: e.clientY, orig: c, base: cells });
    setOffset({ dx: 0, dy: 0 });
  }
  function move(e: React.PointerEvent) {
    if (!drag) return;
    const dx = (e.clientX - drag.sx) / scale;
    const dy = (e.clientY - drag.sy) / scale;
    setOffset({ dx, dy });
    const dc = Math.round(dx / (colW + GAP));
    const dr = Math.round(dy / (ROW + GAP));
    const o = drag.orig;
    const next = drag.mode === "move" ? { ...o, x: o.x + dc, y: o.y + dr } : { ...o, w: o.w + dc, h: o.h + dr };
    onChange(place(drag.base, next) as T[]);
  }
  function end() {
    setDrag(null);
    setOffset({ dx: 0, dy: 0 });
  }

  function key(e: React.KeyboardEvent, c: T) {
    const d = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    if (d) {
      e.preventDefault();
      const next = e.shiftKey ? { ...c, w: c.w + d[0]!, h: c.h + d[1]! } : { ...c, x: c.x + d[0]!, y: c.y + d[1]! };
      onChange(place(cells, next) as T[]);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      onRemove(c.id);
    }
  }

  const rows = height(cells) + 6;
  return (
    <div className="st-zoom" style={{ height: py(rows) * scale }}>
    <div
      ref={ref}
      className={`st-canvas${drag ? " st-dragging" : ""}`}
      style={{ width, height: py(rows), transform: `scale(${scale})`, ["--col" as string]: `${colW + GAP}px`, ["--row" as string]: `${ROW + GAP}px` }}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
      onPointerDown={(e) => { if (e.target === e.currentTarget) onSelect(null); }}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes("application/x-umkm-plugin")) return;
        e.preventDefault();
        setGhost(toGrid(e.clientX, e.clientY));
      }}
      onDragLeave={() => setGhost(null)}
      onDrop={(e) => {
        const kind = e.dataTransfer.getData("application/x-umkm-plugin");
        setGhost(null);
        if (kind) { e.preventDefault(); onDropPlugin(kind, toGrid(e.clientX, e.clientY)); }
      }}
      role="application"
      aria-label="Kanvas halaman. Pilih plugin lalu gunakan panah untuk menggeser, Shift+panah untuk mengubah ukuran."
    >
      {ghost && <div className="st-ghost" style={{ transform: `translate(${px(Math.min(ghost.x, COLS - 4))}px, ${py(ghost.y)}px)`, width: pw(4), height: ph(4) }} />}
      {cells.map((c) => {
        const active = drag?.id === c.id;
        const lift = active && drag.mode === "move";
        const left = lift ? px(drag.orig.x) + offset.dx : px(c.x);
        const top = lift ? py(drag.orig.y) + offset.dy : py(c.y);
        const w = active && drag.mode === "resize" ? Math.max(pw(c.minW ?? 2), pw(drag.orig.w) + offset.dx) : pw(c.w);
        const h = active && drag.mode === "resize" ? Math.max(ph(c.minH ?? 2), ph(drag.orig.h) + offset.dy) : ph(c.h);
        return (
          <div key={c.id}>
            {active && <div className="st-slot" style={{ transform: `translate(${px(c.x)}px, ${py(c.y)}px)`, width: pw(c.w), height: ph(c.h) }} />}
            <div
              className={`st-item${selected === c.id ? " st-selected" : ""}${active ? " st-active" : ""}`}
              style={{ transform: `translate(${left}px, ${top}px)`, width: w, height: h }}
              tabIndex={0}
              aria-label={`${label(c)}, kolom ${c.x + 1}, lebar ${c.w} dari 12`}
              onPointerDown={() => onSelect(c.id)}
              onFocus={() => onSelect(c.id)}
              onKeyDown={(e) => key(e, c)}
            >
              <div className="st-handle" onPointerDown={(e) => start(e, c, "move")} title="Tarik untuk memindah">
                <ArrowsOutCardinal weight="bold" aria-hidden="true" />
                <span>{label(c)}</span>
                <em>{c.w}/12</em>
              </div>
              <div className="st-body">{render(c)}</div>
              <div className="st-resize" onPointerDown={(e) => start(e, c, "resize")} title="Tarik untuk mengubah ukuran" aria-hidden="true" />
            </div>
          </div>
        );
      })}
    </div>
    </div>
  );
}
