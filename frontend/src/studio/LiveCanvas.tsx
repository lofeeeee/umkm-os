"use client";

// Kanvas Studio: renderer situs ASLI (`StudioSite`, mode `studio-edit`) + manipulasi langsung.
// Yang dilihat pemilik toko = yang dilihat pembeli; tidak ada lagi mode "Isi" vs "Tata letak".
//
// - Seret blok (dari mana saja, kecuali teks yang bisa diketik) → pindah. Garis sisip menunjukkan
//   tempat jatuhnya; susunan halaman mengalir ulang langsung saat menyeret.
// - Pegangan kanan → lebar (kolom), pegangan bawah → tinggi (mematikan tinggi otomatis).
// - Plugin dari panel bisa diseret ke kanvas; foto dari komputer bisa dijatuhkan ke blok.
// - Kuas/penghapus: coretan menempel pada blok di bawah kursor (atau latar halaman), dirender oleh
//   `InkLayer` yang sama di situs terbit.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { InkLayer, cellTarget } from "@/renderer/ink";
import { StudioSite, type SitePageLink } from "@/renderer/StudioSite";
import type { GridProduct } from "@/renderer/Site";
import type { Business } from "@/renderer/types";

import { DeviceFrame, zoomOf, type DeviceViewport } from "./DeviceFrame";
import type { PageV2 } from "./doc";
import type { DrawConfig, DrawTarget, DrawTool } from "./DrawPanel";
import type { DrawingStroke } from "./DrawingLayer";
import { BlockEditorContext, type BlockEditor, type GestureMode } from "./editor-context";
import { COLS, DESIGN_WIDTH, GAP, ROW, compact, place, spanWidth } from "./grid";
import type { TplCell } from "./templates";
import { t as tx } from "@/i18n";

/** Plugin yang sedang diseret dari panel (dataTransfer tidak bisa dibaca saat dragover). */
let paletteDrag: { kind: string; w: number } | null = null;
export const setPaletteDrag = (v: { kind: string; w: number } | null) => { paletteDrag = v; };

export const PLUGIN_MIME = "application/x-umkm-plugin";
const THRESHOLD = 4;
const MAX_POINTS = 1000;

interface Rect { id: string; x: number; y: number; w: number; h: number; top: number; bottom: number; left: number; width: number }
interface Gesture {
  id: string; mode: GestureMode; sx: number; sy: number; started: boolean;
  base: TplCell[]; orig: TplCell; grid: HTMLElement; rects: Rect[]; pitch: number;
  grabDx: number; grabDy: number; relLeft: number; relTop: number; k: number; last: string;
  /** Bingkai yang bisa digulir + posisi gulir awal (untuk mengimbangi gulir otomatis). */
  frame: HTMLElement | null; scroll0: number;
  /** Elemen `.sv2-block` yang diseret (digeser dengan transform, tanpa render ulang). */
  outer: HTMLElement; height: number;
  /** Perbesaran bingkai: px layar ÷ px CSS. Nilai transform/garis sisip ditulis dalam px CSS. */
  zoom: number;
  /** Susunan hasil jatuhan (null = tidak berubah). */
  result: TplCell[] | null;
  /** Tinggi tiap baris grid (px CSS) saat gerakan dimulai — untuk menaruh latar di baris yang tepat. */
  tracks: number[];
}
interface Insert { left: number; top: number; width: number }

/** Garis baris grid terdekat dari `px` (px CSS dari atas grid). Di bawah baris terakhir: ROW + GAP per baris. */
export function lineAt(tracks: number[], px: number): number {
  if (px <= 0) return 0;
  let top = 0;
  for (let i = 0; i < tracks.length; i++) {
    const bottom = top + tracks[i]!;
    if (px < bottom) return px - top < bottom - px ? i : i + 1;
    top = bottom;
  }
  return tracks.length + Math.max(0, Math.round((px - top) / (ROW + GAP)));
}

