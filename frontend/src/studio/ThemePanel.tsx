import type { ChangeEvent } from "react";
import { Trash, UploadSimple } from "@phosphor-icons/react/ssr";

import { FONT_PRESETS } from "@/renderer/sections";
import { PALETTES } from "@/studio/plugins";
import { t as tx } from "@/i18n";

export interface ThemeConfig {
  primary: string;
  font: string;
  surface: "light" | "cream" | "dark";
  textScale: number;
  navbarStyle: "top" | "side";
  navbarSticky: boolean;
  backgroundImage?: string;
  backgroundSize?: "cover" | "contain" | "repeat";
  backgroundPosition?: string;
  backgroundOpacity?: number; // 0..100
}

export interface ThemePanelProps {
  config: ThemeConfig;
  onChange: (patch: Partial<ThemeConfig>) => void;
  onUploadBackground: (file: File) => Promise<string | null>; // returns asset:uuid
  assetUrl: (ref: string) => string | undefined;
  className?: string;
}

export const TEXT_SCALES = [
  { v: 0.9, get l() { return tx("Kecil"); } },
  { v: 1, get l() { return tx("Sedang"); } },
  { v: 1.1, get l() { return tx("Besar"); } },
];

export const BACKGROUND_SIZE_OPTIONS: {
  value: "cover" | "contain" | "repeat";
  label: string;
}[] = [
  { value: "cover", get label() { return tx("Penuh"); } },
  { value: "contain", get label() { return tx("Proporsional"); } },
  { value: "repeat", get label() { return tx("Motif berulang"); } },
];

export const BACKGROUND_POSITION_OPTIONS = [
  { value: "center", get label() { return tx("Tengah"); } },
  { value: "top", get label() { return tx("Atas"); } },
  { value: "bottom", get label() { return tx("Bawah"); } },
  { value: "left", get label() { return tx("Kiri"); } },
  { value: "right", get label() { return tx("Kanan"); } },
];

