"use client";

// Pelanggan (P3-3): pencarian, profil, tag & catatan internal, alamat, riwayat pesanan,
// export dan permintaan hapus (ADR-0005, ADR-0012).

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { STATUS_TEXT, rupiah } from "@/dashboard/assets";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Summary { id: string; name: string; email: string | null; phone: string | null; tags: string[]; has_account: boolean; created_at: string; deletion_requested: boolean }
interface Customer extends Summary {
  internal_note: string | null;
  addresses: { label: string | null; address: string; note: string | null }[];
  orders: { id: string; code: string; status: string; total: string; created_at: string }[];
}

export default function CustomersPage() {
  return <WithTenant>{(ctx) => <Customers ctx={ctx} />}</WithTenant>;
}

function Customers({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/customers`;
  const [items, setItems] = useState<Summary[]>([]);
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Summary[] }>("GET", `${base}?limit=100${q ? `&q=${encodeURIComponent(q)}` : ""}`).then((r) => setItems(r.items), (e) => setErr(describe(e))),
    [base, q],
  );
  useEffect(() => { void load(); }, [load]);
  return (
    <>
      <h1>Pelanggan</h1>
      <Alert kind="error">{err}</Alert>
      {open ? (
        <Detail ctx={ctx} url={`${base}/${open}`} onBack={() => { setOpen(null); void load(); }} />
      ) : (
        <>
          <input aria-label="Cari pelanggan" placeholder="Cari nama / email / telepon" value={q} onChange={(e) => setQ(e.target.value)} style={{ marginBottom: 12 }} />
          <div className="d-card" style={{ overflowX: "auto" }}>
            <table className="d-table">
              <thead><tr><th>Nama</th><th>Kontak</th><th>Tag</th><th /></tr></thead>
              <tbody>
                {items.map((c) => (
                  <tr key={c.id}>
                    <td>{c.name}{c.has_account && <span className="d-muted"> · akun</span>}{c.deletion_requested && <span className="d-muted"> · dijadwalkan hapus</span>}</td>
                    <td>{c.email}<div className="d-muted">{c.phone}</div></td>
                    <td>{c.tags.join(", ")}</td>
                    <td><button className="d-btn d-btn-sm" onClick={() => setOpen(c.id)}>Buka</button></td>
                  </tr>
                ))}
                {items.length === 0 && (
                  <tr>
                    <td colSpan={4}>
                      <div className="d-empty">
                        <p className="d-empty-title">Belum ada pelanggan</p>
                        <p className="d-muted">Pelanggan tercatat otomatis saat mereka memesan lewat website toko Anda.</p>
                        <Link className="d-btn" href={`/website?tenant=${ctx.tenant.id}`}>Periksa Website</Link>
                      </div>
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </>
      )}
    </>
  );
}

function Detail({ ctx, url, onBack }: { ctx: TenantCtx; url: string; onBack: () => void }) {
  const can = ctx.can("customers.manage");
  const [c, setC] = useState<Customer | null>(null);
  const [f, setF] = useState({ name: "", email: "", phone: "", tags: "", internal_note: "" });
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const load = useCallback(() => api<Customer>("GET", url).then((x) => {
    setC(x);
    setF({ name: x.name, email: x.email ?? "", phone: x.phone ?? "", tags: x.tags.join(", "), internal_note: x.internal_note ?? "" });
  }, (e) => setMsg({ err: describe(e) })), [url]);
  useEffect(() => { void load(); }, [load]);
  async function run(fn: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await fn(); setMsg({ ok }); await load(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  async function exportJson() {
    try {
      const data = await api<unknown>("GET", `${url}/export`);
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
      a.download = `pelanggan-${c?.id ?? "data"}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
    } catch (e) { setMsg({ err: describe(e) }); }
  }
  if (!c) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  return (
    <div className="d-card">
      <button className="d-btn d-btn-sm" onClick={onBack}>← Daftar pelanggan</button>
      <h2>{c.name}</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <div className="d-grid">
        <label className="d-field"><span>Nama</span><input disabled={!can} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></label>
        <label className="d-field"><span>Email</span><input disabled={!can} value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} /></label>
        <label className="d-field"><span>Telepon</span><input disabled={!can} value={f.phone} onChange={(e) => setF({ ...f, phone: e.target.value })} /></label>
        <label className="d-field"><span>Tag (pisahkan koma)</span><input disabled={!can} value={f.tags} onChange={(e) => setF({ ...f, tags: e.target.value })} /></label>
      </div>
      <label className="d-field"><span>Catatan internal (tidak pernah terlihat pelanggan)</span>
        <textarea disabled={!can} rows={3} maxLength={2000} value={f.internal_note} onChange={(e) => setF({ ...f, internal_note: e.target.value })} />
      </label>
      {can && (
        <div className="d-row">
          <button className="d-btn d-btn-primary" onClick={() => run(() => api("PUT", url, { json: {
            name: f.name, email: f.email || null, phone: f.phone || null,
            tags: f.tags.split(",").map((t) => t.trim()).filter(Boolean), internal_note: f.internal_note || null,
          } }), "Tersimpan.")}>Simpan</button>
          <button className="d-btn" onClick={exportJson}>Export data (JSON)</button>
          {!c.deletion_requested && (
            <button className="d-btn d-btn-danger" onClick={() => {
              if (window.confirm("Data pelanggan akan dianonimkan otomatis 30 hari setelah permintaan ini. Lanjutkan?")) {
                void run(() => api("POST", `${url}/deletion-request`), "Permintaan hapus dicatat.");
              }
            }}>Minta hapus data</button>
          )}
        </div>
      )}
      <h3>Alamat</h3>
      <ul>{c.addresses.map((a, i) => <li key={i}>{a.label && `${a.label}: `}{a.address}</li>)}</ul>
      {c.addresses.length === 0 && <p className="d-muted">Belum ada alamat.</p>}
      <h3>Riwayat pesanan</h3>
      <ul>{c.orders.map((o) => <li key={o.id}>{o.code} — {STATUS_TEXT[o.status] ?? o.status} — {rupiah(o.total)}</li>)}</ul>
      {c.orders.length === 0 && <p className="d-muted">Belum ada pesanan.</p>}
    </div>
  );
}
