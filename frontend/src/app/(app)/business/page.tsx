"use client";

import { useEffect, useMemo, useState, type ReactNode } from "react";

import { prepareImage } from "@/lib/image";
import { LAT_RANGE, LNG_RANGE, coordNumber, parseCoord } from "@/lib/coords";
import { CoordField } from "@/ui/coord-field";
import { ContactField } from "@/ui/contact-field";
import { Assets, FieldInput, type Upload } from "@/dashboard/editor/fields";
import { Plus, Trash } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";
import { api, apiUrl, describe, ERROR_TEXT } from "@/lib/api";
import { SOCIAL_PLATFORMS } from "@/renderer/sections";
import type { Business, BusinessProfile, EditableAddress, HourException, Hours, Json } from "@/renderer/types";
import { t, m } from "@/i18n";

const DAYS = [m("Senin"), m("Selasa"), m("Rabu"), m("Kamis"), m("Jumat"), m("Sabtu"), m("Minggu")];
const ZONES = ["Asia/Jakarta", "Asia/Makassar", "Asia/Jayapura"];

/**
 * Data bisnis saat diedit. Alamat memakai `EditableAddress` karena koordinat boleh berupa
 * teks sementara (mis. "-") selagi pengguna mengetik — backend hanya menerima angka.
 */
type BusinessEdit = Omit<Business, "addresses"> & { addresses: EditableAddress[] };

/**
 * Teks untuk isian koordinat. Nilai dari server berupa angka (mis. -6.2); selama diedit
 * boleh berupa teks sementara ("-", "-6.") yang disimpan pengguna.
 */
const coordText = (v: number | string | null | undefined): string | null => {
  if (v === null || v === undefined || v === "") return null;
  return typeof v === "number" ? String(v) : v;
};

export default function BusinessPage() {
  return <WithTenant>{(ctx) => <Form ctx={ctx} />}</WithTenant>;
}

function Section({ title, description, children, onSave, disabled, msg }: {
  title: string; description: string; children: ReactNode; onSave: () => Promise<void>; disabled: boolean; msg: { err?: string };
}) {
  const [busy, setBusy] = useState(false);
  return (
    <Card
      title={title}
      description={description}
      footer={!disabled && <BusyButton type="button" busy={busy} onClick={async () => { setBusy(true); try { await onSave(); } finally { setBusy(false); } }}>{t("Simpan")}{" "}{title.toLowerCase()}</BusyButton>}
    >
      <Alert kind="error">{msg.err}</Alert>
      {children}
    </Card>
  );
}

