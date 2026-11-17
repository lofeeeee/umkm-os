import type { ChangeEvent } from "react";
import { Crown, LockSimple, Trash, UploadSimple } from "@phosphor-icons/react/ssr";

import { FONT_LABELS, FONT_PRESETS } from "@/renderer/sections";
import { PALETTES } from "@/studio/plugins";
import { t as tx } from "@/i18n";

export interface ThemeConfig {
  primary: string;
  font: string;
  surface: "light" | "cream" | "dark";
  textScale: number;
  navbarStyle: "top" | "side";
  navbarSticky: boolean;
  hideBadge: boolean;
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
  /** Paket Pro: boleh menyembunyikan lencana "Dibuat dengan UMKM OS". */
  isPro?: boolean;
  onUpgradeClick?: () => void;
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
  isPro = false,
  onUpgradeClick,
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
        <p className="d-hint u-mt-0">
          {tx("Pilih tata letak bilah menu dan tautan navigasi website toko.")}
        </p>

        <div className="d-field u-mb-3">
          <span>{tx("Gaya tata letak")}</span>
          <div
            className="d-seg u-w-full"
            role="group"
            aria-label={tx("Gaya tata letak navigasi")}
          >
            <button
              type="button"
              className="u-grow"
              aria-pressed={config.navbarStyle === "top"}
              onClick={() => onChange({ navbarStyle: "top" })}
            >
              {tx("Bilah atas")}
            </button>
            <button
              type="button"
              className="u-grow"
              aria-pressed={config.navbarStyle === "side"}
              onClick={() => onChange({ navbarStyle: "side" })}
            >
              {tx("Bilah samping")}
            </button>
          </div>
        </div>

        <label className="d-switch u-m-0">
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

      {/* 1b. Lencana platform di footer (Pro boleh menyembunyikan; server menegakkan). */}
      <div className="st-section">
        <h3>
          {tx("Lencana footer")}
          {!isPro && <span className="st-pro-chip"><Crown weight="fill" aria-hidden="true" />{tx("Paket Pro")}</span>}
        </h3>
        <label className="d-switch u-m-0">
          <input
            type="checkbox"
            aria-label={tx("Sembunyikan lencana “Dibuat dengan UMKM OS”")}
            checked={isPro && config.hideBadge}
            disabled={!isPro}
            onChange={(e) => onChange({ hideBadge: e.target.checked })}
          />
          <span>
            <strong>{tx("Sembunyikan lencana “Dibuat dengan UMKM OS”")}</strong>
            <span className="d-muted">
              {tx("Footer situs hanya menampilkan merek toko Anda.")}
            </span>
          </span>
        </label>
        {!isPro && onUpgradeClick && (
          <div className="st-pro-note">
            <p>{tx("Menyembunyikan lencana memerlukan paket Pro.")}</p>
            <button type="button" className="d-btn d-btn-sm" onClick={onUpgradeClick}>
              <LockSimple weight="bold" aria-hidden="true" />
              {tx("Tingkatkan ke Pro")}
            </button>
          </div>
        )}
      </div>

      {/* 2. Warna & Palet */}
      <div className="st-section">
        <h3>{tx("Warna & palet")}</h3>
        <p className="d-hint u-mt-0">
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

        <div className="d-grid u-mt-2">
          <label className="d-field u-m-0">
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
          <label className="d-field u-m-0">
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
        <label className="d-field u-mt-2">
          <span>{tx("Font")}</span>
          <select
            aria-label={tx("Pilihan font")}
            value={config.font}
            onChange={(e) => onChange({ font: e.target.value })}
          >
            {FONT_PRESETS.map((f) => (
              <option key={f} value={f}>
                {FONT_LABELS[f] ?? f}
              </option>
            ))}
          </select>
        </label>

        <div className="d-field u-mt-2 u-mb-0">
          <span>{tx("Ukuran teks")}</span>
          <div
            className="d-seg u-w-full"
            role="group"
            aria-label={tx("Ukuran teks")}
          >
            {TEXT_SCALES.map((t) => (
              <button
                key={t.v}
                type="button"
                className="u-grow"
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
        <p className="d-hint u-mt-0">
          {tx("Gunakan gambar latar website toko dan atur transparansi tembus pandang.")}
        </p>

        {config.backgroundImage && (
          <div
            className="st-bg-row"
          >
            {bgUrl ? (
              // eslint-disable-next-line @next/next/no-img-element -- Thumbnail pratinjau latar statis / signed
              <img
                src={bgUrl}
                alt={tx("Pratinjau gambar latar")}
                className="st-bg-thumb"
              />
            ) : (
              <span
                className="st-bg-thumb st-bg-thumb-empty"
              />
            )}
            <div className="u-grow u-min-w-0">
              <div
                className="d-small st-bg-name"

              >
                {tx("Foto latar aktif")}
              </div>
              <button
                type="button"
                className="d-btn d-btn-sm d-btn-ghost st-bg-remove"
                onClick={() => onChange({ backgroundImage: undefined })}
              >
                <Trash size={14} weight="bold" aria-hidden="true" />
                {tx("Hapus latar")}
              </button>
            </div>
          </div>
        )}

        <div className="u-mb-3">
          <label
            className="d-btn d-btn-sm st-bg-upload"

          >
            <UploadSimple size={16} weight="bold" aria-hidden="true" />
            <span>
              {config.backgroundImage ? tx("Ganti foto latar") : tx("Unggah foto latar")}
            </span>
            <input
              type="file"
              accept="image/*"
              className="u-hide"
              onChange={handleFileUpload}
            />
          </label>
        </div>

        <div className="d-grid u-mb-3">
          <label className="d-field u-m-0">
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

          <label className="d-field u-m-0">
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

        <label className="d-field u-m-0">
          <span
            className="st-row-between"
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
          <span className="d-hint u-mt-1">
            {tx("Transparansi tembus pandang pada blok website")}
          </span>
        </label>
      </div>
    </div>
  );
}
