"use client";

// Kanvas grid 12 kolom (prototipe ADR-0017).
// - Seret di MANA SAJA pada plugin untuk memindah (ambang 4px; klik biasa = memilih).
// - Sudut kanan bawah = ubah ukuran. Mengubah tinggi manual mematikan "tinggi otomatis".
// - Tinggi otomatis: isi plugin diukur (ResizeObserver) sehingga konten tidak terpotong/tertindih.
// - Keyboard: panah = geser, Shift+panah = ubah ukuran, Delete = hapus.
// Elemen di dalam isi plugin yang boleh diklik saat mengedit diberi atribut data-interactive.

import { useEffect, useRef, useState, type ReactNode } from "react";

import { DotsSixVertical, LockSimple } from "@phosphor-icons/react/ssr";

import { COLS, height, place, type Cell } from "./grid";

export const ROW = 24;
export const GAP = 12;
/** Lebar desain kanvas (px). Ditampilkan dengan zoom agar muat di layar; tata letak tetap realistis. */
export const DESIGN_WIDTH = 1080;
const THRESHOLD = 4;

/** px tinggi isi → jumlah baris grid. */
export const rowsFor = (px: number) => Math.max(2, Math.ceil((px + GAP) / (ROW + GAP)));

interface Drag { id: string; mode: "move" | "resize"; sx: number; sy: number; orig: Cell; base: Cell[]; started: boolean }