function Form({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/business`;
  const canEdit = ctx.can("tenant.update");
  const [data, setData] = useState<BusinessEdit | null>(null);
  const [uploads, setUploads] = useState<Upload[]>([]);
  const [status, setStatus] = useState<{ status: string; local_time: string } | null>(null);
  const [msgs, setMsgs] = useState<Record<string, { err?: string }>>({});
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
          const u = await api<Upload>("POST", `/api/v1/tenants/${ctx.tenant.id}/uploads`, { bytes: await prepareImage(file) });
          setUploads((x) => [u, ...x]);
          return u;
        } catch { return null; }
      },
    };
  }, [uploads, canEdit, ctx.tenant.id]);

  if (error) return <Alert kind="error">{error}</Alert>;
  if (!data) return <PageSkeleton />;

  async function save(part: string, path: string, body: unknown) {
    setMsgs((m) => ({ ...m, [part]: {} }));
    try {
      const b = await api<Business>("PUT", `${base}/${path}`, { json: body });
      setData(b);
      toast.ok(t("Tersimpan."));
      setStatus(await api("GET", `${base}/status`));
    } catch (e) {
      setMsgs((m) => ({ ...m, [part]: { err: describe(e, { name: t("Nama bisnis") }) } }));
    }
  }

  const p = data.profile;
  const setP = (patch: Partial<BusinessProfile>) => setData({ ...data, profile: { ...p, ...patch } });
  const field = (label: string, key: keyof BusinessProfile, type = "text") => (
    <label className="d-field">
      <span>{label}</span>
      <input type={type} aria-required={label.endsWith("*") ? true : undefined} value={(p[key] as string | null | undefined) ?? ""} disabled={!canEdit}
        onChange={(e) => setP({ [key]: e.target.value } as Partial<BusinessProfile>)} />
    </label>
  );
  /** Isian kontak: validasi langsung (hanya angka & awalan 0/62 untuk telepon). */
  const contact = (label: string, key: "phone" | "whatsapp" | "email", kind: "phone" | "email") => (
    <ContactField
      label={label}
      kind={kind}
      value={(p[key] as string | null | undefined) ?? ""}
      disabled={!canEdit}
      errorText={ERROR_TEXT}
      autoComplete={kind === "email" ? "email" : "tel"}
      maxLength={kind === "email" ? 254 : 20}
      onChange={(v) => setP({ [key]: v } as Partial<BusinessProfile>)}
    />
  );
  const imageProps = (key: "logo" | "cover") => ({
    f: { key, label: key === "logo" ? t("Logo usaha") : t("Gambar sampul"), kind: "image" as const },
    data: (p[key] ? { [key]: p[key] } : {}) as { [k: string]: Json },
    onChange: (d: { [k: string]: Json }) => setP({ [key]: (d[key] as string | undefined) ?? null }),
    disabled: !canEdit,
  });

  const hoursByDay = (d: number) => data.hours.find((h) => h.day_of_week === d);
  const setHours = (d: number, h: Hours | null) =>
    setData({ ...data, hours: [...data.hours.filter((x) => x.day_of_week !== d), ...(h ? [h] : [])].sort((a, b) => a.day_of_week - b.day_of_week) });

  return (
    <Assets.Provider value={assets}>
      <PageHeader
        title={t("Profil bisnis")}
        description={t("Informasi ini tampil di website toko dan dipakai Asisten AI untuk menjawab pelanggan.")}
        actions={status && (
          <span className="d-row" title={t("Waktu lokal {0}", status.local_time.replace("T", " "))}>
            <Badge tone={status.status === "open" ? "ok" : "neutral"}>{status.status === "open" ? t("Sedang buka") : t("Sedang tutup")}</Badge>
            <span className="d-muted d-small">pukul {status.local_time.slice(11, 16)}</span>
          </span>
        )}
      />
      {!canEdit && <Alert kind="info">{t("Anda hanya dapat melihat. Hanya pemilik toko yang dapat mengubah profil.")}</Alert>}

      <Section title={t("Profil")} description={t("Nama, kontak, logo, dan cara pesanan diterima.")} disabled={!canEdit} msg={msgs.profile ?? {}}
        onSave={() => save("profile", "profile", { ...p, socials: p.socials })}>
        <div className="d-grid2">
          {field(t("Nama bisnis *"), "name")}
          <label className="d-field"><span>{t("Zona waktu")}</span>
            <select value={p.timezone} disabled={!canEdit} onChange={(e) => setP({ timezone: e.target.value })}>
              {[...new Set([p.timezone, ...ZONES])].map((z) => <option key={z}>{z}</option>)}
            </select>
          </label>
          {contact(t("Telepon"), "phone", "phone")}
          {contact(t("WhatsApp"), "whatsapp", "phone")}
          {contact(t("Email"), "email", "email")}
        </div>
        <label className="d-field"><span>{t("Deskripsi")}</span>
          <textarea value={p.description ?? ""} maxLength={2000} disabled={!canEdit} onChange={(e) => setP({ description: e.target.value })} />
        </label>
        <div className="d-grid2">
          <FieldInput {...imageProps("logo")} />
          <FieldInput {...imageProps("cover")} />
        </div>
        <fieldset className="e-list-item">
          <legend>{t("Media sosial")}</legend>
          {p.socials.map((s, i) => (
            <div className="d-row" key={i}>
              <select value={s.platform} disabled={!canEdit} aria-label={t("Platform")}
                onChange={(e) => setP({ socials: p.socials.map((x, j) => (j === i ? { ...x, platform: e.target.value } : x)) })}>
                {SOCIAL_PLATFORMS.map((o) => <option key={o}>{o}</option>)}
              </select>
              <input className="u-grow" value={s.url} placeholder="https://" disabled={!canEdit} aria-label="URL"
                onChange={(e) => setP({ socials: p.socials.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)) })} />
              <button type="button" className="d-icon-btn" aria-label={t("Hapus media sosial")} disabled={!canEdit} onClick={() => setP({ socials: p.socials.filter((_, j) => j !== i) })}><Trash weight="bold" /></button>
            </div>
          ))}
          <button type="button" className="d-btn d-btn-sm u-mt-2" disabled={!canEdit || p.socials.length >= 10}
            onClick={() => setP({ socials: [...p.socials, { platform: "instagram", url: "https://" }] })}><Plus weight="bold" aria-hidden="true" />{t("Tambah media sosial")}</button>
        </fieldset>
        <h3>{t("Cara pesanan diterima")}</h3>
        <div className="d-grid">
          <label className="d-switch"><input type="checkbox" checked={p.pickup_available} disabled={!canEdit} onChange={(e) => setP({ pickup_available: e.target.checked })} /><span><strong>{t("Bisa ambil di tempat")}</strong><span className="d-muted">{t("Pembeli mengambil sendiri di toko.")}</span></span></label>
          <label className="d-switch"><input type="checkbox" checked={p.delivery_available} disabled={!canEdit} onChange={(e) => setP({ delivery_available: e.target.checked })} /><span><strong>{t("Bisa diantar")}</strong><span className="d-muted">{t("Ongkir diatur di Pembayaran & pengiriman.")}</span></span></label>
        </div>
        {field(t("Catatan pengambilan/pengantaran"), "fulfillment_note")}
      </Section>

      <Section title={t("Alamat")} description={t("Tampil di bagian Lokasi website. Koordinat opsional untuk peta.")} disabled={!canEdit} msg={msgs.addresses ?? {}}
        onSave={() => save("addresses", "addresses", data.addresses.map((a) => ({
          label: a.label || null, address: a.address,
          // Teks sementara (mis. "-6.") dikirim sebagai angka; teks tak lengkap dibuang.
          latitude: coordNumber(a.latitude),
          longitude: coordNumber(a.longitude),
        })))}>
        {data.addresses.map((a, i) => {
          const set = (patch: Partial<EditableAddress>) =>
            setData({ ...data, addresses: data.addresses.map((x, j) => (j === i ? { ...x, ...patch } : x)) as EditableAddress[] });
          return (
            <div className="e-list-item" key={i}>
              <div className="d-grid2">
                <label className="d-field"><span>{t("Label")}</span><input value={a.label ?? ""} disabled={!canEdit} onChange={(e) => set({ label: e.target.value })} /></label>
                <label className="d-field"><span>{t("Alamat *")}</span><input aria-required="true" value={a.address} disabled={!canEdit} onChange={(e) => set({ address: e.target.value })} /></label>
                <CoordField
                  label={t("Latitude")}
                  value={coordText(a.latitude)}
                  range={LAT_RANGE}
                  hint={t("Boleh negatif. Latitude -90..90, bujur -180..180.")}
                  disabled={!canEdit}
                  onChange={(v) => set({ latitude: parseCoord(v) })}
                />
                <CoordField
                  label={t("Longitude")}
                  value={coordText(a.longitude)}
                  range={LNG_RANGE}
                  hint={t("Boleh negatif. Latitude -90..90, bujur -180..180.")}
                  disabled={!canEdit}
                  onChange={(v) => set({ longitude: parseCoord(v) })}
                />
              </div>
              <button type="button" className="d-btn d-btn-sm d-btn-ghost u-danger" disabled={!canEdit} onClick={() => setData({ ...data, addresses: data.addresses.filter((_, j) => j !== i) })}><Trash weight="bold" aria-hidden="true" />{t("Hapus alamat")}</button>
            </div>
          );
        })}
        <p><button type="button" className="d-btn d-btn-sm" disabled={!canEdit || data.addresses.length >= 10}
          onClick={() => setData({ ...data, addresses: [...data.addresses, { address: "" }] })}><Plus weight="bold" aria-hidden="true" />{t("Tambah alamat")}</button></p>
      </Section>

      <Section title={t("Jam buka")} description={t("Jam tutup lebih kecil dari jam buka berarti tutup lewat tengah malam.")} disabled={!canEdit} msg={msgs.hours ?? {}} onSave={() => save("hours", "hours", data.hours)}>
        <table className="d-table d-hours">
          <tbody>
            {DAYS.map((name, i) => {
              const d = i + 1;
              const h = hoursByDay(d);
              return (
                <tr key={d}>
                  <th scope="row">{t(name)}</th>
                  <td><label className="d-switch u-m-0">
                    <input type="checkbox" checked={!!h} disabled={!canEdit} onChange={(e) => setHours(d, e.target.checked ? { day_of_week: d, open: "08:00", close: "17:00" } : null)} /> <span>{h ? t("Buka") : <span className="d-muted">{t("Tutup")}</span>}</span>
                  </label></td>
                  <td>{h ? (
                    <span className="d-row u-nowrap">
                      <input type="time" aria-label={t("Buka {0}", name)} value={h.open} disabled={!canEdit} onChange={(e) => setHours(d, { ...h, open: e.target.value })} />
                      <span className="d-muted">–</span>
                      <input type="time" aria-label={t("Tutup {0}", name)} value={h.close} disabled={!canEdit} onChange={(e) => setHours(d, { ...h, close: e.target.value })} />
                    </span>
                  ) : null}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Section>

      <Section title={t("Hari libur & jam khusus")} description={t("Tanggal tertentu yang berbeda dari jam buka biasa, mis. libur Lebaran.")} disabled={!canEdit} msg={msgs.exceptions ?? {}}
        onSave={() => save("exceptions", "hour-exceptions", data.exceptions.map((x) => ({
          date: x.date, is_closed: x.is_closed, note: x.note || null,
          open: x.is_closed ? null : x.open ?? null, close: x.is_closed ? null : x.close ?? null,
        })))}>
        {data.exceptions.map((x, i) => {
          const set = (patch: Partial<HourException>) => setData({ ...data, exceptions: data.exceptions.map((y, j) => (j === i ? { ...y, ...patch } : y)) });
          return (
            <div className="d-row e-list-item" key={i}>
              <input type="date" aria-label={t("Tanggal")} value={x.date} disabled={!canEdit} onChange={(e) => set({ date: e.target.value })} />
              <label className="d-check u-m-0">
                <input type="checkbox" checked={x.is_closed} disabled={!canEdit} onChange={(e) => set({ is_closed: e.target.checked, open: e.target.checked ? null : "08:00", close: e.target.checked ? null : "12:00" })} />{" "}{t("Tutup")}
              </label>
              {!x.is_closed && <>
                <input type="time" aria-label={t("Waktu buka")} value={x.open ?? ""} disabled={!canEdit} onChange={(e) => set({ open: e.target.value })} />
                <input type="time" aria-label={t("Waktu tutup")} value={x.close ?? ""} disabled={!canEdit} onChange={(e) => set({ close: e.target.value })} />
              </>}
              <input aria-label={t("Catatan")} placeholder={t("Catatan")} value={x.note ?? ""} maxLength={120} disabled={!canEdit} onChange={(e) => set({ note: e.target.value })} />
              <button type="button" className="d-icon-btn" aria-label={t("Hapus tanggal")} disabled={!canEdit} onClick={() => setData({ ...data, exceptions: data.exceptions.filter((_, j) => j !== i) })}><Trash weight="bold" /></button>
            </div>
          );
        })}
        <p><button type="button" className="d-btn d-btn-sm" disabled={!canEdit}
          onClick={() => setData({ ...data, exceptions: [...data.exceptions, { date: new Date().toISOString().slice(0, 10), is_closed: true }] })}><Plus weight="bold" aria-hidden="true" />{t("Tambah tanggal")}</button></p>
      </Section>
    </Assets.Provider>
  );
}
