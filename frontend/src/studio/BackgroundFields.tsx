"use client";

// Pengaturan plugin Latar di Inspector: cara gambar mengisi kotak, titik fokus, kepekatan, dan sudut.

import { t as tx } from "@/i18n";
import type { StudioCell } from "@/studio/editor-model";

const fitOf = (c: StudioCell) => (c.data.fit === "stretch" ? "stretch" : "crop");
const posOf = (c: StudioCell) => (typeof c.data.position === "string" ? c.data.position : "center");
const opacityOf = (c: StudioCell) => (typeof c.data.opacity === "number" ? c.data.opacity : 100);

export function BackgroundFields({ c, onChange }: { c: StudioCell; onChange: (patch: Partial<StudioCell>, key?: string) => void }) {
  const fit = fitOf(c);
  const set = (k: string, v: string | number, key: string) => onChange({ data: { ...c.data, [k]: v } }, key);
  return (
    <div className="st-section">
      <h3>{tx("Gaya latar")}</h3>
      <div className="d-field u-m-0 u-mb-2">
        <span>{tx("Saat ukuran kotak atau layar berubah")}</span>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Saat ukuran kotak atau layar berubah")}>
          <button type="button" className="u-grow" aria-pressed={fit === "crop"} onClick={() => set("fit", "crop", "bg-fit")}>{tx("Potong pas")}</button>
          <button type="button" className="u-grow" aria-pressed={fit === "stretch"} onClick={() => set("fit", "stretch", "bg-fit")}>{tx("Ikut melar")}</button>
        </div>
        <span className="d-hint u-mt-1">
          {fit === "crop"
            ? tx("Gambar selalu memenuhi kotak tanpa berubah bentuk; bagian tepi dipotong mengikuti lebar layar.")
            : tx("Gambar utuh selalu terlihat dan ikut melar atau menyusut mengikuti ukuran kotak.")}
        </span>
      </div>
      {fit === "crop" && (
        <div className="d-field u-m-0 u-mb-2">
          <span>{tx("Titik fokus (bagian yang dipertahankan)")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Titik fokus latar")}>
            {([["top", tx("Atas")], ["left", tx("Kiri")], ["center", tx("Tengah")], ["right", tx("Kanan")], ["bottom", tx("Bawah")]] as const).map(([v, l]) => (
              <button key={v} type="button" className="u-grow" aria-pressed={posOf(c) === v} onClick={() => set("position", v, "bg-pos")}>{l}</button>
            ))}
          </div>
        </div>
      )}
      <label className="d-field u-m-0 u-mb-2">
        <span className="st-row-between"><span>{tx("Kepekatan latar")}</span><strong>{opacityOf(c)}%</strong></span>
        <input type="range" min={10} max={100} step={5} aria-label={tx("Kepekatan latar")} value={opacityOf(c)}
          onChange={(e) => set("opacity", Number(e.target.value), "bg-opacity")} />
      </label>
      <div className="d-field u-m-0">
        <span>{tx("Sudut")}</span>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Sudut latar")}>
          <button type="button" className="u-grow" aria-pressed={c.style === "plain" || c.style === "flat"} onClick={() => onChange({ style: "plain" }, "")}>{tx("Tegas")}</button>
          <button type="button" className="u-grow" aria-pressed={c.style !== "plain" && c.style !== "flat"} onClick={() => onChange({ style: "card" }, "")}>{tx("Membulat")}</button>
        </div>
      </div>
    </div>
  );
}
