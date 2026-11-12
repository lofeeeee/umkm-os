"use client";

// Pesanan (P3-5..P3-8): daftar, detail, verifikasi pembayaran QRIS (ADR-0004), approval,
// status pemrosesan, refund.

import { useCallback, useEffect, useState } from "react";

import { STATUS_TEXT, rupiah } from "@/dashboard/assets";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { API_ORIGIN, api, describe } from "@/lib/api";

interface Summary { id: string; code: string; status: string; customer_name: string | null; total: string; payment_status: string | null; created_at: string }
interface Order {
  id: string; code: string; customer_id: string; status: string; fulfillment: string;
  customer_name: string | null; customer_email: string | null; customer_phone: string | null;
  delivery_address: string | null; customer_note: string | null;
  items: { product_name: string; variant_name: string; sku: string; unit_price: string; quantity: number; line_total: string }[];
  subtotal: string; delivery_fee: string; total: string; created_at: string; expires_at: string;
  payment: {
    status: string; amount: string; amount_received: string | null; payer_reference: string | null; has_proof: boolean;
    verifier_note: string | null; rejection_reason: string | null; refunded: string;
  } | null;
  events: { from_status: string | null; to_status: string; actor_type: string; reason: string | null; created_at: string }[];
}

const PAY: Record<string, string> = { pending: "Belum dibayar", claimed: "Pembeli menandai sudah bayar", confirmed: "Terkonfirmasi", rejected: "Ditolak", expired: "Kedaluwarsa" };

export default function OrdersPage() {
  return <WithTenant>{(ctx) => <Orders ctx={ctx} />}</WithTenant>;
}