export interface LiveCanvasProps {
  device: DeviceViewport;
  doc: PageV2;
  business: Business;
  products: GridProduct[];
  assetUrl: (ref: string) => string | undefined;
  pages: SitePageLink[];
  /** Blok grid (tanpa chatbot/WhatsApp melayang). */
  cells: TplCell[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onEditBlock: (id: string, path: string, value: string) => void;
  /** Susunan baru dari seret/ubah ukuran (satu seret = satu langkah urungkan). */
  onCellsChange: (cells: TplCell[]) => void;
  onDropPlugin: (kind: string, at: { x: number; y: number }) => void;
  /** Ponsel: blok `id` dipindah ke posisi `to` di urutan ponsel. */
  onMobileReorder?: (id: string, to: number) => void;
  acceptsFile: (id: string) => boolean;
  onDropFile: (id: string, file: File) => void;
  chatReady: boolean | null;
  onChatOpen: (id: string, open: boolean) => void;
  toolbar: (id: string) => ReactNode;
  draw: { tool: DrawTool; config: DrawConfig; target: DrawTarget };
  strokes: DrawingStroke[];
  onAddStroke: (s: DrawingStroke) => void;
  onEraseStroke: (id: string) => void;
  /** Pesan singkat (mis. "geser di tampilan Desktop"). */
  onHint: (msg: string) => void;
  onZoom?: (zoom: number) => void;
}

/**
 * Slot sisip untuk blok selebar [col, col+w) yang titik tengah vertikalnya di `cy` (px relatif grid).
 * Dihitung terhadap susunan SEBELUM menyeret (susunan tidak berubah selama menyeret), jadi tidak ada
 * umpan balik yang membuat blok melompat. Blok yang kolomnya tidak beririsan diabaikan.
 */
function slotFor(rects: Rect[], skip: string | null, col: number, w: number, cy: number, stacked: boolean): { y: number; line: number; before: string | null } {
  const cands = rects
    .filter((r) => r.id !== skip && (stacked || (r.x < col + w && col < r.x + r.w)))
    .sort((a, b) => a.top - b.top);
  const idx = cands.findIndex((r) => cy < (r.top + r.bottom) / 2);
  if (cands.length === 0) return { y: 0, line: 0, before: null };
  if (idx === -1) {
    const last = cands.reduce((m, r) => (r.y + r.h > m.y + m.h ? r : m), cands[0]!);
    return { y: last.y + last.h, line: Math.max(...cands.map((r) => r.bottom)) + GAP / 2, before: null };
  }
  const hit = cands[idx]!;
  const prevBottom = idx > 0 ? Math.max(...cands.slice(0, idx).map((r) => r.bottom)) : hit.top - GAP;
  return { y: hit.y, line: (prevBottom + hit.top) / 2, before: hit.id };
}

const sameLayout = (a: TplCell[], b: TplCell[]) =>
  a.every((c) => { const d = b.find((x) => x.id === c.id); return !!d && d.x === c.x && d.y === c.y && d.w === c.w && d.h === c.h; });

function distSq(p: [number, number], a: [number, number], b: [number, number]) {
  const l2 = (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2;
  if (l2 === 0) return (p[0] - a[0]) ** 2 + (p[1] - a[1]) ** 2;
  const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / l2));
  return (p[0] - (a[0] + t * (b[0] - a[0]))) ** 2 + (p[1] - (a[1] + t * (b[1] - a[1]))) ** 2;
}
function nearStroke(pt: [number, number], pts: [number, number][], r: number) {
  if (pts.length === 1) return (pt[0] - pts[0]![0]) ** 2 + (pt[1] - pts[0]![1]) ** 2 <= r * r;
  for (let i = 0; i < pts.length - 1; i++) if (distSq(pt, pts[i]!, pts[i + 1]!) <= r * r) return true;
  return false;
}

