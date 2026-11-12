"use client";

import { createContext, useContext, useId, useState } from "react";

import type { Field } from "@/renderer/sections";
import type { Json } from "@/renderer/types";

export interface Upload { id: string; ref: string; url: string; content_type: string; byte_size: number }

export interface AssetsCtx {
  uploads: Upload[];
  /** ref -> URL absolut (gabungan upload + aset dari draft) */
  url: (ref: string) => string | undefined;
  upload: (file: File) => Promise<Upload | null>;
  canEdit: boolean;
}

export const Assets = createContext<AssetsCtx>({
  uploads: [],
  url: () => undefined,
  upload: async () => null,
  canEdit: false,
});

type Obj = { [k: string]: Json };

/** Set nilai; string kosong pada field opsional menghapus key (validator menolak "" untuk field wajib saja). */
export function setKey(o: Obj, f: Field, v: Json | undefined): Obj {
  const next = { ...o };
  if (v === undefined || (v === "" && !f.required)) delete next[f.key];
  else next[f.key] = v;
  return next;
}

export function ImagePicker({ value, onChange, label }: { value: string | undefined; onChange: (v: string | undefined) => void; label: string }) {
  const a = useContext(Assets);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const src = value ? a.url(value) : undefined;
  return (
    <div className="d-field">
      <span>{label}</span>
      <div className="d-row">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        {src ? <img src={src} alt="" width={56} height={56} style={{ objectFit: "cover", borderRadius: 6 }} /> : <span className="d-muted">Belum ada gambar</span>}
        <button type="button" className="d-btn d-btn-sm" onClick={() => setOpen(!open)} disabled={!a.canEdit} aria-expanded={open}>
          {open ? "Tutup" : "Pilih gambar"}
        </button>
        {value && <button type="button" className="d-btn d-btn-sm d-btn-danger" onClick={() => onChange(undefined)} disabled={!a.canEdit}>Hapus</button>}
      </div>
      {open && (
        <div className="e-list-item">
          <label className="d-btn d-btn-sm" style={{ display: "inline-block", marginBottom: 8 }}>
            {busy ? "Mengunggah…" : "Unggah PNG/JPEG/WebP (maks 2 MB)"}
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp"
              hidden
              disabled={busy}
              onChange={async (e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (!file) return;
                setBusy(true);
                setErr(null);
                const u = await a.upload(file).catch(() => null);
                setBusy(false);
                if (u) {
                  onChange(u.ref);
                  setOpen(false);
                } else setErr("Unggahan ditolak. Pastikan file gambar PNG/JPEG/WebP ≤ 2 MB.");
              }}
            />
          </label>
          {err && <div className="d-alert d-alert-error" role="alert">{err}</div>}
          <div className="e-thumbs">
            {a.uploads.map((u) => (
              <button type="button" key={u.id} aria-pressed={u.ref === value} aria-label={`Pilih gambar ${u.id.slice(0, 8)}`}
                onClick={() => { onChange(u.ref); setOpen(false); }}>
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img src={a.url(u.ref)} alt="" />
              </button>
            ))}
          </div>
          {a.uploads.length === 0 && <p className="d-muted">Belum ada gambar diunggah.</p>}
        </div>
      )}
    </div>
  );
}

