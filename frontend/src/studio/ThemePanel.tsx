import { Crown, ImageSquare, LockSimple } from "@phosphor-icons/react/ssr";

import { FONT_LABELS, FONT_PRESETS } from "@/renderer/sections";
import { contrast, safeColor } from "@/renderer/safe";
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
  headingFont?: string;
  headingWeight?: number;
  accent?: string;
  gradient?: boolean;
  radius?: "sharp" | "soft" | "round";
  motion?: "none" | "subtle" | "lively";
  texture?: "none" | "grain" | "paper" | "dots";
  pageTransition?: boolean;
  backgroundImage?: string;
  backgroundSize?: "cover" | "contain" | "repeat";
  backgroundPosition?: string;
  backgroundOpacity?: number; // 0..100
}

export interface ThemePanelProps {
  config: ThemeConfig;
  onChange: (patch: Partial<ThemeConfig>) => void;
  /** Tambah plugin Latar (memindahkan gambar latar tema lama bila ada). */
  onAddBackground?: () => void;
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

const SURFACE_BG: Record<ThemeConfig["surface"], string> = { light: "#ffffff", cream: "#fbf6ee", dark: "#0d0d12" };

export function ThemePanel({
  config,
  onChange,
  onAddBackground,
  className = "",
  isPro = false,
  onUpgradeClick,
}: ThemePanelProps) {
  const pageBg = SURFACE_BG[config.surface];
  const lowContrast = [
    { label: tx("Warna utama"), c: config.primary },
    ...(config.accent ? [{ label: tx("Warna aksen"), c: config.accent }] : []),
  ].map((x) => ({ ...x, r: contrast(safeColor(x.c), pageBg) })).filter((x) => x.r < 4.5);
  return (
    <div className={`st-theme-panel ${className}`.trim()}>
      <p className="d-hint u-mt-0">{tx("Tema berlaku untuk seluruh situs: semua halaman memakai warna, font, dan gaya yang sama.")}</p>
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

      <div className="st-section">
        <h3>{tx("Warna aksen")}</h3>
        <div className="d-grid">
          <label className="d-field u-m-0">
            <span>{tx("Aksen (harga, sorotan, gradasi)")}</span>
            <span className="d-color">
              <input type="color" aria-label={tx("Warna aksen")} value={config.accent ?? config.primary}
                onChange={(e) => onChange({ accent: e.target.value.toUpperCase() })} />
              <code>{config.accent ?? tx("Sama dengan utama")}</code>
            </span>
          </label>
          {config.accent && (
            <button type="button" className="d-btn d-btn-sm d-btn-ghost u-as-end" onClick={() => onChange({ accent: undefined, gradient: false })}>{tx("Samakan dengan utama")}</button>
          )}
        </div>
        <label className="d-switch u-mt-2">
          <input type="checkbox" checked={!!config.gradient} onChange={(e) => onChange({ gradient: e.target.checked })} />
          <span><strong>{tx("Gradasi tombol & promo")}</strong><span className="d-muted">{tx("Tombol dan kotak promo memakai gradasi warna utama → aksen.")}</span></span>
        </label>
        {lowContrast.map((x) => (
          <p key={x.label} className="st-contrast-warn" role="status">
            {tx("{0} kurang kontras di atas latar ({1}:1, minimal 4,5:1), sehingga tautan dan harga sulit dibaca. Pilih warna yang lebih gelap atau ganti latar.", x.label, x.r.toFixed(1))}
          </p>
        ))}
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

        <label className="d-field u-mt-2">
          <span>{tx("Font judul")}</span>
          <select aria-label={tx("Font judul")} value={config.headingFont ?? ""} onChange={(e) => onChange({ headingFont: e.target.value || undefined })}>
            <option value="">{tx("Sama dengan teks")}</option>
            {FONT_PRESETS.map((f) => <option key={f} value={f}>{FONT_LABELS[f] ?? f}</option>)}
          </select>
        </label>
        <div className="d-field u-mt-2">
          <span>{tx("Ketebalan judul")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Ketebalan judul")}>
            {([[500, tx("Ringan")], [600, tx("Sedang")], [700, tx("Tebal")], [800, tx("Sangat tebal")]] as const).map(([w, l]) => (
              <button key={w} type="button" className="u-grow" aria-pressed={(config.headingWeight ?? 800) === w} onClick={() => onChange({ headingWeight: w === 800 ? undefined : w })}>
                <span style={{ fontWeight: w }}>{l}</span>
              </button>
            ))}
          </div>
        </div>
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

      {/* Bentuk, gerak & tekstur */}
      <div className="st-section">
        <h3>{tx("Bentuk, gerak & tekstur")}</h3>
        <div className="d-field">
          <span>{tx("Sudut kotak & tombol")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Sudut kotak & tombol")}>
            {([["sharp", tx("Tegas")], ["soft", tx("Lembut")], ["round", tx("Bulat")]] as const).map(([v, l]) => (
              <button key={v} type="button" className="u-grow" aria-pressed={(config.radius ?? "soft") === v} onClick={() => onChange({ radius: v })}>{l}</button>
            ))}
          </div>
        </div>
        <div className="d-field u-mt-2">
          <span>{tx("Gerak saat digulir")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Gerak saat digulir")}>
            {([["none", tx("Tanpa")], ["subtle", tx("Halus")], ["lively", tx("Hidup")]] as const).map(([v, l]) => (
              <button key={v} type="button" className="u-grow" aria-pressed={(config.motion ?? "none") === v} onClick={() => onChange({ motion: v })}>{l}</button>
            ))}
          </div>
          <span className="d-hint u-mt-1">{tx("Blok muncul perlahan saat pembeli menggulir. Otomatis mati untuk pengunjung yang memilih kurangi gerak.")}</span>
        </div>
        <label className="d-switch u-mt-2">
          <input type="checkbox" checked={!!config.pageTransition} onChange={(e) => onChange({ pageTransition: e.target.checked })} />
          <span><strong>{tx("Transisi antarhalaman")}</strong><span className="d-muted">{tx("Perpindahan halaman memudar halus (browser modern).")}</span></span>
        </label>
        <div className="d-field u-mt-2 u-mb-0">
          <span>{tx("Tekstur halaman")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Tekstur halaman")}>
            {([["none", tx("Polos")], ["grain", tx("Bintik film")], ["paper", tx("Kertas")], ["dots", tx("Titik")]] as const).map(([v, l]) => (
              <button key={v} type="button" className="u-grow" aria-pressed={(config.texture ?? "none") === v} onClick={() => onChange({ texture: v })}>{l}</button>
            ))}
          </div>
        </div>
      </div>

      {/* 4. Gambar latar → plugin Latar (lapisan tembus di belakang blok, bisa diseret & diubah ukurannya). */}
      <div className="st-section">
        <h3>{tx("Gambar latar")}</h3>
        <p className="d-hint u-mt-0">
          {tx("Gambar latar sekarang berupa plugin Latar: seret, pindah, dan ubah ukurannya di kanvas. Latar selalu berada di belakang blok lain dan tidak pernah menutupinya.")}
        </p>
        {config.backgroundImage && (
          <p className="d-hint u-mt-0">
            {tx("Halaman ini masih menyimpan gambar latar dari pengaturan lama. Pindahkan ke plugin Latar agar tampil dan bisa diatur.")}
          </p>
        )}
        {onAddBackground && (
          <button type="button" className="d-btn d-btn-sm" onClick={onAddBackground}>
            <ImageSquare size={16} weight="bold" aria-hidden="true" />
            {config.backgroundImage ? tx("Pindahkan ke plugin Latar") : tx("Tambah plugin Latar")}
          </button>
        )}
      </div>
    </div>
  );
}