export function Canvas<T extends Cell>({
  cells, onChange, selected, onSelect, onDropPlugin, onRemove, onMeasure, onResizeH, render, label, toolbar, scale = 1, onDropFile, acceptsFile,
  backgroundLayer, cellOverlay, isDrawingActive = false,
}: {
  /** Foto dijatuhkan dari komputer ke sebuah plugin. */
  onDropFile?: (id: string, file: File) => void;
  acceptsFile?: (c: T) => boolean;
  cells: T[];
  onChange: (cells: T[]) => void;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onDropPlugin: (kind: string, at: { x: number; y: number }) => void;
  onRemove: (id: string) => void;
  /** Tinggi alami isi plugin (px desain) — dipakai untuk tinggi otomatis. */
  onMeasure: (id: string, px: number) => void;
  /** Pengguna mengubah tinggi secara manual. */
  onResizeH: (id: string) => void;
  render: (c: T) => ReactNode;
  label: (c: T) => string;
  toolbar?: (c: T) => ReactNode;
  scale?: number;
  /** Lapisan latar belakang kanvas (mis. coretan brush latar). */
  backgroundLayer?: ReactNode;
  /** Lapisan overlay di atas masing-masing sel (mis. coretan terikat plugin). */
  cellOverlay?: (c: T) => ReactNode;
  /** Sedang dalam mode gambar (kuas/penghapus aktif) */
  isDrawingActive?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const width = DESIGN_WIDTH;
  const [drag, setDrag] = useState<Drag | null>(null);
  const [offset, setOffset] = useState({ dx: 0, dy: 0 });
  const [ghost, setGhost] = useState<{ x: number; y: number } | null>(null);
  const [fileOver, setFileOver] = useState<string | null>(null);

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
    // Kontrol di dalam plugin (mis. minimalkan chatbot) dan tombol toolbar tetap bisa diklik.
    if ((e.target as HTMLElement).closest("[data-interactive], button, label, input, a")) return;
    if (c.locked) { onSelect(c.id); return; } // terkunci: hanya bisa dipilih
    e.stopPropagation();
    if (mode === "resize") e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelect(c.id);
    setDrag({ id: c.id, mode, sx: e.clientX, sy: e.clientY, orig: c, base: cells, started: mode === "resize" });
    setOffset({ dx: 0, dy: 0 });
  }
  function move(e: React.PointerEvent) {
    if (!drag) return;
    const dx = (e.clientX - drag.sx) / scale;
    const dy = (e.clientY - drag.sy) / scale;
    if (!drag.started) {
      if (Math.hypot(dx, dy) * scale < THRESHOLD) return;
      setDrag({ ...drag, started: true });
    }
    setOffset({ dx, dy });
    const dc = Math.round(dx / (colW + GAP));
    const dr = Math.round(dy / (ROW + GAP));
    const o = drag.orig;
    const next = drag.mode === "move" ? { ...o, x: o.x + dc, y: o.y + dr } : { ...o, w: o.w + dc, h: o.h + dr };
    if (drag.mode === "resize" && dr !== 0) onResizeH(o.id);
    onChange(place(drag.base, next) as T[]);
  }
  function end() {
    setDrag(null);
    setOffset({ dx: 0, dy: 0 });
  }

  function key(e: React.KeyboardEvent, c: T) {
    if (e.target !== e.currentTarget) return;
    if (c.locked && e.key !== "Delete" && e.key !== "Backspace") return;
    const d = ({ ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as Record<string, number[]>)[e.key];
    if (d) {
      e.preventDefault();
      if (e.shiftKey && d[1] !== 0) onResizeH(c.id);
      const next = e.shiftKey ? { ...c, w: c.w + d[0]!, h: c.h + d[1]! } : { ...c, x: c.x + d[0]!, y: c.y + d[1]! };
      onChange(place(cells, next) as T[]);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      onRemove(c.id);
    }
  }

  const live = drag?.started ? drag : null;
  const rows = height(cells) + 6;
  return (
    <div className="st-zoom" style={{ height: py(rows) * scale }}>
      <div
        ref={ref}
        className={`st-canvas${live ? " st-dragging" : ""}`}
        style={{ width, height: py(rows), transform: `scale(${scale})`, ["--col" as string]: `${colW + GAP}px` }}
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
        aria-label="Kanvas halaman. Seret plugin untuk memindah; panah untuk menggeser, Shift+panah untuk mengubah ukuran."
      >
        {backgroundLayer && (
          <div
            className="st-bg-layer"
            style={{
              position: "absolute",
              inset: 0,
              pointerEvents: isDrawingActive ? "all" : "none",
              zIndex: isDrawingActive ? 40 : 1,
            }}
          >
            {backgroundLayer}
          </div>
        )}
        {ghost && <div className="st-ghost" style={{ transform: `translate(${px(Math.min(ghost.x, COLS - 4))}px, ${py(ghost.y)}px)`, width: pw(4), height: ph(4) }} />}
        {cells.map((c) => {
          const active = live?.id === c.id;
          const lift = active && live.mode === "move";
          const left = lift ? px(live.orig.x) + offset.dx : px(c.x);
          const top = lift ? py(live.orig.y) + offset.dy : py(c.y);
          const w = active && live.mode === "resize" ? Math.max(pw(c.minW ?? 2), pw(live.orig.w) + offset.dx) : pw(c.w);
          const h = active && live.mode === "resize" ? Math.max(ph(c.minH ?? 2), ph(live.orig.h) + offset.dy) : ph(c.h);
          return (
            <div key={c.id}>
              {active && <div className="st-slot" style={{ transform: `translate(${px(c.x)}px, ${py(c.y)}px)`, width: pw(c.w), height: ph(c.h) }} />}
              <div
                className={`st-item${selected === c.id ? " st-selected" : ""}${active ? " st-active" : ""}${fileOver === c.id ? " st-file-over" : ""}${c.locked ? " st-locked" : ""}`}
                onDragOver={(e) => {
                  if (!e.dataTransfer.types.includes("Files") || !acceptsFile?.(c)) return;
                  e.preventDefault();
                  e.stopPropagation();
                  if (fileOver !== c.id) setFileOver(c.id);
                }}
                onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setFileOver(null); }}
                onDrop={(e) => {
                  const f = e.dataTransfer.files?.[0];
                  if (!f || !acceptsFile?.(c)) return;
                  e.preventDefault();
                  e.stopPropagation();
                  setFileOver(null);
                  onDropFile?.(c.id, f);
                }}
                style={{
                  transform: `translate(${left}px, ${top}px)`,
                  width: w,
                  height: h,
                  pointerEvents: isDrawingActive ? "none" : "auto",
                }}
                tabIndex={0}
                aria-label={`${label(c)}, kolom ${c.x + 1}, lebar ${c.w} dari 12${c.locked ? ", terkunci" : ""}`}
                onPointerDown={(e) => start(e, c, "move")}
                onFocus={() => onSelect(c.id)}
                onKeyDown={(e) => key(e, c)}
              >
                <div className="st-bar">
                  <span className="st-grip">{c.locked ? <LockSimple weight="fill" aria-hidden="true" /> : <DotsSixVertical weight="bold" aria-hidden="true" />}{label(c)} <em>{c.w}/12</em></span>
                  {toolbar && <span className="st-tools">{toolbar(c)}</span>}
                </div>
                <div className="st-body">
                  <Measure id={c.id} onMeasure={onMeasure}>{render(c)}</Measure>
                  {cellOverlay && <div className="st-cell-overlay" style={{ position: "absolute", inset: 0, pointerEvents: "all", zIndex: 5 }}>{cellOverlay(c)}</div>}
                  {fileOver === c.id && <div className="st-drop-hint">Lepas untuk memasang foto</div>}
                </div>
                {c.locked
                  ? <span className="st-lock-badge" title="Posisi terkunci" aria-hidden="true"><LockSimple weight="fill" /></span>
                  : <div className="st-resize" onPointerDown={(e) => start(e, c, "resize")} title="Tarik untuk mengubah ukuran" aria-hidden="true" />}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Melaporkan tinggi alami isi setiap kali berubah (lebar plugin, teks, varian tata letak, buka/tutup chatbot). */
function Measure({ id, onMeasure, children }: { id: string; onMeasure: (id: string, px: number) => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => onMeasure(id, el.offsetHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [id, onMeasure]);
  // Tautan di dalam pratinjau tidak boleh berpindah halaman saat mengedit.
  return <div ref={ref} className="st-measure-inner" onClickCapture={(e) => { if ((e.target as HTMLElement).closest("a")) e.preventDefault(); }}>{children}</div>;
}
