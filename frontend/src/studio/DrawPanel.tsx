"use client";

// Panel Coretan: kuas & penghapus untuk menggambar langsung di halaman (aksen, panah, stabilo).
// Coretan disimpan di dokumen (`drawings`) dan dirender juga di situs terbit (src/renderer/ink.tsx).

import { Check, Eraser, PaintBrush, Trash } from "@phosphor-icons/react/ssr";

import { confirm } from "@/ui/dialog";

export interface DrawConfig {
  color: string;
  size: number;
  opacity: number;
}

export type DrawTool = "brush" | "eraser" | null;
/** "cell" = coretan menempel pada blok tempat mulai menggambar; "background" = latar halaman. */
export type DrawTarget = "background" | "cell";

export interface DrawPanelProps {
  activeTool: DrawTool;
  onSelectTool: (tool: DrawTool) => void;
  target: DrawTarget;
  onSelectTarget: (target: DrawTarget) => void;
  config: DrawConfig;
  onChangeConfig: (config: DrawConfig) => void;
  primaryColor?: string;
  /** Jumlah coretan di halaman ini. */
  strokeCount?: number;
  onClearStrokes: () => void | Promise<void>;
  className?: string;
}

export const SIZE_PRESETS = [
  { label: "Halus", size: 2 },
  { label: "Sedang", size: 4 },
  { label: "Tebal", size: 8 },
  { label: "Stabilo", size: 18 },
];

export const STANDARD_SWATCHES = ["#18181B", "#EF4444", "#F59E0B", "#10B981", "#3B82F6", "#8B5CF6", "#EC4899", "#FFFFFF"];

export function DrawPanel({
  activeTool, onSelectTool, target, onSelectTarget, config, onChangeConfig, primaryColor = "#4F46E5", strokeCount = 0, onClearStrokes, className = "",
}: DrawPanelProps) {
  const swatches = [primaryColor.toUpperCase(), ...STANDARD_SWATCHES.filter((c) => c.toLowerCase() !== primaryColor.toLowerCase())];

  const clear = async () => {
    const ok = await confirm({
      title: "Hapus semua coretan?",
      body: `${strokeCount} coretan di halaman ini akan dihapus. Bisa dibatalkan dengan Urungkan (Ctrl+Z).`,
      confirmLabel: "Hapus coretan",
      danger: true,
    });
    if (ok) await onClearStrokes();
  };

  return (
    <div className={`st-draw ${className}`.trim()}>
      <div className="st-panel-head">
        <h2>Coretan</h2>
        <p>Gambar panah, lingkaran, atau stabilo langsung di halaman. Coretan ikut terbit di situs.</p>
      </div>

      <div className="st-tools2" role="group" aria-label="Pilih alat gambar">
        <button type="button" aria-pressed={activeTool === "brush"} onClick={() => onSelectTool(activeTool === "brush" ? null : "brush")}>
          <PaintBrush weight={activeTool === "brush" ? "fill" : "duotone"} aria-hidden="true" />
          <span>Kuas</span>
        </button>
        <button type="button" aria-pressed={activeTool === "eraser"} onClick={() => onSelectTool(activeTool === "eraser" ? null : "eraser")}>
          <Eraser weight={activeTool === "eraser" ? "fill" : "duotone"} aria-hidden="true" />
          <span>Penghapus</span>
        </button>
      </div>
      <p className="d-hint st-draw-tip">
        {activeTool === null
          ? "Pilih Kuas untuk mulai menggambar. Tekan Esc untuk selesai."
          : activeTool === "brush"
            ? "Gambar di kanvas sebelah kanan. Selama kuas aktif, blok tidak bisa diseret."
            : "Sapukan penghapus pada coretan untuk menghapusnya."}
      </p>

      <section className="st-group">
        <h3>Tempel coretan ke</h3>
        <div className="d-seg st-seg-full" role="group" aria-label="Target coretan">
          <button type="button" aria-pressed={target === "cell"} onClick={() => onSelectTarget("cell")}>Blok di bawahnya</button>
          <button type="button" aria-pressed={target === "background"} onClick={() => onSelectTarget("background")}>Latar halaman</button>
        </div>
        <p className="d-hint">
          {target === "cell"
            ? "Coretan menempel pada blok tempat Anda mulai menggambar dan ikut pindah bersama blok itu."
            : "Coretan berada di belakang blok, pada latar halaman."}
        </p>
      </section>

      <section className="st-group">
        <div className="st-group-row">
          <h3>Ketebalan</h3>
          <span className="st-value">{config.size}px</span>
        </div>
        <div className="st-stroke-preview" aria-hidden="true">
          <svg viewBox="0 0 240 40" preserveAspectRatio="none">
            <path d="M 16 26 Q 70 6 120 20 T 224 14" stroke={config.color} strokeWidth={config.size} strokeOpacity={config.opacity} strokeLinecap="round" fill="none" />
          </svg>
        </div>
        <div className="d-seg st-seg-full" role="group" aria-label="Ketebalan kuas">
          {SIZE_PRESETS.map((p) => (
            <button key={p.size} type="button" aria-pressed={config.size === p.size}
              onClick={() => onChangeConfig({ ...config, size: p.size, opacity: p.size >= 18 ? 0.4 : config.opacity === 0.4 ? 1 : config.opacity })}>
              {p.label}
            </button>
          ))}
        </div>
        <input type="range" min={1} max={40} value={config.size} aria-label="Ketebalan kuas (px)" className="st-range"
          onChange={(e) => onChangeConfig({ ...config, size: Number(e.target.value) })} />
      </section>

      <section className="st-group">
        <h3>Warna</h3>
        <div className="st-swatches" role="radiogroup" aria-label="Warna kuas">
          {swatches.map((sw, i) => {
            const on = config.color.toLowerCase() === sw.toLowerCase();
            return (
              <button key={sw} type="button" role="radio" aria-checked={on} className="st-sw" style={{ ["--sw" as string]: sw }}
                onClick={() => onChangeConfig({ ...config, color: sw })} title={i === 0 ? "Warna utama toko" : sw} aria-label={i === 0 ? "Warna utama toko" : sw}>
                {on && <Check weight="bold" aria-hidden="true" />}
              </button>
            );
          })}
          <label className="st-sw st-sw-custom" title="Warna lain">
            <input type="color" value={config.color.length === 7 ? config.color : "#000000"} onChange={(e) => onChangeConfig({ ...config, color: e.target.value.toUpperCase() })} aria-label="Warna lain" />
          </label>
        </div>
        <div className="d-seg st-seg-full" role="group" aria-label="Gaya coretan" style={{ marginTop: 10 }}>
          <button type="button" aria-pressed={config.opacity === 1} onClick={() => onChangeConfig({ ...config, opacity: 1 })}>Padat</button>
          <button type="button" aria-pressed={config.opacity < 1} onClick={() => onChangeConfig({ ...config, opacity: 0.4 })}>Transparan</button>
        </div>
      </section>

      <section className="st-group">
        <button type="button" className="d-btn d-btn-block st-danger-btn" onClick={() => void clear()} disabled={strokeCount === 0}>
          <Trash weight="bold" aria-hidden="true" />
          {strokeCount === 0 ? "Belum ada coretan" : `Hapus semua coretan (${strokeCount})`}
        </button>
      </section>
    </div>
  );
}