/* ---------------------------------------------------------------- FLIP (animasi pindah posisi) */
const outerEls = (root: HTMLElement | null) => Array.from(root?.querySelectorAll<HTMLElement>(".sv2-grid > .sv2-block") ?? []);
const recordRects = (root: HTMLElement | null) => new Map(outerEls(root).map((el) => [el.id, el.getBoundingClientRect()]));
const reducedMotion = () => typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
/** Geser elemen secara visual dari (dx, dy) ke posisi aslinya. */
function animateFrom(el: HTMLElement, sdx: number, sdy: number) {
  // Selisih diukur di px layar; transform ditulis dalam px CSS (bingkai bisa diperkecil).
  const z = zoomOf(el);
  const dx = sdx / z, dy = sdy / z;
  if (reducedMotion() || (Math.abs(dx) < 1 && Math.abs(dy) < 1)) return;
  el.style.transition = "none";
  el.style.transform = `translate(${dx}px, ${dy}px)`;
  void el.offsetWidth; // paksa reflow agar posisi awal terpakai
  el.style.transition = "transform 240ms cubic-bezier(0.23, 1, 0.32, 1)";
  el.style.transform = "";
  const done = () => { el.style.transition = ""; el.removeEventListener("transitionend", done); };
  el.addEventListener("transitionend", done);
}
/** Dilepas tanpa perubahan: blok kembali ke tempatnya dengan halus. */
function settle(el: HTMLElement) {
  const now = el.getBoundingClientRect();
  el.style.transform = "";
  const home = el.getBoundingClientRect();
  animateFrom(el, now.left - home.left, now.top - home.top);
}