export function ThemePanel({
  config,
  onChange,
  onUploadBackground,
  assetUrl,
  className = "",
}: ThemePanelProps) {
  const bgUrl = config.backgroundImage
    ? assetUrl(config.backgroundImage)
    : undefined;

  const handleFileUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const ref = await onUploadBackground(file);
    if (ref) {
      onChange({ backgroundImage: ref });
    }
  };

  return (
    <div className={`st-theme-panel ${className}`.trim()}>
      {/* 1. Navigasi Website (Navbar) */}
      <div className="st-section">
        <h3>{tx("Navigasi website")}</h3>
        <p className="d-hint" style={{ marginTop: 0 }}>
          {tx("Pilih tata letak bilah menu dan tautan navigasi website toko.")}
        </p>

        <div className="d-field" style={{ marginBottom: 12 }}>
          <span>{tx("Gaya tata letak")}</span>
          <div
            className="d-seg"
            role="group"
            aria-label={tx("Gaya tata letak navigasi")}
            style={{ width: "100%" }}
          >
            <button
              type="button"
              style={{ flex: 1 }}
              aria-pressed={config.navbarStyle === "top"}
              onClick={() => onChange({ navbarStyle: "top" })}
            >
              {tx("Bilah atas")}
            </button>
            <button
              type="button"
              style={{ flex: 1 }}
              aria-pressed={config.navbarStyle === "side"}
              onClick={() => onChange({ navbarStyle: "side" })}
            >
              {tx("Bilah samping")}
            </button>
          </div>
        </div>

        <label className="d-switch" style={{ margin: 0 }}>
          <input
            type="checkbox"
            aria-label={tx("Navigasi menempel saat digulir")}
            checked={config.navbarSticky}
            onChange={(e) => onChange({ navbarSticky: e.target.checked })}
          />
          <span>
            <strong>{tx("Navigasi menempel saat digulir")}</strong>
            <span className="d-muted">
              {tx("Bilah navigasi tetap terlihat saat pengunjung menggulir halaman.")}
            </span>
          </span>
        </label>
      </div>

      {/* 2. Warna & Palet */}
      <div className="st-section">
        <h3>{tx("Warna & palet")}</h3>
        <p className="d-hint" style={{ marginTop: 0 }}>
          {tx("Pilih kombinasi warna utama dan latar permukaan website.")}
        </p>

        <div className="st-palettes">
          {PALETTES.map((p) => (
            <button
              key={p.name}
              type="button"
              className="st-swatch"
              aria-pressed={
                config.primary === p.primary && config.surface === p.surface
              }
              title={p.name}
              onClick={() =>
                onChange({ primary: p.primary, surface: p.surface })
              }
              style={{
                ["--sw" as string]: p.primary,
                ["--s-on" as string]: p.onPrimary,
                ["--sf" as string]:
                  p.surface === "dark"
                    ? "#111118"
                    : p.surface === "cream"
                      ? "#fbf6ee"
                      : "#ffffff",
              }}
            >
              <span aria-hidden="true" />
              <em>{p.name}</em>
            </button>
          ))}
        </div>

        <div className="d-grid" style={{ marginTop: 10 }}>
          <label className="d-field" style={{ margin: 0 }}>
            <span>{tx("Warna bebas")}</span>
            <span className="d-color">
              <input
                type="color"
                aria-label={tx("Warna bebas")}
                value={config.primary}
                onChange={(e) =>
                  onChange({ primary: e.target.value.toUpperCase() })
                }
              />
              <code>{config.primary}</code>
            </span>
          </label>
          <label className="d-field" style={{ margin: 0 }}>
            <span>{tx("Latar")}</span>
            <select
              aria-label={tx("Latar permukaan")}
              value={config.surface}
              onChange={(e) =>
                onChange({
                  surface: e.target.value as "light" | "cream" | "dark",
                })
              }
            >
              <option value="light">{tx("Putih")}</option>
              <option value="cream">{tx("Krem")}</option>
              <option value="dark">{tx("Gelap")}</option>
            </select>
          </label>
        </div>
      </div>

      {/* 3. Tipografi */}
      <div className="st-section">
        <h3>{tx("Tipografi")}</h3>
        <label className="d-field" style={{ marginTop: 6 }}>
          <span>{tx("Font")}</span>
          <select
            aria-label={tx("Pilihan font")}
            value={config.font}
            onChange={(e) => onChange({ font: e.target.value })}
          >
            {FONT_PRESETS.map((f) => (
              <option key={f} value={f}>
                {f}
              </option>
            ))}
          </select>
        </label>

        <div className="d-field" style={{ marginTop: 10, marginBottom: 0 }}>
          <span>{tx("Ukuran teks")}</span>
          <div
            className="d-seg"
            role="group"
            aria-label={tx("Ukuran teks")}
            style={{ width: "100%" }}
          >
            {TEXT_SCALES.map((t) => (
              <button
                key={t.v}
                type="button"
                style={{ flex: 1 }}
                aria-pressed={(config.textScale ?? 1) === t.v}
                onClick={() => onChange({ textScale: t.v })}
              >
                <span style={{ fontSize: `${t.v * 0.8125}rem` }}>{t.l}</span>
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* 4. Latar Belakang Gambar & In-Through */}
      <div className="st-section">
        <h3>{tx("Gambar latar")}</h3>
        <p className="d-hint" style={{ marginTop: 0 }}>
          {tx("Gunakan gambar latar website toko dan atur transparansi tembus pandang.")}
        </p>

        {config.backgroundImage && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              marginBottom: 12,
              padding: 8,
              border: "1px solid var(--line)",
              borderRadius: "var(--rad-sm, 6px)",
              background: "var(--surface)",
            }}
          >
            {bgUrl ? (
              // eslint-disable-next-line @next/next/no-img-element -- Thumbnail pratinjau latar statis / signed
              <img
                src={bgUrl}
                alt={tx("Pratinjau gambar latar")}
                style={{
                  width: 60,
                  height: 44,
                  objectFit: "cover",
                  borderRadius: 4,
                  border: "1px solid var(--line)",
                  flex: "none",
                }}
              />
            ) : (
              <span
                style={{
                  width: 60,
                  height: 44,
                  borderRadius: 4,
                  background: "var(--line)",
                  display: "inline-block",
                  flex: "none",
                }}
              />
            )}
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                className="d-small"
                style={{
                  fontWeight: 600,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {tx("Foto latar aktif")}
              </div>
              <button
                type="button"
                className="d-btn d-btn-sm d-btn-ghost"
                style={{
                  color: "var(--danger, #ef4444)",
                  padding: "2px 6px",
                  marginTop: 4,
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                }}
                onClick={() => onChange({ backgroundImage: undefined })}
              >
                <Trash size={14} weight="bold" aria-hidden="true" />
                {tx("Hapus latar")}
              </button>
            </div>
          </div>
        )}

        <div style={{ marginBottom: 12 }}>
          <label
            className="d-btn d-btn-sm"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              cursor: "pointer",
            }}
          >
            <UploadSimple size={16} weight="bold" aria-hidden="true" />
            <span>
              {config.backgroundImage ? tx("Ganti foto latar") : tx("Unggah foto latar")}
            </span>
            <input
              type="file"
              accept="image/*"
              style={{ display: "none" }}
              onChange={handleFileUpload}
            />
          </label>
        </div>

        <div className="d-grid" style={{ marginBottom: 12 }}>
          <label className="d-field" style={{ margin: 0 }}>
            <span>{tx("Pilihan ukuran")}</span>
            <select
              aria-label={tx("Ukuran gambar latar")}
              value={config.backgroundSize ?? "cover"}
              onChange={(e) =>
                onChange({
                  backgroundSize: e.target.value as
                    | "cover"
                    | "contain"
                    | "repeat",
                })
              }
            >
              {BACKGROUND_SIZE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>

          <label className="d-field" style={{ margin: 0 }}>
            <span>{tx("Pilihan posisi")}</span>
            <select
              aria-label={tx("Posisi gambar latar")}
              value={config.backgroundPosition ?? "center"}
              onChange={(e) => onChange({ backgroundPosition: e.target.value })}
            >
              {BACKGROUND_POSITION_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        <label className="d-field" style={{ margin: 0 }}>
          <span
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span>{tx("Kepekatan gambar latar")}</span>
            <strong>{config.backgroundOpacity ?? 0}%</strong>
          </span>
          <input
            type="range"
            min={0}
            max={90}
            step={5}
            aria-label={tx("Kepekatan gambar latar")}
            value={config.backgroundOpacity ?? 0}
            onChange={(e) =>
              onChange({ backgroundOpacity: Number(e.target.value) })
            }
          />
          <span className="d-hint" style={{ marginTop: 4 }}>
            {tx("Transparansi tembus pandang pada blok website")}
          </span>
        </label>
      </div>
    </div>
  );
}
