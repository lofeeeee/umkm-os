"use client";

// Pengaturan toko online (P3): QRIS statis milik toko (ADR-0004), ongkir, approval, SoD.
// Hanya Owner (tenant.update) yang dapat mengubah; pergantian QRIS tercatat di audit log.

import { useEffect, useState } from "react";

import { useUploads } from "@/dashboard/assets";
import { Assets, ImagePicker } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Settings {
  qris: string | null; qris_receiver_name: string | null; checkout_enabled: boolean; delivery_fee: string;
  order_approval_required: boolean; reservation_ttl_minutes: number; separation_of_duties: boolean;
}

export default function ShopSettingsPage() {
  return <WithTenant>{(ctx) => <Form ctx={ctx} />}</WithTenant>;
}

function Form({ ctx }: { ctx: TenantCtx }) {
  const url = `/api/v1/tenants/${ctx.tenant.id}/commerce/settings`;
  const can = ctx.can("tenant.update");
  const assets = useUploads(ctx.tenant.id, can);
  const [s, setS] = useState<Settings | null>(null);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  useEffect(() => { api<Settings>("GET", url).then(setS, (e) => setMsg({ err: describe(e) })); }, [url]);
  if (!s) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  async function save() {
    setMsg({});
    try { setS(await api<Settings>("PUT", url, { json: s })); setMsg({ ok: "Tersimpan." }); } catch (e) { setMsg({ err: describe(e) }); }
  }
  return (
    <Assets.Provider value={assets}>
      <h1>Toko online</h1>
      <div className="d-card">
        <h2>Pembayaran QRIS</h2>
        <p className="d-muted">
          Unggah gambar QRIS merchant GoPay milik toko. Pembeli membayar langsung ke rekening toko; Anda
          mengonfirmasi setiap pembayaran di menu Pesanan. Platform tidak memegang dana.
        </p>
        <Alert kind="error">{msg.err}</Alert>
        <Alert kind="ok">{msg.ok}</Alert>
        <ImagePicker label="Gambar QRIS" value={s.qris ?? undefined} onChange={(v) => setS({ ...s, qris: v ?? null })} />
        <label className="d-field"><span>Nama penerima (tampil di halaman bayar)</span>
          <input disabled={!can} maxLength={80} value={s.qris_receiver_name ?? ""} onChange={(e) => setS({ ...s, qris_receiver_name: e.target.value || null })} />
        </label>
        <label className="d-check"><input type="checkbox" disabled={!can} checked={s.checkout_enabled} onChange={(e) => setS({ ...s, checkout_enabled: e.target.checked })} /> Terima pesanan online</label>
        <h2>Pengiriman & pesanan</h2>
        <label className="d-field"><span>Ongkir datar untuk pengiriman (Rp)</span>
          <input disabled={!can} inputMode="decimal" value={s.delivery_fee} onChange={(e) => setS({ ...s, delivery_fee: e.target.value })} />
        </label>
        <p className="d-muted">Pilihan ambil di toko / dikirim diatur di Profil bisnis.</p>
        <label className="d-field"><span>Batas waktu pembayaran (menit, 15–1440)</span>
          <input disabled={!can} type="number" min={15} max={1440} value={s.reservation_ttl_minutes} onChange={(e) => setS({ ...s, reservation_ttl_minutes: Number(e.target.value) })} />
        </label>
        <label className="d-check"><input type="checkbox" disabled={!can} checked={s.order_approval_required} onChange={(e) => setS({ ...s, order_approval_required: e.target.checked })} /> Pesanan yang sudah dibayar perlu disetujui sebelum diproses</label>
        <label className="d-check"><input type="checkbox" disabled={!can} checked={s.separation_of_duties} onChange={(e) => setS({ ...s, separation_of_duties: e.target.checked })} /> Penyetuju harus berbeda dari staf yang mengonfirmasi pembayaran</label>
        {can && <button className="d-btn d-btn-primary" onClick={save}>Simpan</button>}
      </div>
    </Assets.Provider>
  );
}
