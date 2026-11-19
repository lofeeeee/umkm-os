"use client";

// Pengaturan plugin Gambar di Inspector: mode tumpukan, foto (PNG transparan didukung), cara mengisi kotak,
// kepekatan, tautan, teks alternatif, dan pojok layar untuk popup.

import { ImageSquare } from "@phosphor-icons/react/ssr";

import { t as tx } from "@/i18n";
import type { StudioCell } from "@/studio/editor-model";
import { pluginDef } from "@/studio/plugins";

const str = (v: unknown, d: string) => (typeof v === "string" && v ? v : d);

export function ImageFields({ c, onChange, onLayout, onPhoto }: {
  c: StudioCell;
  onChange: (patch: Partial<StudioCell>, key?: string) => void;
  onLayout: (l: string) => void;
  onPhoto: (f: File) => void;
}) {
  const mode = c.layout ?? "above";
  const fit = str(c.data.fit, "contain");
  const set = (k: string, v: string | number, key = `img-${k}`) => onChange({ data: { ...c.data, [k]: v } }, key);
  const hint: Record<string, string> = {
    above: tx("Di depan semua blok. Tanpa tautan, gambar tembus klik sehingga tombol di bawahnya tetap bisa ditekan."),
    below: tx("Di belakang blok, seperti latar kecil. Tidak pernah menutupi isi."),
    sticky: tx("Menempel di atas layar selama area kotaknya terlihat saat digulir. Tarik kotaknya lebih tinggi untuk jarak gantung yang lebih panjang."),
    popup: tx("Menempel di pojok layar dan ikut saat halaman digulir. Pembeli bisa menutupnya."),
  };
  return (
    <div className="st-section">
      <h3>{tx("Penempatan")}</h3>
      <div className="d-seg u-w-full" role="group" aria-label={tx("Penempatan gambar")}>
        {(pluginDef("image").layouts ?? []).map((l) => (
          <button key={l.value} type="button" className="u-grow" aria-pressed={mode === l.value} onClick={() => onLayout(l.value)}>{l.label}</button>
        ))}
      </div>
      <p className="d-hint u-mt-1">{hint[mode]}</p>
      {mode === "popup" && (
        <div className="d-field u-mt-2">
          <span>{tx("Pojok layar")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Pojok layar")}>
            {([["top-left", "↖"], ["top-right", "↗"], ["bottom-left", "↙"], ["bottom-right", "↘"]] as const).map(([v, l]) => (
              <button key={v} type="button" className="u-grow" aria-pressed={str(c.data.corner, "bottom-right") === v} onClick={() => set("corner", v)}>{l}</button>
            ))}
          </div>
          <span className="u-mt-2">{tx("Ukuran popup")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Ukuran popup")}>
            {([[2, tx("Kecil")], [3, tx("Sedang")], [5, tx("Besar")]] as const).map(([w, l]) => (
              <button key={w} type="button" className="u-grow" aria-pressed={c.w === w} onClick={() => onChange({ w }, "")}>{l}</button>
            ))}
          </div>
        </div>
      )}
      {mode === "popup" && <label className="d-btn d-btn-sm u-mt-2"><ImageSquare weight="bold" aria-hidden="true" />{c.data.image ? tx("Ganti gambar") : tx("Pasang gambar")}
        <input type="file" accept="image/png,image/webp,image/jpeg" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) onPhoto(f); }} />
      </label>}
      <p className="d-hint u-mt-1">{tx("PNG atau WebP transparan tetap transparan, cocok untuk stiker, logo, dan potongan objek.")}</p>
      {mode !== "popup" && (
        <div className="d-field u-mt-2">
          <span>{tx("Isi kotak")}</span>
          <div className="d-seg u-w-full" role="group" aria-label={tx("Isi kotak")}>
            <button type="button" className="u-grow" aria-pressed={fit === "contain"} onClick={() => set("fit", "contain")}>{tx("Utuh")}</button>
            <button type="button" className="u-grow" aria-pressed={fit === "crop"} onClick={() => set("fit", "crop")}>{tx("Potong pas")}</button>
            <button type="button" className="u-grow" aria-pressed={fit === "stretch"} onClick={() => set("fit", "stretch")}>{tx("Ikut melar")}</button>
          </div>
        </div>
      )}
      <div className="d-field u-mt-2">
        <span>{tx("Bentuk")}</span>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Bentuk gambar")}>
          {([["none", tx("Asli")], ["circle", tx("Lingkaran")], ["arch", tx("Lengkung atas")], ["blob", tx("Organik")], ["squircle", tx("Kotak bulat")]] as const).map(([v, l]) => (
            <button key={v} type="button" className="u-grow" aria-pressed={str(c.data.shape, "none") === v} onClick={() => set("shape", v)}>{l}</button>
          ))}
        </div>
      </div>
      <div className="d-field u-mt-2">
        <span>{tx("Filter warna")}</span>
        <div className="d-seg u-w-full st-seg-wrap" role="group" aria-label={tx("Filter warna")}>
          {([["none", tx("Asli")], ["mono", tx("Hitam-putih")], ["warm", tx("Hangat")], ["cool", tx("Sejuk")], ["vivid", tx("Cerah")], ["duotone", tx("Duotone merek")]] as const).map(([v, l]) => (
            <button key={v} type="button" className="u-grow" aria-pressed={str(c.data.filter, "none") === v} onClick={() => set("filter", v)}>{l}</button>
          ))}
        </div>
      </div>
      <label className="d-field u-mt-2">
        <span className="st-row-between"><span>{tx("Kepekatan")}</span><strong>{typeof c.data.opacity === "number" ? c.data.opacity : 100}%</strong></span>
        <input type="range" min={10} max={100} step={5} aria-label={tx("Kepekatan gambar")} value={typeof c.data.opacity === "number" ? c.data.opacity : 100}
          onChange={(e) => set("opacity", Number(e.target.value))} />
      </label>
      <label className="d-field"><span>{tx("Tautan saat diklik (opsional)")}</span>
        <input maxLength={500} placeholder={tx("mis. /produk atau https://…")} value={str(c.data.href, "")}
          onChange={(e) => onChange({ data: { ...c.data, href: e.target.value } }, "img-href")} />
      </label>
      <label className="d-field"><span>{tx("Teks alternatif")}</span>
        <input maxLength={160} placeholder={tx("Kosongkan bila gambar hanya hiasan")} value={str(c.data.alt, "")}
          onChange={(e) => onChange({ data: { ...c.data, alt: e.target.value } }, "img-alt")} />
      </label>
      <div className="d-field u-m-0">
        <span>{tx("Sudut")}</span>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Sudut gambar")}>
          <button type="button" className="u-grow" aria-pressed={c.style === "plain" || c.style === "flat"} onClick={() => onChange({ style: "plain" }, "")}>{tx("Tegas")}</button>
          <button type="button" className="u-grow" aria-pressed={c.style !== "plain" && c.style !== "flat"} onClick={() => onChange({ style: "card" }, "")}>{tx("Membulat")}</button>
        </div>
      </div>
    </div>
  );
}
