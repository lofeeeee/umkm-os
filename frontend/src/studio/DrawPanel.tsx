"use client";

import { useId, useState } from "react";
import {
  Eraser,
  Highlighter,
  PaintBrush,
  Trash,
  X,
  Check,
} from "@phosphor-icons/react/ssr";

import { confirm } from "@/ui/dialog";

export interface DrawConfig {
  color: string;
  size: number;
  opacity: number;
}

export type DrawTool = "brush" | "eraser" | null;
export type DrawTarget = "background" | "cell";

export interface DrawPanelProps {
  activeTool: DrawTool;
  onSelectTool: (tool: DrawTool) => void;
  target: DrawTarget;
  onSelectTarget: (target: DrawTarget) => void;
  selectedCellId?: string | null;
  config: DrawConfig;
  onChangeConfig: (config: DrawConfig) => void;
  primaryColor?: string;
  onClearStrokes: () => void | Promise<void>;
  onClose?: () => void;
  className?: string;
}

export const SIZE_PRESETS = [
  { label: "Halus", size: 2, desc: "2px" },
  { label: "Standar", size: 4, desc: "4px" },
  { label: "Tebal", size: 8, desc: "8px" },
  { label: "Penanda/Stabilo", shortLabel: "Stabilo", size: 18, desc: "18px" },
];

export const STANDARD_SWATCHES = [
  "#000000",
  "#FFFFFF",
  "#EF4444",
  "#F59E0B",
  "#10B981",
  "#3B82F6",
  "#8B5CF6",
];