export function LiveCanvas(p: LiveCanvasProps) {
  const wrap = useRef<HTMLDivElement>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [insert, setInsert] = useState<Insert | null>(null);
  const gesture = useRef<Gesture | null>(null);
  // Nilai terbaru untuk handler window (tanpa memasang ulang listener).
  const latest = useRef(p);
  useEffect(() => { latest.current = p; });

  const gridEl = () => wrap.current?.querySelector<HTMLElement>(".sv2-grid") ?? null;
  const blockEl = (id: string) => wrap.current?.querySelector<HTMLElement>(`.sv2-grid [data-block-id="${CSS.escape(id)}"]`) ?? null;
  /** Ponsel: grid satu kolom (blok ditumpuk). */
  const isStacked = (g: HTMLElement) => { const cs = getComputedStyle(g); return cs.display !== "grid" || cs.gridTemplateColumns.trim().split(/\s+/).length < 2; };
  /** Jarak antarkolom dalam px LAYAR (celah 12 px CSS ikut diperkecil zoom). */
  const pitchOf = (g: HTMLElement) => { const w = g.getBoundingClientRect().width; const z = zoomOf(g); return (w - GAP * z * (COLS - 1)) / COLS + GAP * z; };

  /** Posisi layar semua blok, relatif ke pojok kiri atas grid. */
  const snapshot = useCallback((g: HTMLElement): Rect[] => {
    const gr = g.getBoundingClientRect();
    const out: Rect[] = [];
    for (const c of latest.current.cells) {
      // Latar tidak ikut menentukan slot sisip blok lain (blok boleh jatuh di atasnya).
      if (c.layer) continue;
      const el = blockEl(c.id);
      if (!el) continue;
      const r = el.getBoundingClientRect();
      out.push({ id: c.id, x: c.x, y: c.y, w: c.w, h: c.h, top: r.top - gr.top, bottom: r.bottom - gr.top, left: r.left - gr.left, width: r.width });
    }
    return out;
  }, []);

  /* ------------------------------------------------------------ seret & ubah ukuran */
  /** Gulir otomatis bingkai saat kursor mendekati tepi atas/bawah selama menyeret. */
  const autoScroll = useRef<{ raf: number; x: number; y: number }>({ raf: 0, x: 0, y: 0 });
  const onMoveRef = useRef<(ev: { clientX: number; clientY: number }) => void>(() => {});
  const onMove = useCallback((ev: Pick<PointerEvent, "clientX" | "clientY"> & { preventDefault?: () => void }) => {
    const G = gesture.current;
    if (!G) return;
    autoScroll.current.x = ev.clientX;
    autoScroll.current.y = ev.clientY;
    if (!G.started) {
      if (Math.hypot(ev.clientX - G.sx, ev.clientY - G.sy) < THRESHOLD) return;
      G.started = true;
      setActiveId(G.id);
      document.documentElement.classList.add("lc-grabbing");
    }
    ev.preventDefault?.();
    if (!autoScroll.current.raf) {
      const tick = () => {
        const g = gesture.current;
        const frame = g?.frame;
        if (!g || !frame) { autoScroll.current.raf = 0; return; }
        const fr = frame.getBoundingClientRect();
        const { x, y } = autoScroll.current;
        const edge = 64;
        const dy = y < fr.top + edge ? -Math.ceil((fr.top + edge - y) / 4) : y > fr.bottom - edge ? Math.ceil((y - (fr.bottom - edge)) / 4) : 0;
        if (dy !== 0) {
          const before = frame.scrollTop;
          frame.scrollTop += dy;
          if (frame.scrollTop !== before) onMoveRef.current({ clientX: x, clientY: y });
        }
        autoScroll.current.raf = requestAnimationFrame(tick);
      };
      autoScroll.current.raf = requestAnimationFrame(tick);
    }
    const gr = G.grid.getBoundingClientRect();
    const o = G.orig;
    let next: TplCell;
    let key: string;
    if (o.layer && G.mode !== "resize-x") {
      // Latar: bebas di posisi kursor (tidak menyisip/mendorong blok lain). Baris dipetakan dari tinggi
      // baris grid sebenarnya, karena tinggi baris mengikuti isi blok.
      const px = (ev.clientY - gr.top) / G.zoom;
      if (G.mode === "move") {
        const col = Math.max(0, Math.min(COLS - o.w, Math.round((ev.clientX - gr.left - G.grabDx) / G.pitch)));
        next = { ...o, x: col, y: lineAt(G.tracks, px - G.grabDy / G.zoom) };
      } else {
        next = { ...o, h: Math.max(o.minH ?? 2, lineAt(G.tracks, px) - o.y) };
      }
      key = `l${next.x},${next.y},${next.h}`;
      if (key === G.last) return;
      G.last = key;
      latest.current.onCellsChange(place(G.base, next) as TplCell[]);
      return;
    }
    if (G.mode === "move") {
      // Blok mengikuti kursor 1:1 (transform langsung di DOM — halus, tanpa render ulang React).
      const scrolled = (G.frame?.scrollTop ?? 0) - G.scroll0;
      G.outer.style.transform = `translate(${(ev.clientX - G.sx) / G.zoom}px, ${(ev.clientY - G.sy) / G.zoom + scrolled}px)`;
      const col = Math.max(0, Math.min(COLS - o.w, Math.round((ev.clientX - gr.left - G.grabDx) / G.pitch)));
      const cy = ev.clientY - gr.top - G.grabDy + G.height / 2;
      const t = slotFor(G.rects, G.id, col, o.w, cy, false);
      key = `m${col},${t.before ?? "end"}`;
      if (key === G.last) return;
      G.last = key;
      // Angkat blok dari susunan dulu, rapatkan sisanya, lalu sisipkan tepat SEBELUM blok sasaran
      // (atau di bawah kolom itu). Tanpa ini, menyisip "sebelum blok berikutnya" saat menyeret ke bawah
      // malah menukar keduanya sehingga blok melompat terlalu jauh.
      const rest = compact(G.base.filter((c) => c.id !== G.id)) as TplCell[];
      const hit = t.before ? rest.find((c) => c.id === t.before) : undefined;
      const y = hit ? hit.y : rest.filter((c) => c.x < col + o.w && col < c.x + c.w).reduce((m, c) => Math.max(m, c.y + c.h), 0);
      const moved = { ...o, x: col, y };
      const out = place([...rest, moved], moved) as TplCell[];
      G.result = sameLayout(out, G.base) ? null : out;
      setInsert(G.result ? { left: (col * G.pitch) / G.zoom, top: t.line / G.zoom, width: (o.w * G.pitch) / G.zoom - GAP } : null);
      return;
    } else if (G.mode === "resize-x") {
      const w = Math.max(o.minW ?? 2, Math.min(COLS - o.x, Math.round((ev.clientX - gr.left - G.relLeft + GAP * G.zoom) / G.pitch)));
      next = { ...o, w };
      key = `w${w}`;
    } else {
      const px = (ev.clientY - gr.top - G.relTop) * G.k;
      const h = Math.max(o.minH ?? 2, Math.round((px + GAP) / (ROW + GAP)));
      next = { ...o, h, autoH: false };
      key = `h${h}`;
    }
    if (key === G.last) return;
    G.last = key;
    latest.current.onCellsChange(place(G.base, next) as TplCell[]);
  }, []);

  useEffect(() => { onMoveRef.current = onMove; }, [onMove]);

  /* FLIP: setelah susunan berubah karena jatuhan, blok meluncur dari posisi lama ke posisi baru. */
  const flip = useRef<Map<string, DOMRect> | null>(null);
  useLayoutEffect(() => {
    const before = flip.current;
    if (!before) return;
    flip.current = null;
    const els = outerEls(wrap.current);
    for (const el of els) el.style.transform = "";
    for (const el of els) {
      const old = before.get(el.id);
      if (!old) continue;
      const r = el.getBoundingClientRect();
      animateFrom(el, old.left - r.left, old.top - r.top);
    }
  }, [p.cells]);
  /** Lepas listener gerakan yang sedang berjalan (dipanggil saat pointerup atau unmount). */
  const stopGesture = useRef<() => void>(() => {});
  useEffect(() => () => stopGesture.current(), []);

  /* ------------------------------------------------------------ susun ulang di ponsel */
  /** Ponsel: seret blok naik/turun untuk mengubah URUTAN PONSEL (susunan desktop tidak berubah). */
  const startMobileDrag = useCallback((id: string, e: React.PointerEvent<HTMLElement>, g: HTMLElement, el: HTMLElement) => {
    e.preventDefault();
    stopGesture.current();
    const outer = el.closest<HTMLElement>(".sv2-block") ?? el;
    const zoom = zoomOf(g);
    const sy = e.clientY;
    let started = false;
    let target = -1;
    const others = () => [...g.querySelectorAll<HTMLElement>(":scope > .sv2-block:not([data-layer])")]
      .filter((x) => x !== outer && x.offsetParent !== null)
      .map((x) => ({ el: x, r: x.getBoundingClientRect() }))
      .sort((a, b) => a.r.top - b.r.top);
    const move = (ev: PointerEvent) => {
      if (!started && Math.abs(ev.clientY - sy) < THRESHOLD) return;
      if (!started) { started = true; setActiveId(id); document.documentElement.classList.add("lc-grabbing"); }
      outer.style.transform = `translateY(${(ev.clientY - sy) / zoom}px)`;
      const list = others();
      target = list.filter((o) => o.r.top + o.r.height / 2 < ev.clientY).length;
      const gr = g.getBoundingClientRect();
      const line = target < list.length ? list[target]!.r.top - 6 : (list.at(-1)?.r.bottom ?? gr.top) + 6;
      setInsert({ left: 0, top: (line - gr.top) / zoom, width: gr.width / zoom });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      outer.style.transform = "";
      setActiveId(null);
      setInsert(null);
      document.documentElement.classList.remove("lc-grabbing");
      if (started && target >= 0) latest.current.onMobileReorder?.(id, target);
    };
    stopGesture.current = up;
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }, []);


  const startGesture = useCallback((id: string, mode: GestureMode, e: React.PointerEvent<HTMLElement>) => {
    const P = latest.current;
    if (P.draw.tool) return;
    const c = P.cells.find((x) => x.id === id);
    const g = gridEl();
    const el = blockEl(id);
    if (!c || !g || !el || c.locked) return;
    if (isStacked(g)) {
      if (mode !== "move" || c.layer || !P.onMobileReorder) {
        P.onHint(mode !== "move" ? tx("Ubah ukuran di tampilan Desktop atau Tablet — di ponsel blok selalu selebar layar.") : tx("Latar diatur di tampilan Desktop atau Tablet."));
        return;
      }
      startMobileDrag(id, e, g, el);
      return;
    }
    // Cegah seleksi teks & seret gambar bawaan browser selama gerakan.
    e.preventDefault();
    // preventDefault juga mencegah perpindahan fokus: selesaikan dulu teks yang sedang diketik (commit saat blur).
    const focused = document.activeElement as HTMLElement | null;
    if (focused?.isContentEditable || focused?.matches("input, textarea")) focused.blur();
    // Gerakan sebelumnya yang belum selesai (mis. pointerup hilang) dibersihkan DULU, sebelum yang baru dipasang.
    stopGesture.current();
    const gr = g.getBoundingClientRect();
    const br = el.getBoundingClientRect();
    const frame = g.closest<HTMLElement>(".st-frame-scroll");
    gesture.current = {
      id, mode, sx: e.clientX, sy: e.clientY, started: false, base: P.cells, orig: c, grid: g,
      rects: snapshot(g), pitch: pitchOf(g), grabDx: e.clientX - br.left, grabDy: e.clientY - br.top, relLeft: br.left - gr.left, relTop: br.top - gr.top,
      k: DESIGN_WIDTH / gr.width, last: "", frame, scroll0: frame?.scrollTop ?? 0,
      outer: el.closest<HTMLElement>(".sv2-block") ?? el, height: br.height, result: null, zoom: zoomOf(g),
      tracks: getComputedStyle(g).gridTemplateRows.split(/\s+/).map(parseFloat).filter(Number.isFinite),
    };
    const up = () => {
      const G = gesture.current;
      if (G?.mode === "move" && G.started) {
        if (G.result) {
          // FLIP: catat posisi sekarang (blok yang diseret = posisi lepasnya). Transform-nya dilepas di
          // useLayoutEffect setelah susunan baru dirender, jadi tidak ada satu frame "kembali ke asal".
          flip.current = recordRects(wrap.current);
          latest.current.onCellsChange(G.result);
        } else settle(G.outer);
      }
      gesture.current = null;
      cancelAnimationFrame(autoScroll.current.raf);
      autoScroll.current.raf = 0;
      setActiveId(null);
      setInsert(null);
      document.documentElement.classList.remove("lc-grabbing");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
    stopGesture.current = up;
    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }, [onMove, snapshot, startMobileDrag]);

  /* ------------------------------------------------------------ plugin dari panel */
  const dropTarget = (clientX: number, clientY: number): { x: number; y: number; ins: Insert } | null => {
    const g = gridEl();
    if (!g || !paletteDrag) return null;
    const gr = g.getBoundingClientRect();
    const stacked = isStacked(g);
    const pitch = pitchOf(g);
    const w = stacked ? COLS : Math.min(COLS, paletteDrag.w);
    const col = stacked ? 0 : Math.max(0, Math.min(COLS - w, Math.round((clientX - gr.left) / pitch - w / 2)));
    const t = slotFor(snapshot(g), null, col, w, clientY - gr.top, stacked);
    const z = zoomOf(g);
    return { x: col, y: t.y, ins: { left: stacked ? 0 : (col * pitch) / z, top: t.line / z, width: stacked ? gr.width / z : (w * pitch) / z - GAP } };
  };

  /* ------------------------------------------------------------ kuas & penghapus */
  /** Coretan yang sedang digambar + kotak tempatnya (relatif grid; null = latar). */
  const [live, setLive] = useState<{ stroke: DrawingStroke; box: { left: number; top: number; width: number; height: number } | null; viewW: number } | null>(null);
  const ink = useRef<{ target: string; el: HTMLElement; scale: number; points: [number, number][]; erased: Set<string> } | null>(null);

  /** Titik layar → koordinat desain relatif ke elemen (grid atau blok). */
  const local = (el: HTMLElement, scale: number, x: number, y: number): [number, number] => {
    const r = el.getBoundingClientRect();
    return [Math.max(0, Math.round((x - r.left) * scale)), Math.max(0, Math.round((y - r.top) * scale))];
  };
  const blockAt = (x: number, y: number): HTMLElement | null => {
    for (const el of document.elementsFromPoint(x, y)) {
      const b = (el as HTMLElement).closest?.<HTMLElement>(".sv2-grid [data-block-id]");
      if (b) return b;
    }
    return null;
  };
  const scaleFor = (el: HTMLElement, id: string | null) => {
    const width = el.getBoundingClientRect().width || 1;
    if (!id) return DESIGN_WIDTH / width;
    const c = latest.current.cells.find((x) => x.id === id);
    return spanWidth(c?.w ?? COLS) / width;
  };

  const erase = (x: number, y: number) => {
    const P = latest.current;
    const g = gridEl();
    const s = ink.current;
    if (!g || !s) return;
    for (const st of P.strokes) {
      if (s.erased.has(st.id)) continue;
      let el: HTMLElement | null = g;
      let id: string | null = null;
      if (st.target !== "background") {
        id = st.target.startsWith("cell:") ? st.target.slice(5) : st.target;
        el = blockEl(id);
      }
      if (!el) continue;
      const pt = local(el, scaleFor(el, id), x, y);
      if (nearStroke(pt, st.points, Math.max(st.size / 2, 12))) {
        s.erased.add(st.id);
        P.onEraseStroke(st.id);
      }
    }
  };

  const inkDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const P = latest.current;
    const g = gridEl();
    if (!g || !P.draw.tool || e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    let el: HTMLElement = g;
    let target = "background";
    if (P.draw.tool === "brush" && P.draw.target === "cell") {
      const b = blockAt(e.clientX, e.clientY);
      const id = b?.dataset.blockId;
      if (b && id) { el = b; target = cellTarget(id); }
    }
    const scale = scaleFor(el, target === "background" ? null : target.slice(5));
    ink.current = { target, el, scale, points: [local(el, scale, e.clientX, e.clientY)], erased: new Set() };
    if (P.draw.tool === "eraser") { erase(e.clientX, e.clientY); return; }
    const g0 = g.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const z = zoomOf(g);
    const box = el === g ? null : { left: (r.left - g0.left) / z, top: (r.top - g0.top) / z, width: r.width / z, height: r.height / z };
    setLive({ stroke: strokeOf(ink.current.points, P.draw.config), box, viewW: box ? r.width * scale : DESIGN_WIDTH });
  };
  const inkMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const s = ink.current;
    const P = latest.current;
    if (!s) return;
    if (P.draw.tool === "eraser") { erase(e.clientX, e.clientY); return; }
    const pt = local(s.el, s.scale, e.clientX, e.clientY);
    const last = s.points[s.points.length - 1]!;
    if (s.points.length >= MAX_POINTS || Math.hypot(pt[0] - last[0], pt[1] - last[1]) < 2) return;
    s.points.push(pt);
    setLive((l) => (l ? { ...l, stroke: strokeOf(s.points, P.draw.config) } : l));
  };
  const inkUp = () => {
    const s = ink.current;
    const P = latest.current;
    ink.current = null;
    setLive(null);
    if (!s || P.draw.tool !== "brush") return;
    P.onAddStroke({
      id: `ink-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
      target: s.target,
      color: P.draw.config.color,
      size: Math.max(1, Math.min(64, Math.round(P.draw.config.size))),
      opacity: P.draw.config.opacity,
      points: s.points.slice(0, MAX_POINTS),
    });
  };
  // Pratinjau garis yang sedang digambar, di ruang koordinat target.
  const liveLayer = live ? (
    live.box
      ? <div className="lc-ink-box" style={{ left: live.box.left, top: live.box.top, width: live.box.width, height: live.box.height }}><InkLayer strokes={[]} live={live.stroke} viewWidth={live.viewW} /></div>
      : <InkLayer strokes={[]} live={live.stroke} viewWidth={DESIGN_WIDTH} />
  ) : null;

  const drawing = !!p.draw.tool;
  const gridOverlay = (
    <>
      {insert && <div className="lc-insert" style={{ transform: `translate(${insert.left}px, ${insert.top - 7}px)`, width: insert.width }} aria-hidden="true"><span>{tx("Lepas di sini")}</span></div>}
      {drawing && (
        <div
          className={`lc-ink-capture lc-ink-${p.draw.tool}`}
          style={{ ["--ink" as string]: p.draw.config.color, ["--ink-size" as string]: `${p.draw.config.size}px` }}
          onPointerDown={inkDown} onPointerMove={inkMove} onPointerUp={inkUp} onPointerCancel={inkUp}
          aria-label={p.draw.tool === "brush" ? tx("Area menggambar") : tx("Area menghapus coretan")}
        >
          {liveLayer}
        </div>
      )}
    </>
  );

  const editor = useMemo<BlockEditor>(() => ({
    activeId,
    isLocked: (id) => !!latest.current.cells.find((c) => c.id === id)?.locked,
    startGesture,
    toolbar: (id) => latest.current.toolbar(id),
    acceptsFile: (id) => latest.current.acceptsFile(id),
    onDropFile: (id, f) => latest.current.onDropFile(id, f),
    onChatOpen: (id, o) => latest.current.onChatOpen(id, o),
    chatReady: p.chatReady,
    gridOverlay,
    drawing,
    // gridOverlay berubah setiap render (garis sisip/kuas) — sengaja tidak di-memo lebih jauh.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }), [activeId, startGesture, p.chatReady, insert, drawing, live, p.draw.tool, p.draw.config, p.selectedId, p.cells]);

  return (
    <div
      ref={wrap}
      className={`lc${drawing ? " lc-drawing" : ""}${activeId ? " lc-active" : ""}`}
      data-device={p.device}
      onPointerDown={(e) => {
        // Klik di luar blok (latar halaman/bingkai) melepas pilihan.
        if (!drawing && !(e.target as HTMLElement).closest("[data-block-id], [data-interactive], button, a, input, textarea, [contenteditable]")) p.onSelect(null);
      }}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(PLUGIN_MIME)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
        const t = dropTarget(e.clientX, e.clientY);
        setInsert(t ? t.ins : null);
      }}
      onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setInsert(null); }}
      onDrop={(e) => {
        const kind = e.dataTransfer.getData(PLUGIN_MIME);
        const t = dropTarget(e.clientX, e.clientY);
        setInsert(null);
        setPaletteDrag(null);
        if (!kind) return;
        e.preventDefault();
        p.onDropPlugin(kind, t ? { x: t.x, y: t.y } : { x: 0, y: 9999 });
      }}
    >
      <DeviceFrame device={p.device} surface={p.doc.theme.surface ?? "light"} onZoom={p.onZoom}>
        <BlockEditorContext.Provider value={editor}>
          <StudioSite
            doc={p.doc}
            business={p.business}
            products={p.products}
            assetUrl={p.assetUrl}
            pages={p.pages}
            mode="studio-edit"
            selectedId={p.selectedId}
            onSelect={p.onSelect}
            onEditBlock={p.onEditBlock}
          />
        </BlockEditorContext.Provider>
      </DeviceFrame>
    </div>
  );
}

function strokeOf(points: [number, number][], c: DrawConfig): DrawingStroke {
  return { id: "live", target: "live", color: c.color, size: c.size, opacity: c.opacity, points: [...points] };
}
