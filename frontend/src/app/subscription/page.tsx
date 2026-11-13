"use client";

// Langganan toko (P7-1..P7-3, ADR-0016): paket & status, pemakaian vs batas, tagihan + unggah bukti bayar manual.

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { API_ORIGIN, api, describe } from "@/lib/api";

interface Limits {
  max_members: number | null; max_products: number | null; platform_ai_tokens_monthly: number | null;
  byok_allowed: boolean; storage_bytes: number | null; conversations_monthly: number | null; custom_domain: boolean;
}
interface Plan { code: string; name: string; price_idr: number; period_days: number; trial_days: number | null; limits: Limits }
interface Subscription {
  plan: Plan; status: "trialing" | "active" | "expired"; trial_ends_at: string | null; current_period_end: string | null;
  usage: { members: number; pending_invitations: number; products: number; storage_bytes: number; conversations_month: number; platform_tokens_month: number };
}
interface Invoice {
  id: string; code: string; plan_code: string; amount_idr: number; period_days: number;
  status: "awaiting_payment" | "submitted" | "paid" | "rejected" | "cancelled";
  has_proof: boolean; rejection_reason: string | null; period_end: string | null; created_at: string;
}

const rupiah = (n: number) => `Rp${n.toLocaleString("id-ID")}`;
const date = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString("id-ID", { dateStyle: "long" }) : "—");
const STATUS: Record<Subscription["status"], string> = { trialing: "Uji coba", active: "Aktif", expired: "Berakhir" };
const INV: Record<Invoice["status"], string> = {
  awaiting_payment: "Menunggu pembayaran", submitted: "Menunggu verifikasi", paid: "Lunas", rejected: "Ditolak", cancelled: "Dibatalkan",
};

function limit(v: number | null, unit = ""): string {
  if (v === null) return "Tak terbatas";
  return `${v.toLocaleString("id-ID")}${unit}`;
}
function mb(bytes: number | null): string {
  return bytes === null ? "Tak terbatas" : `${Math.round(bytes / 1048576).toLocaleString("id-ID")} MB`;
}

export default function SubscriptionPage() {
  return <WithTenant>{(ctx) => <View ctx={ctx} />}</WithTenant>;
}