export function FieldInput({ f, data, onChange, disabled }: { f: Field; data: Obj; onChange: (d: Obj) => void; disabled: boolean }) {
  const id = useId();
  const v = data[f.key];
  const label = `${f.label}${f.required ? " *" : ""}`;
  switch (f.kind) {
    case "text":
    case "href":
      return (
        <label className="d-field" htmlFor={id}>
          <span>{label}</span>
          <input id={id} type="text" value={typeof v === "string" ? v : ""} maxLength={f.kind === "href" ? 500 : f.max}
            placeholder={f.kind === "href" ? "https://…, /halaman, #section, mailto:, tel:" : undefined}
            disabled={disabled} onChange={(e) => onChange(setKey(data, f, e.target.value))} />
        </label>
      );
    case "textarea":
      return (
        <label className="d-field" htmlFor={id}>
          <span>{label} {f.max && <small className="d-muted">({(typeof v === "string" ? v.length : 0)}/{f.max})</small>}</span>
          <textarea id={id} value={typeof v === "string" ? v : ""} maxLength={f.max} disabled={disabled}
            onChange={(e) => onChange(setKey(data, f, e.target.value))} />
        </label>
      );
    case "number":
      return (
        <label className="d-field" htmlFor={id}>
          <span>{label}</span>
          <input id={id} type="number" min={f.min} max={f.max} value={typeof v === "number" ? v : ""} disabled={disabled}
            onChange={(e) => onChange(setKey(data, f, e.target.value === "" ? undefined : Number(e.target.value)))} />
        </label>
      );
    case "bool":
      return (
        <label className="d-check">
          <input type="checkbox" checked={v === true} disabled={disabled} onChange={(e) => onChange(setKey(data, f, e.target.checked))} />
          {f.label}
        </label>
      );
    case "select":
      return (
        <label className="d-field" htmlFor={id}>
          <span>{label}</span>
          <select id={id} value={typeof v === "string" ? v : ""} disabled={disabled} onChange={(e) => onChange(setKey(data, f, e.target.value))}>
            {f.options?.map((o) => <option key={o} value={o}>{o}</option>)}
          </select>
        </label>
      );
    case "image":
      return <ImagePicker label={label} value={typeof v === "string" ? v : undefined} onChange={(r) => onChange(setKey(data, f, r))} />;
    case "cta": {
      const c = v && typeof v === "object" && !Array.isArray(v) ? (v as Obj) : undefined;
      return (
        <fieldset className="e-list-item">
          <legend>{f.label}</legend>
          <label className="d-check">
            <input type="checkbox" checked={!!c} disabled={disabled}
              onChange={(e) => onChange(setKey(data, f, e.target.checked ? { label: "Lihat", href: "#" + "products-1" } : undefined))} />
            Tampilkan tombol
          </label>
          {c && (
            <>
              <FieldInput f={{ key: "label", label: "Teks tombol", kind: "text", max: 40, required: true }} data={c} disabled={disabled}
                onChange={(n) => onChange(setKey(data, f, n))} />
              <FieldInput f={{ key: "href", label: "Tautan", kind: "href", required: true }} data={c} disabled={disabled}
                onChange={(n) => onChange(setKey(data, f, n))} />
            </>
          )}
        </fieldset>
      );
    }
    case "list": {
      const items = Array.isArray(v) ? (v as Obj[]) : [];
      const set = (next: Obj[]) => onChange({ ...data, [f.key]: next });
      return (
        <fieldset className="e-list-item">
          <legend>{f.label} ({items.length}{f.maxItems ? `/${f.maxItems}` : ""})</legend>
          {items.map((it, i) => (
            <div key={i} className="e-list-item">
              {f.item?.map((sub) => (
                <FieldInput key={sub.key} f={sub} data={it} disabled={disabled}
                  onChange={(n) => set(items.map((x, j) => (j === i ? n : x)))} />
              ))}
              <div className="d-row">
                <button type="button" className="d-btn d-btn-sm" disabled={disabled || i === 0}
                  onClick={() => { const n = [...items]; [n[i - 1], n[i]] = [n[i]!, n[i - 1]!]; set(n); }}>Naik</button>
                <button type="button" className="d-btn d-btn-sm d-btn-danger" disabled={disabled || items.length <= (f.minItems ?? 0)}
                  onClick={() => set(items.filter((_, j) => j !== i))}>Hapus</button>
              </div>
            </div>
          ))}
          <button type="button" className="d-btn d-btn-sm" disabled={disabled || items.length >= (f.maxItems ?? 99)}
            onClick={() => {
              const o: Obj = {};
              for (const s of f.item ?? []) {
                if (s.kind === "select") o[s.key] = s.options?.[0] ?? "";
                else if (s.required && s.kind !== "image") o[s.key] = s.kind === "href" ? "https://" : "Isi";
              }
              set([...items, o]);
            }}>Tambah</button>
        </fieldset>
      );
    }
  }
}
