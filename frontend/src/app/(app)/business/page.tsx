"use client";

import { useEffect, useMemo, useState, type ReactNode } from "react";

import { Assets, FieldInput, type Upload } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, apiUrl, describe } from "@/lib/api";
import { SOCIAL_PLATFORMS } from "@/renderer/sections";
import type { Address, Business, BusinessProfile, HourException, Hours, Json } from "@/renderer/types";

const DAYS = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];
const ZONES = ["Asia/Jakarta", "Asia/Makassar", "Asia/Jayapura"];

export default function BusinessPage() {
  return <WithTenant>{(ctx) => <Form ctx={ctx} />}</WithTenant>;
}

function Section({ title, children, onSave, disabled, msg }: {
  title: string; children: ReactNode; onSave: () => void; disabled: boolean; msg: { ok?: string; err?: string };
}) {
  return (
    <div className="d-card">
      <h2>{title}</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {children}
      <button className="d-btn d-btn-primary" onClick={onSave} disabled={disabled}>Simpan {title.toLowerCase()}</button>
    </div>
  );
}

function Form({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/business`;
  const canEdit = ctx.can("tenant.update");
  const [data, setData] = useState<Business | null>(null);
  const [uploads, setUploads] = useState<Upload[]>([]);
  const [status, setStatus] = useState<{ status: string; local_time: string } | null>(null);
  const [msgs, setMsgs] = useState<Record<string, { ok?: string; err?: string }>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      api<Business>("GET", `${base}/profile`),
      api<Upload[]>("GET", `/api/v1/tenants/${ctx.tenant.id}/uploads`).catch(() => []),
      api<{ status: string; local_time: string }>("GET", `${base}/status`).catch(() => null),
    ])
      .then(([b, u, s]) => { setData(b); setUploads(u); setStatus(s); })
      .catch((e) => setError(describe(e)));
  }, [base, ctx.tenant.id]);

  const assets = useMemo(() => {
    const map = Object.fromEntries(uploads.map((u) => [u.ref, u.url]));
    return {
      uploads, canEdit,
      url: (r: string) => (map[r] ? apiUrl(map[r]!) : undefined),
      upload: async (file: File) => {
        try {
          const u = await api<Upload>("POST", `/api/v1/tenants/${ctx.tenant.id}/uploads`, { bytes: file });
          setUploads((x) => [u, ...x]);
          return u;
        } catch { return null; }
      },
    };
  }, [uploads, canEdit, ctx.tenant.id]);

  if (error) return <Alert kind="error">{error}</Alert>;
  if (!data) return <p className="d-muted" aria-busy="true">Memuat…</p>;

  async function save(part: string, path: string, body: unknown) {
    setMsgs((m) => ({ ...m, [part]: {} }));
    try {
      const b = await api<Business>("PUT", `${base}/${path}`, { json: body });
      setData(b);
      setMsgs((m) => ({ ...m, [part]: { ok: "Tersimpan." } }));
      setStatus(await api("GET", `${base}/status`));
    } catch (e) {
      setMsgs((m) => ({ ...m, [part]: { err: describe(e) } }));
    }
  }

  const p = data.profile;
  const setP = (patch: Partial<BusinessProfile>) => setData({ ...data, profile: { ...p, ...patch } });
  const field = (label: string, key: keyof BusinessProfile, type = "text") => (
    <label className="d-field">
      <span>{label}</span>
      <input type={type} value={(p[key] as string | null | undefined) ?? ""} disabled={!canEdit}
        onChange={(e) => setP({ [key]: e.target.value } as Partial<BusinessProfile>)} />
    </label>
  );
  const imageProps = (key: "logo" | "cover") => ({
    f: { key, label: key === "logo" ? "Logo usaha" : "Gambar sampul", kind: "image" as const },
    data: (p[key] ? { [key]: p[key] } : {}) as { [k: string]: Json },
    onChange: (d: { [k: string]: Json }) => setP({ [key]: (d[key] as string | undefined) ?? null }),
    disabled: !canEdit,
  });

  const hoursByDay = (d: number) => data.hours.find((h) => h.day_of_week === d);
  const setHours = (d: number, h: Hours | null) =>
    setData({ ...data, hours: [...data.hours.filter((x) => x.day_of_week !== d), ...(h ? [h] : [])].sort((a, b) => a.day_of_week - b.day_of_week) });

  return (
    <Assets.Provider value={assets}>
      <h1>Profil bisnis</h1>
      {status && (
        <p className="d-muted">
          Status sekarang: <strong>{status.status === "open" ? "Buka" : "Tutup"}</strong> (waktu lokal {status.local_time.replace("T", " ")})
        </p>
      )}
      {!canEdit && <Alert kind="ok">Anda hanya dapat melihat. Mengubah profil butuh izin tenant.update.</Alert>}

      <Section title="Profil" disabled={!canEdit} msg={msgs.profile ?? {}}
        onSave={() => save("profile", "profile", { ...p, socials: p.socials })}>
        <div className="d-grid2">
          {field("Nama bisnis *", "name")}
          <label className="d-field"><span>Zona waktu</span>
            <select value={p.timezone} disabled={!canEdit} onChange={(e) => setP({ timezone: e.target.value })}>
              {[...new Set([p.timezone, ...ZONES])].map((z) => <option key={z}>{z}</option>)}
            </select>
          </label>
          {field("Telepon", "phone", "tel")}
          {field("WhatsApp", "whatsapp", "tel")}
          {field("Email", "email", "email")}
        </div>
        <label className="d-field"><span>Deskripsi</span>
          <textarea value={p.description ?? ""} maxLength={2000} disabled={!canEdit} onChange={(e) => setP({ description: e.target.value })} />
        </label>
        <div className="d-grid2">
          <FieldInput {...imageProps("logo")} />
          <FieldInput {...imageProps("cover")} />
        </div>
        <fieldset className="e-list-item">
          <legend>Media sosial</legend>
          {p.socials.map((s, i) => (
            <div className="d-row" key={i}>
              <select value={s.platform} disabled={!canEdit} aria-label="Platform"
                onChange={(e) => setP({ socials: p.socials.map((x, j) => (j === i ? { ...x, platform: e.target.value } : x)) })}>
                {SOCIAL_PLATFORMS.map((o) => <option key={o}>{o}</option>)}
              </select>
              <input style={{ flex: 1 }} value={s.url} placeholder="https://" disabled={!canEdit} aria-label="URL"
                onChange={(e) => setP({ socials: p.socials.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)) })} />
              <button className="d-btn d-btn-sm d-btn-danger" disabled={!canEdit} onClick={() => setP({ socials: p.socials.filter((_, j) => j !== i) })}>Hapus</button>
            </div>
          ))}
          <button className="d-btn d-btn-sm" disabled={!canEdit || p.socials.length >= 10}
            onClick={() => setP({ socials: [...p.socials, { platform: "instagram", url: "https://" }] })}>Tambah</button>
        </fieldset>
        <label className="d-check"><input type="checkbox" checked={p.pickup_available} disabled={!canEdit} onChange={(e) => setP({ pickup_available: e.target.checked })} /> Bisa ambil di tempat</label>
        <label className="d-check"><input type="checkbox" checked={p.delivery_available} disabled={!canEdit} onChange={(e) => setP({ delivery_available: e.target.checked })} /> Bisa diantar</label>
        {field("Catatan pengambilan/pengantaran", "fulfillment_note")}
      </Section>

      <Section title="Alamat" disabled={!canEdit} msg={msgs.addresses ?? {}}
        onSave={() => save("addresses", "addresses", data.addresses.map((a) => ({
          label: a.label || null, address: a.address,
          latitude: a.latitude ?? null, longitude: a.longitude ?? null,
        })))}>
        {data.addresses.map((a, i) => {
          const set = (patch: Partial<Address>) => setData({ ...data, addresses: data.addresses.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
          const num = (v: string) => (v.trim() === "" ? null : Number(v));
          return (
            <div className="e-list-item" key={i}>
              <div className="d-grid2">
                <label className="d-field"><span>Label</span><input value={a.label ?? ""} disabled={!canEdit} onChange={(e) => set({ label: e.target.value })} /></label>
                <label className="d-field"><span>Alamat *</span><input value={a.address} disabled={!canEdit} onChange={(e) => set({ address: e.target.value })} /></label>
                <label className="d-field"><span>Latitude</span><input inputMode="decimal" value={a.latitude ?? ""} disabled={!canEdit} onChange={(e) => set({ latitude: num(e.target.value) })} /></label>
                <label className="d-field"><span>Longitude</span><input inputMode="decimal" value={a.longitude ?? ""} disabled={!canEdit} onChange={(e) => set({ longitude: num(e.target.value) })} /></label>
              </div>
              <button className="d-btn d-btn-sm d-btn-danger" disabled={!canEdit} onClick={() => setData({ ...data, addresses: data.addresses.filter((_, j) => j !== i) })}>Hapus alamat</button>
            </div>
          );
        })}
        <p><button className="d-btn d-btn-sm" disabled={!canEdit || data.addresses.length >= 10}
          onClick={() => setData({ ...data, addresses: [...data.addresses, { address: "" }] })}>Tambah alamat</button></p>
      </Section>

      <Section title="Jam buka" disabled={!canEdit} msg={msgs.hours ?? {}} onSave={() => save("hours", "hours", data.hours)}>
        <p className="d-muted">Jam tutup lebih kecil dari jam buka berarti tutup lewat tengah malam.</p>
        <table className="d-table">
          <tbody>
            {DAYS.map((name, i) => {
              const d = i + 1;
              const h = hoursByDay(d);
              return (
                <tr key={d}>
                  <th scope="row">{name}</th>
                  <td><label className="d-check" style={{ margin: 0 }}>
                    <input type="checkbox" checked={!!h} disabled={!canEdit} onChange={(e) => setHours(d, e.target.checked ? { day_of_week: d, open: "08:00", close: "17:00" } : null)} /> Buka
                  </label></td>
                  <td>{h && <input type="time" aria-label={`Buka ${name}`} value={h.open} disabled={!canEdit} onChange={(e) => setHours(d, { ...h, open: e.target.value })} />}</td>
                  <td>{h && <input type="time" aria-label={`Tutup ${name}`} value={h.close} disabled={!canEdit} onChange={(e) => setHours(d, { ...h, close: e.target.value })} />}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Section>

      <Section title="Hari libur & jam khusus" disabled={!canEdit} msg={msgs.exceptions ?? {}}
        onSave={() => save("exceptions", "hour-exceptions", data.exceptions.map((x) => ({
          date: x.date, is_closed: x.is_closed, note: x.note || null,
          open: x.is_closed ? null : x.open ?? null, close: x.is_closed ? null : x.close ?? null,
        })))}>
        {data.exceptions.map((x, i) => {
          const set = (patch: Partial<HourException>) => setData({ ...data, exceptions: data.exceptions.map((y, j) => (j === i ? { ...y, ...patch } : y)) });
          return (
            <div className="d-row e-list-item" key={i}>
              <input type="date" aria-label="Tanggal" value={x.date} disabled={!canEdit} onChange={(e) => set({ date: e.target.value })} />
              <label className="d-check" style={{ margin: 0 }}>
                <input type="checkbox" checked={x.is_closed} disabled={!canEdit} onChange={(e) => set({ is_closed: e.target.checked, open: e.target.checked ? null : "08:00", close: e.target.checked ? null : "12:00" })} /> Tutup
              </label>
              {!x.is_closed && <>
                <input type="time" aria-label="Buka" value={x.open ?? ""} disabled={!canEdit} onChange={(e) => set({ open: e.target.value })} />
                <input type="time" aria-label="Tutup" value={x.close ?? ""} disabled={!canEdit} onChange={(e) => set({ close: e.target.value })} />
              </>}
              <input aria-label="Catatan" placeholder="Catatan" value={x.note ?? ""} maxLength={120} disabled={!canEdit} onChange={(e) => set({ note: e.target.value })} />
              <button className="d-btn d-btn-sm d-btn-danger" disabled={!canEdit} onClick={() => setData({ ...data, exceptions: data.exceptions.filter((_, j) => j !== i) })}>Hapus</button>
            </div>
          );
        })}
        <p><button className="d-btn d-btn-sm" disabled={!canEdit}
          onClick={() => setData({ ...data, exceptions: [...data.exceptions, { date: new Date().toISOString().slice(0, 10), is_closed: true }] })}>Tambah tanggal</button></p>
      </Section>
    </Assets.Provider>
  );
}
