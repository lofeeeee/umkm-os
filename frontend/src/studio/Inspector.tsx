"use client";

// Panel pengaturan blok terpilih (Inspector) di Studio.

import Link from "@/ui/link";

import {
  LockSimple, ArrowLeft, ArrowsIn, ArrowsOut, Copy, ArrowDown, ArrowRight, ArrowUp, ImageSquare, Trash, Crop,
} from "@phosphor-icons/react/ssr";

import { FieldInput } from "@/dashboard/editor/fields";
import { FIELDS } from "@/renderer/sections";
import type { SectionType } from "@/renderer/types";
import { CELL_STYLES, pluginDef } from "@/studio/plugins";
import { Alert } from "@/dashboard/shell";
import { t as tx } from "@/i18n";
import { Tr } from "@/i18n/react";
import { type StudioCell, isEmbed, SINGLE, IMAGE_KEY, takesPhoto } from "@/studio/editor-model";
import { BackgroundFields } from "@/studio/BackgroundFields";

export function Inspector({ c, onBack, onChange, onWidth, onMove, onLayout, onDuplicate, onRemove, onFull, onPhoto, onCrop, onCoverPage, aiHref, businessHref, chatReady, upgradeHref }: {
  c: StudioCell;
  onBack: () => void;
  onChange: (patch: Partial<StudioCell>, key?: string) => void;
  onWidth: (w: number) => void;
  onMove: (d: "up" | "down" | "left" | "right") => void;
  onLayout: (l: string) => void;
  onDuplicate: () => void;
  onRemove: () => void;
  onFull: () => void;
  onPhoto: (f: File) => void;
  onCrop?: () => void;
  /** Plugin Latar: bentangkan ke seluruh halaman. */
  onCoverPage?: () => void;
  aiHref: string;
  businessHref: string;
  /** Chatbot aktif di Asisten AI? null = belum diketahui. */
  chatReady: boolean | null;
  /** Diisi bila plugin butuh paket lebih tinggi (ADR-0019). */
  upgradeHref: string | null;
}) {
  const def = pluginDef(c.kind);
  const layer = !!c.layer;
  return (
    <>
      <div className="d-row d-row-between">
        <button type="button" className="d-back u-m-0" onClick={onBack}><ArrowLeft weight="bold" aria-hidden="true" />{tx("Blok")}</button>
        <h2 className="u-m-0 u-fs-md u-grow">{def.name}</h2>
        <div className="d-row">
          <button type="button" className="d-icon-btn" aria-label={tx("Duplikat")} title={tx("Duplikat")} disabled={SINGLE.includes(c.kind)} onClick={onDuplicate}><Copy weight="bold" /></button>
          <button type="button" className="d-icon-btn" aria-label={tx("Hapus dari halaman")} title={tx("Hapus")} onClick={onRemove}><Trash weight="bold" /></button>
        </div>
      </div>
      {upgradeHref && (
        <div className="d-alert d-alert-info" role="note"><div>
          <Tr s="Blok ini bagian paket Pro: tetap tersimpan, tetapi tidak tampil di situs sampai toko memakai Pro.{0}" a={[" "]} />
          <Link href={upgradeHref}>{tx("Lihat paket")}</Link>
        </div></div>
      )}
      {layer && <p className="d-hint u-m-0">{tx("Latar selalu berada di belakang blok lain dan tidak pernah menutupinya. Saat latar dipilih, blok lain dibuat tembus pandang supaya latar mudah diseret dan diubah ukurannya — tekan Esc bila selesai.")}</p>}
      {c.kind !== "chatbot" && !layer && <p className="d-hint u-m-0">{tx("Klik teks di kanvas untuk mengetik langsung.")}{takesPhoto(c.kind) && tx(" Seret foto dari komputer ke blok untuk memasangnya.")}</p>}
      {!isEmbed(c) && <div className="st-section">
        <label className="d-switch">
          <input type="checkbox" checked={!!c.locked} onChange={(e) => onChange({ locked: e.target.checked }, "")} />
          <span><strong><LockSimple weight="bold" aria-hidden="true" className="u-va-icon" />{" "}{tx("Kunci posisi")}</strong><span className="d-muted">{tx("Blok ini tidak bisa diseret, dan blok lain tidak akan menggesernya.")}</span></span>
        </label>
        <h3>{tx("Posisi")}</h3>
        {/* WCAG 2.5.7 (P9-6): semua yang bisa diseret juga bisa lewat tombol. */}
        <div className="d-seg u-w-full u-mb-3" role="group" aria-label={tx("Pindahkan")}>
          {([["up", tx("Pindah ke atas"), ArrowUp], ["down", tx("Pindah ke bawah"), ArrowDown], ["left", tx("Geser ke kiri"), ArrowLeft], ["right", tx("Geser ke kanan"), ArrowRight]] as const).map(([d, l, Icon]) => (
            <button key={d} type="button" style={{ flex: 1, minHeight: 32 }} aria-label={l} title={l} disabled={!!c.locked} onClick={() => onMove(d)}><Icon weight="bold" aria-hidden="true" /></button>
          ))}
        </div>
        <h3>{tx("Ukuran")}</h3>
        <div className="d-seg u-w-full" role="group" aria-label={tx("Lebar")}>
          {([[3, "¼"], [4, "⅓"], [6, "½"], [8, "⅔"], [12, tx("Penuh")]] as const).map(([w, l]) => (
            <button key={w} type="button" className="u-grow" aria-pressed={c.w === w} disabled={w < (c.minW ?? 2) || !!c.locked} onClick={() => onWidth(w)}>{l}</button>
          ))}
        </div>
        {!layer && <label className="d-switch u-mt-2">
          <input type="checkbox" checked={c.autoH !== false} onChange={(e) => onChange({ autoH: e.target.checked }, "")} />
          <span><strong>{tx("Tinggi otomatis")}</strong><span className="d-muted">{tx("Kotak mengikuti panjang isi sehingga tidak terpotong. Mati otomatis bila tinggi ditarik manual.")}</span></span>
        </label>}
        <div className="d-row">
          {layer
            ? onCoverPage && <button type="button" className="d-btn d-btn-sm" disabled={!!c.locked} onClick={onCoverPage}><ArrowsOut weight="bold" aria-hidden="true" />{tx("Tutupi seluruh halaman")}</button>
            : <button type="button" className="d-btn d-btn-sm" onClick={onFull}><ArrowsOut weight="bold" aria-hidden="true" />{tx("Lihat penuh")}</button>}
          {takesPhoto(c.kind) && (
            <label className="d-btn d-btn-sm"><ImageSquare weight="bold" aria-hidden="true" />{c.kind === "gallery" ? tx("Tambah foto") : tx("Ganti foto")}
              <input type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) onPhoto(f); }} />
            </label>
          )}
          {takesPhoto(c.kind) && onCrop && (
            <button type="button" className="d-btn d-btn-sm" onClick={onCrop} title={tx("Potong foto")}>
              <Crop weight="bold" aria-hidden="true" />{" "}{tx("Potong")}
            </button>
          )}
        </div>
        {takesPhoto(c.kind) && !layer && (
          <div className="u-mt-3">
            <div className="d-field u-m-0 u-mb-2">
              <span>{tx("Tampilan foto")}</span>
              <div className="d-seg u-w-full" role="group" aria-label={tx("Tampilan foto")}>
                <button type="button" className="u-grow" aria-pressed={((c.data.image_fit as string) ?? "cover") === "cover"} onClick={() => onChange({ data: { ...c.data, image_fit: "cover" } }, "image_fit")}>{tx("Penuhi kotak")}</button>
                <button type="button" className="u-grow" aria-pressed={(c.data.image_fit as string) === "contain"} onClick={() => onChange({ data: { ...c.data, image_fit: "contain" } }, "image_fit")}>{tx("Tampil utuh")}</button>
              </div>
            </div>
            <div className="d-field u-m-0">
              <span>{tx("Titik fokus (posisi)")}</span>
              <div className="d-seg u-w-full" role="group" aria-label={tx("Posisi foto")}>
                {([["top", tx("Atas")], ["center", tx("Tengah")], ["bottom", tx("Bawah")]] as const).map(([pos, lbl]) => (
                  <button key={pos} type="button" className="u-grow" aria-pressed={((c.data.image_position as string) ?? "center") === pos} onClick={() => onChange({ data: { ...c.data, image_position: pos } }, "image_pos")}>{lbl}</button>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>}
      {layer ? <BackgroundFields c={c} onChange={onChange} /> : <>
      <div className="st-section">
        <h3>{tx("Gaya")}</h3>
        <div className="st-styles" role="group" aria-label={tx("Gaya")}>
          {CELL_STYLES.map((s) => (
            <button key={s.value} type="button" className="st-style-tile" aria-pressed={c.style === s.value} onClick={() => onChange({ style: s.value }, "")}>
              <span className={`st-style-sample st-style-${s.value}`} aria-hidden="true"><i /><b /></span>
              {s.label}
            </button>
          ))}
        </div>
        {def.layouts && (
          <>
            <h3 className="u-mt-3">{tx("Tata letak")}</h3>
            <div className="d-seg u-w-full" role="group" aria-label={tx("Tata letak")}>
              {def.layouts.map((l) => <button key={l.value} type="button" className="u-grow" aria-pressed={c.layout === l.value} onClick={() => onLayout(l.value)}>{l.label}</button>)}
            </div>
          </>
        )}
        {c.kind === "chatbot" && (
          <>
            <h3 className="u-mt-3">{tx("Keadaan awal")}</h3>
            <div className="d-seg u-w-full" role="group" aria-label={tx("Keadaan chatbot")}>
              <button type="button" className="u-grow" aria-pressed={c.chatOpen !== false} onClick={() => onChange({ chatOpen: true }, "")}><ArrowsOut weight="bold" aria-hidden="true" />{" "}{tx("Maksimal")}</button>
              <button type="button" className="u-grow" aria-pressed={c.chatOpen === false} onClick={() => onChange({ chatOpen: false }, "")}><ArrowsIn weight="bold" aria-hidden="true" />{" "}{tx("Minimal")}</button>
            </div>
            {c.layout === "floating" && <p className="d-hint u-mt-2">{tx("Gelembung bulat di pojok kanan bawah, ikut saat halaman digulir. Klik gelembungnya untuk membuka/menutup.")}</p>}
            {c.layout === "bar" && <p className="d-hint u-mt-2">{tx("Bilah horizontal selebar layar di bagian bawah, ikut saat halaman digulir. Pembeli bisa langsung mengetik pertanyaan.")}</p>}
          </>
        )}
      </div>
      <div className="st-section">
        <h3>{tx("Isi")}</h3>
        {c.kind === "chatbot" ? (
          <>
            {chatReady === false && (
              <Alert kind="warn">
                {tx("Chatbot belum aktif, jadi blok ini")}{" "}<strong>{tx("tidak tampil")}</strong>{" "}<Tr s="di website yang diterbitkan. Buka{0}" a={[" "]} />
                <Link href={aiHref}>{tx("Asisten AI")}</Link>{tx(", pilih mode")}{" "}<strong>{tx("Hanya staf")}</strong>{" "}{tx("(tanpa penyedia AI) atau tambahkan penyedia, lalu simpan.")}
              </Alert>
            )}
            <label className="d-field"><span>{tx("Judul")}</span><input maxLength={40} value={String(c.data.title ?? "")} onChange={(e) => onChange({ data: { ...c.data, title: e.target.value } })} /></label>
            <label className="d-field"><span>{tx("Pesan sambutan")}</span><textarea rows={3} maxLength={300} value={String(c.data.welcome ?? "")} onChange={(e) => onChange({ data: { ...c.data, welcome: e.target.value } })} /></label>
            <p className="d-hint">{tx("Jawaban memakai pengaturan di menu")}{" "}<Link href={aiHref}>{tx("Asisten AI")}</Link>.</p>
          </>
        ) : c.kind === "text" ? (
          <>
            <label className="d-field"><span>{tx("Judul (opsional)")}</span><input maxLength={120} value={String(c.data.title ?? "")} onChange={(e) => onChange({ data: { ...c.data, title: e.target.value } })} /></label>
            <label className="d-field"><span>{tx("Isi")}</span><textarea rows={6} maxLength={5000} value={String(c.data.body ?? "")} onChange={(e) => onChange({ data: { ...c.data, body: e.target.value } })} /><span className="d-hint">{tx("Baris kosong memisahkan paragraf.")}</span></label>
          </>
        ) : c.kind === "whatsapp" ? (
          <>
            <label className="d-field"><span>{tx("Tulisan tombol")}</span><input maxLength={40} value={String(c.data.label ?? "")} onChange={(e) => onChange({ data: { ...c.data, label: e.target.value } })} /></label>
            <label className="d-field"><span>{tx("Pesan awal (opsional)")}</span><textarea rows={3} maxLength={300} value={String(c.data.message ?? "")} onChange={(e) => onChange({ data: { ...c.data, message: e.target.value } })} /><span className="d-hint">{tx("Terisi otomatis saat pembeli membuka WhatsApp.")}</span></label>
            <p className="d-hint">{tx("Nomor diambil dari WhatsApp (atau telepon) di")}{" "}<Link href={businessHref}>{tx("Profil bisnis")}</Link>.</p>
          </>
        ) : (
          <>
            {FIELDS[c.kind as SectionType].map((f) => (
              <FieldInput key={f.key} f={f} data={c.data} disabled={false} onChange={(d) => onChange({ data: d })} />
            ))}
            {/* WCAG 1.1.1 (P9-6): field v2 — hanya di Studio; editor v1 tidak mengenalnya. */}
            {c.kind in IMAGE_KEY && (
              <label className="d-field"><span>{tx("Teks alternatif foto")}</span>
                <input maxLength={160} placeholder={tx("mis. Segelas kopi susu gula aren di meja kayu")} value={String(c.data.image_alt ?? "")}
                  onChange={(e) => onChange({ data: { ...c.data, image_alt: e.target.value } })} />
                <span className="d-hint">{tx("Dibacakan untuk pembeli tunanetra dan membantu Google memahami foto. Kosongkan bila foto hanya hiasan.")}</span>
              </label>
            )}
          </>
        )}
      </div>
      </>}
    </>
  );
}