function View({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/subscription`;
  const [sub, setSub] = useState<Subscription | null>(null);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [instructions, setInstructions] = useState("");
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const [busy, setBusy] = useState(false);
  const canPay = ctx.can("tenant.update");

  const load = useCallback(async () => {
    const [s, p, i] = await Promise.all([
      api<Subscription>("GET", base),
      api<Plan[]>("GET", "/api/v1/plans"),
      api<{ items: Invoice[]; payment_instructions: string }>("GET", `${base}/invoices`),
    ]);
    setSub(s);
    setPlans(p);
    setInvoices(i.items);
    setInstructions(i.payment_instructions);
  }, [base]);
  useEffect(() => {
    (async () => {
      try { await load(); } catch (e) { setMsg({ err: describe(e) }); }
    })();
  }, [load]);

  async function run(fn: () => Promise<unknown>, ok: string) {
    setBusy(true);
    setMsg({});
    try { await fn(); await load(); setMsg({ ok }); } catch (e) { setMsg({ err: describe(e) }); } finally { setBusy(false); }
  }

  if (!sub) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  const open = invoices.find((x) => x.status === "awaiting_payment" || x.status === "submitted");
  const u = sub.usage;
  const L = sub.plan.limits;
  return (
    <>
      <h1>Langganan</h1>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <section className="d-card" aria-label="Paket saat ini">
        <h2>Paket {sub.plan.name} — {STATUS[sub.status]}</h2>
        {sub.status === "trialing" && <p>Uji coba berakhir {date(sub.trial_ends_at)}.</p>}
        {sub.status === "active" && <p>Aktif sampai {date(sub.current_period_end)}.</p>}
        {sub.status === "expired" && (
          <div className="d-alert d-alert-error" role="alert">Langganan berakhir: website, katalog, checkout, dan chat toko sedang nonaktif. Pilih paket dan bayar untuk mengaktifkan kembali.</div>
        )}
        <table className="d-table">
          <caption className="an-caption">Pemakaian</caption>
          <thead><tr><th scope="col">Batas</th><th scope="col">Terpakai</th><th scope="col">Maks</th></tr></thead>
          <tbody>
            <tr><th scope="row">Produk</th><td>{u.products}</td><td>{limit(L.max_products)}</td></tr>
            <tr><th scope="row">Anggota tim (termasuk undangan)</th><td>{u.members + u.pending_invitations}</td><td>{limit(L.max_members)}</td></tr>
            <tr><th scope="row">Penyimpanan file</th><td>{mb(u.storage_bytes)}</td><td>{mb(L.storage_bytes)}</td></tr>
            <tr><th scope="row">Percakapan bulan ini</th><td>{u.conversations_month}</td><td>{limit(L.conversations_monthly)}</td></tr>
            <tr><th scope="row">Token AI platform bulan ini</th><td>{u.platform_tokens_month.toLocaleString("id-ID")}</td><td>{L.platform_ai_tokens_monthly === 0 ? "Tidak termasuk" : limit(L.platform_ai_tokens_monthly)}</td></tr>
          </tbody>
        </table>
        {L.byok_allowed && <p className="d-muted">Asisten AI dapat memakai kunci provider milik toko sendiri (BYOK).</p>}
      </section>

      {open ? (
        <section className="d-card" aria-label="Tagihan berjalan">
          <h2>Tagihan {open.code}</h2>
          <p>Paket {plans.find((p) => p.code === open.plan_code)?.name ?? open.plan_code} — <strong>{rupiah(open.amount_idr)}</strong> untuk {open.period_days} hari. Status: <strong>{INV[open.status]}</strong>.</p>
          <div className="d-alert" role="note">
            <strong>Cara bayar:</strong> {instructions || "Hubungi admin platform untuk instruksi pembayaran."} Cantumkan kode {open.code}.
          </div>
          {canPay && (
            <label className="d-field"><span>{open.has_proof ? "Ganti bukti bayar" : "Unggah bukti bayar"} (PNG/JPEG/WebP, maks 2 MB)</span>
              <input type="file" accept="image/png,image/jpeg,image/webp" disabled={busy} onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) void run(() => api("POST", `${base}/invoices/${open.id}/proof`, { bytes: f }), "Bukti terkirim. Menunggu verifikasi admin platform.");
              }} />
            </label>
          )}
          {open.has_proof && <p><a href={`${API_ORIGIN}${base}/invoices/${open.id}/proof`} target="_blank" rel="noreferrer">Lihat bukti yang diunggah</a></p>}
          {canPay && <button className="d-btn" disabled={busy} onClick={() => void run(() => api("POST", `${base}/invoices/${open.id}/cancel`), "Tagihan dibatalkan.")}>Batalkan tagihan</button>}
        </section>
      ) : canPay && (
        <section className="d-card" aria-label="Pilih paket">
          <h2>Pilih paket</h2>
          <div className="an-grid">
            {plans.map((p) => (
              <div key={p.code} className="an-tile">
                <div className="an-label">{p.name}</div>
                <div className="an-value">{rupiah(p.price_idr)}</div>
                <div className="an-note">per {p.period_days} hari</div>
                <ul className="sub-feat">
                  <li>{limit(p.limits.max_products)} produk</li>
                  <li>{limit(p.limits.max_members)} anggota tim</li>
                  <li>{mb(p.limits.storage_bytes)} penyimpanan</li>
                  <li>{limit(p.limits.conversations_monthly)} percakapan/bulan</li>
                  <li>{p.limits.platform_ai_tokens_monthly === 0 ? "Tanpa token AI platform" : `${limit(p.limits.platform_ai_tokens_monthly)} token AI/bulan`}</li>
                </ul>
                <button className="d-btn d-btn-primary" disabled={busy} onClick={() => void run(() => api("POST", `${base}/invoices`, { json: { plan_code: p.code } }), `Tagihan paket ${p.name} dibuat.`)}>
                  {sub.status === "active" && sub.plan.code === p.code ? "Perpanjang" : `Pilih ${p.name}`}
                </button>
              </div>
            ))}
          </div>
        </section>
      )}

      {invoices.length > 0 && (
        <section className="d-card" aria-label="Riwayat tagihan" style={{ overflowX: "auto" }}>
          <table className="d-table">
            <caption className="an-caption">Riwayat tagihan</caption>
            <thead><tr><th scope="col">Kode</th><th scope="col">Paket</th><th scope="col">Nominal</th><th scope="col">Status</th></tr></thead>
            <tbody>
              {invoices.map((x) => (
                <tr key={x.id}>
                  <th scope="row">{x.code}</th><td>{x.plan_code}</td><td>{rupiah(x.amount_idr)}</td>
                  <td>{INV[x.status]}{x.status === "paid" && x.period_end ? ` (s.d. ${date(x.period_end)})` : ""}{x.rejection_reason ? ` — ${x.rejection_reason}` : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      <p className="d-muted">Ketentuan paket & pembayaran: lihat <a href="/legal/syarat" target="_blank" rel="noopener">Syarat & Ketentuan</a> bagian 3.</p>
    </>
  );
}
