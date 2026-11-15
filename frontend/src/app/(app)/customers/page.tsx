"use client";

// Pelanggan (P3-3): pencarian, profil, tag & catatan internal, alamat, riwayat pesanan,
// export dan permintaan hapus (ADR-0005, ADR-0012).

import Link from "@/ui/link";
import { useCallback, useEffect, useState } from "react";

import { DownloadSimple, Trash, Users } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { ORDER_STATUS, relatif, rupiah, status, tanggal } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, BusyButton, Card, EmptyState, PageHeader, PageSkeleton, SearchInput, Skeleton } from "@/ui/page";
import { toast } from "@/ui/toast";

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
  const [items, setItems] = useState<Summary[] | null>(null);
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Summary[] }>("GET", `${base}?limit=100${q ? `&q=${encodeURIComponent(q)}` : ""}`).then((r) => { setItems(r.items); setErr(null); }, (e) => setErr(describe(e))),
    [base, q],
  );
  useEffect(() => {
    const t = setTimeout(() => void load(), q ? 250 : 0);
    return () => clearTimeout(t);
  }, [load, q]);

  if (open) return <Detail ctx={ctx} url={`${base}/${open}`} onBack={() => { setOpen(null); void load(); }} />;
  return (
    <>
      <PageHeader title="Pelanggan" description="Pelanggan tercatat otomatis saat memesan. Tambahkan tag dan catatan internal untuk layanan yang lebih personal." />
      <Alert kind="error">{err}</Alert>
      <div className="d-toolbar">
        <SearchInput label="Cari pelanggan" placeholder="Cari nama, email, atau telepon…" value={q} onChange={setQ} />
      </div>
      <div className="d-table-wrap">
        {items === null ? <div style={{ padding: 20 }}><Skeleton lines={5} /></div> : items.length === 0 ? (
          <EmptyState icon={Users} title={q ? "Tidak ada yang cocok" : "Belum ada pelanggan"}
            action={!q ? <Link className="d-btn" href={`/website?tenant=${ctx.tenant.id}`}>Periksa Website</Link> : undefined}>
            {q ? "Coba nama, email, atau nomor lain." : "Pelanggan tercatat otomatis saat mereka memesan lewat website toko Anda."}
          </EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>Nama</th><th>Kontak</th><th className="d-hide-sm">Tag</th><th className="d-hide-sm">Sejak</th><th><span className="sr-only">Aksi</span></th></tr></thead>
            <tbody>
              {items.map((c) => (
                <tr key={c.id}>
                  <td>
                    <div className="d-cell-main">
                      <span className="d-avatar d-avatar-muted" aria-hidden="true">{(c.name.trim()[0] ?? "?").toUpperCase()}</span>
                      <div>
                        <span className="d-strong">{c.name}</span>
                        <span className="d-sub">
                          {c.has_account ? "Punya akun" : "Tamu"}
                          {c.deletion_requested && " · dijadwalkan hapus"}
                        </span>
                      </div>
                    </div>
                  </td>
                  <td>{c.email ?? <span className="d-muted">—</span>}{c.phone && <span className="d-sub">{c.phone}</span>}</td>
                  <td className="d-hide-sm"><div className="d-chips">{c.tags.map((t) => <span key={t} className="d-chip" style={{ paddingRight: 10 }}>{t}</span>)}</div></td>
                  <td className="d-hide-sm d-muted">{tanggal(c.created_at)}</td>
                  <td className="d-actions"><button className="d-btn d-btn-sm" onClick={() => setOpen(c.id)}>Buka</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}

function Detail({ ctx, url, onBack }: { ctx: TenantCtx; url: string; onBack: () => void }) {
  const can = ctx.can("customers.manage");
  const [c, setC] = useState<Customer | null>(null);
  const [f, setF] = useState({ name: "", email: "", phone: "", tags: "", internal_note: "" });
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => api<Customer>("GET", url).then((x) => {
    setC(x);
    setF({ name: x.name, email: x.email ?? "", phone: x.phone ?? "", tags: x.tags.join(", "), internal_note: x.internal_note ?? "" });
  }, (e) => setErr(describe(e))), [url]);
  useEffect(() => { void load(); }, [load]);
  async function run(fn: () => Promise<unknown>, ok: string) {
    setErr(null);
    setBusy(true);
    try { await fn(); toast.ok(ok); await load(); } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  async function exportJson() {
    try {
      const data = await api<unknown>("GET", `${url}/export`);
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
      a.download = `pelanggan-${c?.id ?? "data"}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
    } catch (e) { setErr(describe(e)); }
  }
  if (!c) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const spent = c.orders.filter((o) => o.status === "COMPLETED").reduce((n, o) => n + Number(o.total), 0);
  return (
    <>
      <PageHeader
        back={{ onClick: onBack, label: "Semua pelanggan" }}
        title={c.name}
        description={`${c.has_account ? "Punya akun" : "Pelanggan tamu"} · tercatat sejak ${tanggal(c.created_at)}`}
        actions={can ? (
          <>
            <button className="d-btn" onClick={exportJson}><DownloadSimple weight="bold" aria-hidden="true" />Ekspor data</button>
            {!c.deletion_requested && (
              <button className="d-btn d-btn-danger" onClick={async () => {
                if (await confirm({ title: "Minta hapus data pelanggan?", body: "Data pelanggan akan dianonimkan otomatis 30 hari setelah permintaan ini. Riwayat pesanan tetap tersimpan tanpa identitas.", confirmLabel: "Minta hapus data", danger: true })) {
                  void run(() => api("POST", `${url}/deletion-request`), "Permintaan hapus dicatat.");
                }
              }}><Trash weight="bold" aria-hidden="true" />Minta hapus data</button>
            )}
          </>
        ) : undefined}
      />
      {c.deletion_requested && <Alert kind="warn">Data pelanggan ini dijadwalkan untuk dianonimkan.</Alert>}
      <Alert kind="error">{err}</Alert>
      <div className="d-split">
        <div style={{ minWidth: 0 }}>
          <Card title="Profil" description="Catatan internal tidak pernah terlihat pelanggan.">
            <form onSubmit={(e) => {
              e.preventDefault();
              void run(() => api("PUT", url, { json: {
                name: f.name, email: f.email || null, phone: f.phone || null,
                tags: f.tags.split(",").map((t) => t.trim()).filter(Boolean), internal_note: f.internal_note || null,
              } }), "Tersimpan.");
            }}>
              <div className="d-grid">
                <label className="d-field"><span>Nama</span><input disabled={!can} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></label>
                <label className="d-field"><span>Email</span><input type="email" disabled={!can} value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} /></label>
                <label className="d-field"><span>Telepon</span><input type="tel" disabled={!can} value={f.phone} onChange={(e) => setF({ ...f, phone: e.target.value })} /></label>
                <label className="d-field"><span>Tag <span className="d-opt">(pisahkan koma)</span></span><input disabled={!can} placeholder="mis. langganan, grosir" value={f.tags} onChange={(e) => setF({ ...f, tags: e.target.value })} /></label>
              </div>
              <label className="d-field"><span>Catatan internal</span>
                <textarea disabled={!can} rows={3} maxLength={2000} placeholder="mis. Suka kopi tanpa gula" value={f.internal_note} onChange={(e) => setF({ ...f, internal_note: e.target.value })} />
              </label>
              {can && <BusyButton busy={busy}>Simpan</BusyButton>}
            </form>
          </Card>
          <Card flush title="Riwayat pesanan">
            {c.orders.length === 0 ? <EmptyState title="Belum ada pesanan" /> : (
              <div className="d-table-wrap">
                <table className="d-table">
                  <thead><tr><th>Pesanan</th><th>Status</th><th className="d-num">Total</th></tr></thead>
                  <tbody>
                    {c.orders.map((o) => {
                      const st = status(ORDER_STATUS, o.status);
                      return (
                        <tr key={o.id}>
                          <td className="d-nowrap"><Link className="d-strong" style={{ textDecoration: "none" }} href={`/orders?tenant=${ctx.tenant.id}&open=${o.id}`}>{o.code}</Link><span className="d-sub">{relatif(o.created_at)}</span></td>
                          <td><Badge tone={st.tone}>{st.label}</Badge></td>
                          <td className="d-num">{rupiah(o.total)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        </div>
        <div className="d-stack" style={{ alignContent: "start" }}>
          <Card title="Ringkasan">
            <dl className="d-dl">
              <dt>Pesanan</dt><dd>{c.orders.length}</dd>
              <dt>Belanja selesai</dt><dd>{rupiah(spent)}</dd>
              <dt>Terakhir pesan</dt><dd>{c.orders[0] ? relatif(c.orders[0].created_at) : "—"}</dd>
            </dl>
          </Card>
          <Card title="Alamat">
            {c.addresses.length === 0 ? <p className="d-muted">Belum ada alamat.</p> : (
              <ul className="d-facts">{c.addresses.map((a, i) => <li key={i}><span><strong>{a.label ?? `Alamat ${i + 1}`}</strong><br />{a.address}{a.note && <><br /><span className="d-muted">{a.note}</span></>}</span></li>)}</ul>
            )}
          </Card>
        </div>
      </div>
    </>
  );
}
