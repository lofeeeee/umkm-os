"use client";

import { useId, useState } from "react";
import {
  Eraser,
  Highlighter,
  PaintBrush,
  Trash,
  X,
  Check,
  Circle,
  Selection,
  SquaresFour,
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
  { label: "Stabilo", size: 18, desc: "18px" },
];

export const STANDARD_SWATCHES = [
  "#000000",
  "#EF4444",
  "#F59E0B",
  "#10B981",
  "#3B82F6",
  "#8B5CF6",
  "#EC4899",
  "#FFFFFF",
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
  const colorId = useId();

  const handleClear = async () => {
    try {
      setIsClearing(true);
      const ok = await confirm({
        title: "Bersihkan Semua Coretan?",
        body:
          target === "background"
            ? "Semua coretan pada latar belakang kanvas akan dihapus permanen."
            : "Semua coretan pada plugin terpilih akan dihapus permanen.",
        confirmLabel: "Hapus Coretan",
        cancelLabel: "Batal",
        danger: true,
      });
      if (ok) {
        await onClearStrokes();
      }
    } catch {
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

  const allSwatches = [
    primaryColor,
    ...STANDARD_SWATCHES.filter((c) => c.toLowerCase() !== primaryColor.toLowerCase()),
  ];

  return (
    <div className={`st-draw-panel-redesign ${className}`.trim()}>
      {/* 1. Header & Tool Selector */}
      <div className="st-section" style={{ paddingTop: 0 }}>
        <div className="d-row d-row-between" style={{ marginBottom: 12 }}>
          <h2 style={{ margin: 0, fontSize: "0.9375rem", display: "flex", alignItems: "center", gap: 8 }}>
            <PaintBrush weight="bold" size={18} style={{ color: "var(--brand, #4f46e5)" }} />
            <span>Coretan Bebas</span>
          </h2>
          {onClose && (
            <button type="button" className="d-icon-btn d-icon-btn-sm" onClick={onClose} title="Tutup">
              <X size={16} />
            </button>
          )}
        </div>

        <p className="d-hint" style={{ margin: "0 0 12px" }}>
          Gambar aksen, stabilo, panah, atau dekorasi bebas langsung di atas kanvas toko.
        </p>

        {/* Alat: Kuas vs Penghapus */}
        <div className="d-seg" role="group" aria-label="Pilih alat gambar" style={{ width: "100%" }}>
          <button
            type="button"
            style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", gap: 6 }}
            aria-pressed={activeTool === "brush"}
            onClick={() => onSelectTool(activeTool === "brush" ? null : "brush")}
          >
            <PaintBrush weight={activeTool === "brush" ? "fill" : "bold"} size={16} />
            <span>Kuas (Brush)</span>
          </button>
          <button
            type="button"
            style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", gap: 6 }}
            aria-pressed={activeTool === "eraser"}
            onClick={() => onSelectTool(activeTool === "eraser" ? null : "eraser")}
          >
            <Eraser weight={activeTool === "eraser" ? "fill" : "bold"} size={16} />
            <span>Penghapus</span>
          </button>
        </div>
      </div>

      {/* 2. Target Lapisan */}
      <div className="st-section">
        <h3>Lapisan Target</h3>
        <div className="d-seg" role="group" aria-label="Target lapisan coretan" style={{ width: "100%" }}>
          <button
            type="button"
            style={{ flex: 1 }}
            aria-pressed={target === "background"}
            onClick={() => onSelectTarget("background")}
          >
            Latar Belakang
          </button>
          <button
            type="button"
            style={{ flex: 1 }}
            aria-pressed={target === "cell"}
            onClick={() => onSelectTarget("cell")}
          >
            Plugin Terpilih
          </button>
        </div>
        <p className="d-hint" style={{ marginTop: 8, marginBottom: 0 }}>
          {target === "background"
            ? "Coretan berada di lapisan latar, di bawah blok konten website."
            : selectedCellId
              ? "Coretan menempel di plugin terpilih dan ikut bergerak saat plugin digeser."
              : "Pilih salah satu plugin di kanvas untuk menempelkan coretan padanya."}
        </p>
      </div>

      {/* 3. Pratinjau Goresan & Ukuran */}
      <div className="st-section">
        <div className="d-row d-row-between" style={{ alignItems: "center", marginBottom: 8 }}>
          <h3 style={{ margin: 0 }}>Ketebalan Kuas</h3>
          <span className="d-muted" style={{ fontSize: "0.75rem", fontFamily: "monospace" }}>
            {config.size}px
          </span>
        </div>

        {/* Live Stroke Preview Card */}
        <div
          style={{
            height: 44,
            borderRadius: 8,
            border: "1px solid var(--line, #e4e4e7)",
            background: "var(--bg-subtle, rgba(0,0,0,0.02))",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            marginBottom: 12,
            overflow: "hidden",
          }}
        >
          <svg width="240" height="40" viewBox="0 0 240 40">
            <path
              d="M 20 20 Q 80 8 120 20 T 220 20"
              stroke={config.color}
              strokeWidth={config.size}
              strokeOpacity={config.opacity}
              strokeLinecap="round"
              fill="none"
            />
          </svg>
        </div>

        {/* Presets */}
        <div className="d-seg" role="group" aria-label="Preset ketebalan" style={{ width: "100%" }}>
          {SIZE_PRESETS.map((p) => (
            <button
              key={p.size}
              type="button"
              style={{ flex: 1 }}
              aria-pressed={config.size === p.size}
              onClick={() => onChangeConfig({ ...config, size: p.size })}
              title={`${p.label} (${p.desc})`}
            >
              <span style={{ fontSize: "0.75rem" }}>{p.label}</span>
            </button>
          ))}
        </div>
      </div>

      {/* 4. Warna & Opasitas */}
      <div className="st-section">
        <h3>Warna & Opasitas</h3>

        {/* Swatches Grid */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(5, 1fr)",
            gap: 8,
            marginBottom: 12,
          }}
        >
          {allSwatches.map((swatch, idx) => {
            const isSelected = config.color.toLowerCase() === swatch.toLowerCase();
            return (
              <button
                key={swatch}
                type="button"
                onClick={() => onChangeConfig({ ...config, color: swatch })}
                style={{
                  width: "100%",
                  aspectRatio: "1",
                  borderRadius: "50%",
                  backgroundColor: swatch,
                  border: swatch.toUpperCase() === "#FFFFFF" ? "1px solid #d4d4d8" : "none",
                  boxShadow: isSelected
                    ? "0 0 0 2px var(--card, #fff), 0 0 0 4px var(--brand, #4f46e5)"
                    : "0 1px 3px rgba(0,0,0,0.12)",
                  cursor: "pointer",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  transition: "transform 0.1s ease",
                  outline: "none",
                }}
                title={idx === 0 ? "Warna Utama Tema" : swatch}
              >
                {isSelected && (
                  <Check
                    size={14}
                    weight="bold"
                    style={{
                      color: ["#ffffff", "#f59e0b", "#ffff00"].includes(swatch.toLowerCase()) ? "#18181b" : "#ffffff",
                    }}
                  />
                )}
              </button>
            );
          })}
        </div>

        {/* Kustom Warna & Mode Opasitas */}
        <div className="d-grid" style={{ marginBottom: 12 }}>
          <label className="d-field" style={{ margin: 0 }}>
            <span>Warna kustom</span>
            <span className="d-color">
              <input
                id={colorId}
                type="color"
                value={config.color}
                onChange={(e) => onChangeConfig({ ...config, color: e.target.value })}
              />
              <code>{config.color}</code>
            </span>
          </label>

          <div className="d-field" style={{ margin: 0 }}>
            <span>Gaya coretan</span>
            <div className="d-seg" role="group" aria-label="Gaya kuas" style={{ width: "100%" }}>
              <button
                type="button"
                style={{ flex: 1 }}
                aria-pressed={config.opacity === 1}
                onClick={() => onChangeConfig({ ...config, opacity: 1 })}
              >
                Padat (100%)
              </button>
              <button
                type="button"
                style={{ flex: 1 }}
                aria-pressed={config.opacity < 1}
                onClick={() => onChangeConfig({ ...config, opacity: 0.4 })}
              >
                Stabilo (40%)
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* 5. Aksi Bersihkan */}
      <div className="st-section" style={{ borderBottom: "none" }}>
        <button
          type="button"
          className="d-btn d-btn-block"
          style={{
            borderColor: "var(--danger, #ef4444)",
            color: "var(--danger, #ef4444)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
          }}
          onClick={handleClear}
          disabled={isClearing}
        >
          <Trash size={16} weight="bold" />
          <span>Bersihkan Coretan ({target === "background" ? "Latar" : "Plugin"})</span>
        </button>
      </div>
    </div>
  );
}