function Orders({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/orders`;
  const [items, setItems] = useState<Summary[]>([]);
  const [status, setStatus] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Summary[] }>("GET", `${base}?limit=100${status ? `&status=${status}` : ""}`).then((r) => setItems(r.items), (e) => setErr(describe(e))),
    [base, status],
  );
  useEffect(() => { void load(); }, [load]);
  return (
    <>
      <h1>Pesanan</h1>
      <Alert kind="error">{err}</Alert>
      <div className="d-row" style={{ marginBottom: 12 }}>
        <select aria-label="Filter status" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Semua status</option>
          {Object.entries(STATUS_TEXT).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <button className="d-btn d-btn-sm" onClick={() => void load()}>Muat ulang</button>
      </div>
      {open ? (
        <Detail ctx={ctx} base={base} id={open} onBack={() => { setOpen(null); void load(); }} />
      ) : (
        <div className="d-card" style={{ overflowX: "auto" }}>
          <table className="d-table">
            <thead><tr><th>Kode</th><th>Pelanggan</th><th>Total</th><th>Status</th><th>Pembayaran</th><th /></tr></thead>
            <tbody>
              {items.map((o) => (
                <tr key={o.id}>
                  <td>{o.code}<div className="d-muted">{new Date(o.created_at).toLocaleString("id-ID")}</div></td>
                  <td>{o.customer_name ?? "—"}</td>
                  <td>{rupiah(o.total)}</td>
                  <td>{STATUS_TEXT[o.status] ?? o.status}</td>
                  <td>{o.payment_status ? PAY[o.payment_status] ?? o.payment_status : "—"}</td>
                  <td><button className="d-btn d-btn-sm" onClick={() => setOpen(o.id)}>Buka</button></td>
                </tr>
              ))}
              {items.length === 0 && <tr><td colSpan={6} className="d-muted">Belum ada pesanan.</td></tr>}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

const NEXT: Record<string, { to: string; label: string; perm: string; fulfillment?: string }[]> = {
  APPROVED: [{ to: "PROCESSING", label: "Mulai proses", perm: "orders.approve" }],
  PROCESSING: [
    { to: "READY_FOR_PICKUP", label: "Siap diambil", perm: "orders.approve", fulfillment: "pickup" },
    { to: "SHIPPED", label: "Dikirim", perm: "orders.approve", fulfillment: "delivery" },
  ],
  READY_FOR_PICKUP: [{ to: "COMPLETED", label: "Selesai", perm: "orders.approve" }],
  SHIPPED: [{ to: "COMPLETED", label: "Selesai", perm: "orders.approve" }],
  AWAITING_PAYMENT: [{ to: "CANCELLED", label: "Batalkan pesanan", perm: "orders.cancel" }],
  PENDING: [{ to: "CANCELLED", label: "Batalkan pesanan", perm: "orders.cancel" }],
};

function Detail({ ctx, base, id, onBack }: { ctx: TenantCtx; base: string; id: string; onBack: () => void }) {
  const url = `${base}/${id}`;
  const [o, setO] = useState<Order | null>(null);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [reason, setReason] = useState("");
  const [refund, setRefund] = useState({ amount: "", reason: "", restock: false });
  const [proof, setProof] = useState<string | null>(null);
  const load = useCallback(() => api<Order>("GET", url).then((x) => { setO(x); setAmount((a) => a || x.total); }, (e) => setMsg({ err: describe(e) })), [url]);
  useEffect(() => { void load(); }, [load]);
  async function act(path: string, body: unknown, ok: string) {
    setMsg({});
    try { setO(await api<Order>("POST", `${url}${path}`, { json: body })); setMsg({ ok }); setReason(""); } catch (e) { setMsg({ err: describe(e) }); }
  }
  async function showProof() {
    // Bukti bayar = data pelanggan: diambil dengan sesi staf, ditampilkan sebagai object URL lokal.
    const r = await fetch(`${API_ORIGIN}${url}/payment/proof`, { credentials: "include" });
    if (r.ok) setProof(URL.createObjectURL(await r.blob()));
    else setMsg({ err: "Bukti tidak dapat dimuat." });
  }
  if (!o) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  const p = o.payment;
  const open = o.status === "AWAITING_PAYMENT" && p && (p.status === "pending" || p.status === "claimed");
  return (
    <div className="d-card">
      <button className="d-btn d-btn-sm" onClick={onBack}>← Daftar pesanan</button>
      <h2>Pesanan {o.code} · {STATUS_TEXT[o.status] ?? o.status}</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <p>
        {o.customer_name} · {o.customer_email ?? ""} {o.customer_phone ?? ""}<br />
        {o.fulfillment === "delivery" ? `Kirim ke: ${o.delivery_address ?? ""}` : "Ambil di toko"}
        {o.customer_note && <><br />Catatan: {o.customer_note}</>}
      </p>
      <table className="d-table">
        <thead><tr><th>Produk</th><th>SKU</th><th>Jumlah</th><th>Subtotal</th></tr></thead>
        <tbody>{o.items.map((i, k) => <tr key={k}><td>{i.product_name}{i.variant_name !== "Default" && ` · ${i.variant_name}`}</td><td>{i.sku}</td><td>{i.quantity}</td><td>{rupiah(i.line_total)}</td></tr>)}</tbody>
      </table>
      <p>Ongkir {rupiah(o.delivery_fee)} · <strong>Total {rupiah(o.total)}</strong></p>

      {p && (
        <>
          <h3>Pembayaran QRIS · {PAY[p.status] ?? p.status}</h3>
          {p.payer_reference && <p>Referensi pembeli: {p.payer_reference}</p>}
          {p.amount_received && <p>Diterima {rupiah(p.amount_received)}{p.verifier_note && ` — ${p.verifier_note}`} · Refund {rupiah(p.refunded)}</p>}
          {p.rejection_reason && <p>Alasan ditolak: {p.rejection_reason}</p>}
          {p.has_proof && (proof
            // eslint-disable-next-line @next/next/no-img-element -- object URL lokal
            ? <img src={proof} alt="Bukti pembayaran" style={{ maxWidth: 320, borderRadius: 8 }} />
            : <button className="d-btn d-btn-sm" onClick={showProof}>Lihat bukti pembayaran</button>)}
          {open && ctx.can("payments.verify") && (
            <div className="d-card">
              <p className="d-muted">Cocokkan dengan mutasi di aplikasi merchant QRIS Anda sebelum mengonfirmasi.</p>
              <div className="d-row">
                <input aria-label="Jumlah diterima" inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} />
                <input aria-label="Catatan" placeholder="Catatan (wajib bila jumlah berbeda)" maxLength={500} value={note} onChange={(e) => setNote(e.target.value)} />
                <button className="d-btn d-btn-primary" onClick={() => act("/payment/confirm", { amount_received: amount, note: note || null }, "Pembayaran dikonfirmasi.")}>Konfirmasi diterima</button>
              </div>
              <div className="d-row" style={{ marginTop: 8 }}>
                <input aria-label="Alasan tolak" placeholder="Alasan menolak (wajib)" maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} />
                <button className="d-btn d-btn-danger" onClick={() => act("/payment/reject", { reason }, "Pembayaran ditolak; pesanan dibatalkan.")}>Tolak pembayaran</button>
              </div>
            </div>
          )}
        </>
      )}

      {o.status === "APPROVAL_PENDING" && (
        <div className="d-card">
          <h3>Persetujuan</h3>
          <p className="d-muted">Staf yang mengonfirmasi pembayaran tidak dapat menyetujui sendiri (bila separation of duties aktif).</p>
          <input aria-label="Alasan" placeholder="Alasan (wajib bila menolak)" maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} />
          <div className="d-row" style={{ marginTop: 8 }}>
            {ctx.can("orders.approve") && <button className="d-btn d-btn-primary" onClick={() => act("/approve", { reason: reason || null }, "Disetujui.")}>Setujui</button>}
            {ctx.can("orders.reject") && <button className="d-btn d-btn-danger" onClick={() => act("/reject", { reason }, "Ditolak; stok dikembalikan.")}>Tolak</button>}
          </div>
        </div>
      )}

      <div className="d-row">
        {(NEXT[o.status] ?? []).filter((n) => ctx.can(n.perm) && (!n.fulfillment || n.fulfillment === o.fulfillment)).map((n) => (
          <button key={n.to} className={`d-btn${n.to === "CANCELLED" ? " d-btn-danger" : ""}`} onClick={() => act("/status", { status: n.to, reason: reason || null }, "Status diperbarui.")}>{n.label}</button>
        ))}
      </div>

      {p?.status === "confirmed" && o.status !== "REFUNDED" && ctx.can("payments.refund") && (
        <div className="d-card">
          <h3>Catat refund</h3>
          <p className="d-muted">Kembalikan dana ke pembeli di luar aplikasi, lalu catat di sini.</p>
          <div className="d-row">
            <input aria-label="Jumlah refund" inputMode="decimal" placeholder="Jumlah" value={refund.amount} onChange={(e) => setRefund({ ...refund, amount: e.target.value })} />
            <input aria-label="Alasan refund" placeholder="Alasan (wajib)" maxLength={500} value={refund.reason} onChange={(e) => setRefund({ ...refund, reason: e.target.value })} />
            <label className="d-check"><input type="checkbox" checked={refund.restock} onChange={(e) => setRefund({ ...refund, restock: e.target.checked })} /> Kembalikan stok</label>
            <button className="d-btn" onClick={() => act("/refunds", refund, "Refund tercatat.")}>Catat refund</button>
          </div>
        </div>
      )}

      <h3>Riwayat</h3>
      <ol>
        {o.events.map((e, i) => (
          <li key={i}>{new Date(e.created_at).toLocaleString("id-ID")} — {STATUS_TEXT[e.to_status] ?? e.to_status} ({e.actor_type}){e.reason && `: ${e.reason}`}</li>
        ))}
      </ol>
    </div>
  );
}