export function DrawPanel({
  activeTool,
  onSelectTool,
  target,
  onSelectTarget,
  selectedCellId,
  config,
  onChangeConfig,
  primaryColor = "#4F46E5",
  onClearStrokes,
  onClose,
  className = "",
}: DrawPanelProps) {
  const [isClearing, setIsClearing] = useState(false);
  const colorInputId = useId();

  const handleClear = async () => {
    try {
      setIsClearing(true);
      const ok = await confirm({
        title: "Bersihkan Coretan?",
        body:
          target === "background"
            ? "Semua coretan pada latar belakang kanvas akan dihapus."
            : "Semua coretan pada plugin yang dipilih akan dihapus.",
        confirmLabel: "Bersihkan Coretan",
        cancelLabel: "Batal",
        danger: true,
      });
      if (ok) {
        await onClearStrokes();
      }
    } catch {
      // Fallback untuk browser jika confirm dialog host tidak tersedia
      if (typeof window !== "undefined") {
        const msg =
          target === "background"
            ? "Bersihkan semua coretan pada latar belakang?"
            : "Bersihkan semua coretan pada plugin terpilih?";
        if (window.confirm(msg)) {
          await onClearStrokes();
        }
      }
    } finally {
      setIsClearing(false);
    }
  };

  const allSwatches = [primaryColor, ...STANDARD_SWATCHES.filter((c) => c.toLowerCase() !== primaryColor.toLowerCase())];

  return (
    <div
      className={`st-draw-panel bg-white border border-neutral-200 rounded-xl shadow-lg p-4 w-80 text-neutral-800 space-y-4 ${className}`.trim()}
      style={{ fontFamily: "inherit" }}
    >
      {/* Header Panel */}
      <div className="flex items-center justify-between border-b border-neutral-100 pb-2">
        <div className="flex items-center gap-2">
          <span className="p-1.5 rounded-lg bg-indigo-50 text-indigo-600">
            <PaintBrush weight="bold" className="w-4 h-4" />
          </span>
          <h3 className="font-semibold text-sm text-neutral-900">Coretan & Anotasi</h3>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded-md text-neutral-400 hover:text-neutral-600 hover:bg-neutral-100"
            title="Tutup panel"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* 1. Tool Toggle: Kuas (Brush) vs Penghapus (Eraser) */}
      <div>
        <label className="block text-xs font-medium text-neutral-500 mb-1.5">Alat Gambar</label>
        <div className="grid grid-cols-2 gap-2 bg-neutral-100 p-1 rounded-lg">
          <button
            type="button"
            onClick={() => onSelectTool(activeTool === "brush" ? null : "brush")}
            className={`flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-md text-xs font-medium transition-colors ${
              activeTool === "brush"
                ? "bg-white text-indigo-600 shadow-sm"
                : "text-neutral-600 hover:text-neutral-900"
            }`}
          >
            <PaintBrush weight={activeTool === "brush" ? "fill" : "regular"} className="w-4 h-4" />
            <span>Kuas (Brush)</span>
          </button>
          <button
            type="button"
            onClick={() => onSelectTool(activeTool === "eraser" ? null : "eraser")}
            className={`flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-md text-xs font-medium transition-colors ${
              activeTool === "eraser"
                ? "bg-white text-rose-600 shadow-sm"
                : "text-neutral-600 hover:text-neutral-900"
            }`}
          >
            <Eraser weight={activeTool === "eraser" ? "fill" : "regular"} className="w-4 h-4" />
            <span>Penghapus (Eraser)</span>
          </button>
        </div>
      </div>

      {/* 2. Target Selector: Latar Belakang (background) vs Plugin Terpilih (cell) */}
      <div>
        <label className="block text-xs font-medium text-neutral-500 mb-1.5">Target Coretan</label>
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => onSelectTarget("background")}
            className={`py-1.5 px-2.5 rounded-lg border text-xs font-medium text-center transition-all ${
              target === "background"
                ? "border-indigo-600 bg-indigo-50/50 text-indigo-700 font-semibold"
                : "border-neutral-200 hover:bg-neutral-50 text-neutral-600"
            }`}
          >
            Latar Belakang
          </button>
          <button
            type="button"
            onClick={() => onSelectTarget("cell")}
            className={`py-1.5 px-2.5 rounded-lg border text-xs font-medium text-center transition-all ${
              target === "cell"
                ? "border-indigo-600 bg-indigo-50/50 text-indigo-700 font-semibold"
                : "border-neutral-200 hover:bg-neutral-50 text-neutral-600"
            }`}
          >
            Plugin Terpilih
          </button>
        </div>
        {target === "cell" && !selectedCellId && (
          <p className="text-[11px] text-amber-600 mt-1 leading-tight">
            * Pilih salah satu plugin di kanvas untuk menempelkan coretan padanya.
          </p>
        )}
      </div>

      {/* 3. Stroke Size Presets */}
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <label className="text-xs font-medium text-neutral-500">Ukuran Garis</label>
          <span className="text-[11px] text-neutral-400 font-mono">{config.size}px</span>
        </div>
        <div className="grid grid-cols-4 gap-1.5">
          {SIZE_PRESETS.map((preset) => {
            const isSelected = config.size === preset.size;
            return (
              <button
                key={preset.size}
                type="button"
                onClick={() => onChangeConfig({ ...config, size: preset.size })}
                className={`py-1.5 px-1 rounded-lg border flex flex-col items-center justify-center gap-1 transition-all ${
                  isSelected
                    ? "border-indigo-600 bg-indigo-50/40 text-indigo-700 font-semibold shadow-xs"
                    : "border-neutral-200 hover:bg-neutral-50 text-neutral-600"
                }`}
                title={`${preset.label} (${preset.desc})`}
              >
                <div
                  className="rounded-full bg-current"
                  style={{
                    width: Math.min(preset.size, 12),
                    height: Math.min(preset.size, 12),
                  }}
                />
                <span className="text-[10px] truncate max-w-full">
                  {preset.shortLabel || preset.label}
                </span>
                <span className="text-[9px] text-neutral-400 font-mono -mt-1">{preset.desc}</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 4. Color Picker: Theme Primary + Swatches + Custom */}
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <label className="text-xs font-medium text-neutral-500">Warna Coretan</label>
          <div className="flex items-center gap-1.5">
            <span
              className="w-3.5 h-3.5 rounded-full border border-neutral-300 shadow-xs inline-block"
              style={{ backgroundColor: config.color }}
            />
            <span className="text-[11px] font-mono text-neutral-600 uppercase">{config.color}</span>
          </div>
        </div>

        <div className="flex flex-wrap gap-1.5 items-center">
          {allSwatches.map((hex, idx) => {
            const isSelected = config.color.toLowerCase() === hex.toLowerCase();
            return (
              <button
                key={`${hex}-${idx}`}
                type="button"
                onClick={() => onChangeConfig({ ...config, color: hex })}
                className={`w-6 h-6 rounded-full border flex items-center justify-center transition-transform hover:scale-110 relative ${
                  isSelected
                    ? "ring-2 ring-indigo-500 ring-offset-1 border-transparent"
                    : "border-neutral-300"
                }`}
                style={{ backgroundColor: hex }}
                title={idx === 0 ? `Warna Tema Primer (${hex})` : hex}
              >
                {isSelected && (
                  <Check
                    weight="bold"
                    className={`w-3 h-3 ${
                      hex.toLowerCase() === "#ffffff" ? "text-neutral-800" : "text-white"
                    }`}
                  />
                )}
              </button>
            );
          })}

          {/* Input warna kustom */}
          <label
            htmlFor={colorInputId}
            className="w-6 h-6 rounded-full border border-neutral-300 bg-linear-to-br from-rose-400 via-amber-300 to-indigo-400 cursor-pointer flex items-center justify-center hover:scale-110 transition-transform shadow-xs"
            title="Pilih warna kustom"
          >
            <input
              id={colorInputId}
              type="color"
              value={config.color}
              onChange={(e) => onChangeConfig({ ...config, color: e.target.value })}
              className="sr-only"
            />
          </label>
        </div>
      </div>

      {/* 5. Opacity Mode: Padat (100%) vs Stabilo/Highlighter (40%) */}
      <div>
        <label className="block text-xs font-medium text-neutral-500 mb-1.5">Mode Opasitas</label>
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => onChangeConfig({ ...config, opacity: 1 })}
            className={`py-1.5 px-2 rounded-lg border text-xs font-medium flex items-center justify-center gap-1.5 transition-all ${
              config.opacity >= 0.9
                ? "border-indigo-600 bg-indigo-50/50 text-indigo-700 font-semibold"
                : "border-neutral-200 hover:bg-neutral-50 text-neutral-600"
            }`}
          >
            <span className="w-2.5 h-2.5 rounded-full bg-neutral-800 inline-block" />
            <span>Padat (100%)</span>
          </button>
          <button
            type="button"
            onClick={() => onChangeConfig({ ...config, opacity: 0.4 })}
            className={`py-1.5 px-2 rounded-lg border text-xs font-medium flex items-center justify-center gap-1.5 transition-all ${
              config.opacity < 0.9
                ? "border-indigo-600 bg-indigo-50/50 text-indigo-700 font-semibold"
                : "border-neutral-200 hover:bg-neutral-50 text-neutral-600"
            }`}
          >
            <Highlighter className="w-3.5 h-3.5" />
            <span>Stabilo (40%)</span>
          </button>
        </div>
      </div>

      {/* 6. Bersihkan Coretan */}
      <div className="pt-2 border-t border-neutral-100">
        <button
          type="button"
          onClick={handleClear}
          disabled={isClearing}
          className="w-full flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg border border-rose-200 text-rose-600 hover:bg-rose-50 text-xs font-medium transition-colors cursor-pointer disabled:opacity-50"
        >
          <Trash className="w-4 h-4" />
          <span>Bersihkan Coretan</span>
        </button>
      </div>
    </div>
  );
}
