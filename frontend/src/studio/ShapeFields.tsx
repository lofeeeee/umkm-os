"use client";

// Pengaturan plugin Bentuk & Logo di Inspector.

import { t as tx } from "@/i18n";
import { SHAPE_DEFS } from "@/renderer/layers";
import type { StudioCell } from "@/studio/editor-model";
import { pluginDef } from "@/studio/plugins";

const SHAPE_LABEL = (): Record<string, string> => ({
  circle: tx("Lingkaran"), blob: tx("Organik"), star: tx("Bintang"), sparkle: tx("Kilau"), ring: tx("Cincin"),
  arrow: tx("Panah"), wave: tx("Gelombang"), burst: tx("Ledakan"), squiggle: tx("Coretan"), badge: tx("Lencana"),
});

export function ShapeFields({ c, onChange, onLayout }: { c: StudioCell; onChange: (patch: Partial<StudioCell>, key?: string) => void; onLayout: (l: string) => void }) {
  const set = (k: string, v: string | number, key = `shape-${k}`) => onChange({ data: { ...c.data, [k]: v } }, key);
  const color = typeof c.data.color === "string" ? c.data.color : "primary";
  const labels = SHAPE_LABEL();
  return (
    <div className="st-section">
      <h3>{tx("Bentuk")}</h3>
      <div className="st-shape-grid" role="group" aria-label={tx("Pilih bentuk")}>
        {Object.entries(SHAPE_DEFS).map(([k, d]) => (
          <button key={k} type="button" className="st-shape-tile" aria-pressed={c.data.shape === k} title={labels[k]} aria-label={labels[k]} onClick={() => set("shape", k)}>
            <svg viewBox="0 0 100 100" aria-hidden="true"><path d={d.d} fill={d.stroke ? "none" : "currentColor"} stroke={d.stroke ? "currentColor" : undefined} strokeWidth={d.stroke ? 9 : undefined} fillRule={d.evenodd ? "evenodd" : undefined} strokeLinecap="round" /></svg>
          </button>
        ))}
      </div>
      <h3 className="u-mt-3">{tx("Penempatan")}</h3>
      <div className="d-seg u-w-full" role="group" aria-label={tx("Penempatan bentuk")}>
        {(pluginDef("shape").layouts ?? []).map((l) => (
          <button key={l.value} type="button" className="u-grow" aria-pressed={(c.layout ?? "above") === l.value} onClick={() => onLayout(l.value)}>{l.label}</button>
        ))}
      </div>
      <div className="d-field u-mt-2">
        <span>{tx("Warna")}</span>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Warna bentuk")}>
          {([["primary", tx("Utama")], ["accent", tx("Aksen")]] as const).map(([v, l]) => (
            <button key={v} type="button" className="u-grow" aria-pressed={color === v} onClick={() => set("color", v)}>{l}</button>
          ))}
          <input type="color" aria-label={tx("Warna bentuk bebas")} value={color.startsWith("#") ? color : "#f59e0b"} onChange={(e) => set("color", e.target.value.toUpperCase())} />
        </div>
      </div>
      <label className="d-field">
        <span className="st-row-between"><span>{tx("Putar")}</span><strong>{typeof c.data.rotate === "number" ? c.data.rotate : 0}°</strong></span>
        <input type="range" min={-180} max={180} step={5} aria-label={tx("Putar bentuk")} value={typeof c.data.rotate === "number" ? c.data.rotate : 0} onChange={(e) => set("rotate", Number(e.target.value))} />
      </label>
      <label className="d-field u-m-0">
        <span className="st-row-between"><span>{tx("Kepekatan")}</span><strong>{typeof c.data.opacity === "number" ? c.data.opacity : 100}%</strong></span>
        <input type="range" min={10} max={100} step={5} aria-label={tx("Kepekatan bentuk")} value={typeof c.data.opacity === "number" ? c.data.opacity : 100} onChange={(e) => set("opacity", Number(e.target.value))} />
      </label>
    </div>
  );
}

export function LogoFields({ c, onChange, businessHref }: { c: StudioCell; onChange: (patch: Partial<StudioCell>, key?: string) => void; businessHref: string }) {
  return (
    <div className="st-section">
      <h3>{tx("Logo")}</h3>
      <p className="d-hint u-mt-0">{tx("Logo dan nama diambil dari Profil bisnis. Ganti di sana agar semua halaman ikut berubah.")} <a href={businessHref}>{tx("Profil bisnis")}</a></p>
      <label className="d-switch">
        <input type="checkbox" checked={c.data.show_name !== false} onChange={(e) => onChange({ data: { ...c.data, show_name: e.target.checked } }, "logo-name")} />
        <span><strong>{tx("Tampilkan nama toko")}</strong><span className="d-muted">{tx("Saat logo ini ada, logo & nama di menu navigasi disembunyikan agar tidak dobel.")}</span></span>
      </label>
      <label className="d-field u-m-0"><span>{tx("Tautan saat diklik")}</span>
        <input maxLength={500} placeholder="/" value={typeof c.data.href === "string" ? c.data.href : ""} onChange={(e) => onChange({ data: { ...c.data, href: e.target.value } }, "logo-href")} />
      </label>
    </div>
  );
}
