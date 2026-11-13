"use client";

// Langganan toko (P7-1..P7-3, ADR-0016): paket & status, pemakaian vs batas, tagihan + unggah bukti bayar manual.

import { useCallback, useEffect, useState } from "react";

import { Check, UploadSimple } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { API_ORIGIN, api, describe } from "@/lib/api";
import { INVOICE_STATUS, SUBSCRIPTION_STATUS, status as statusOf } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";

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
  const [msg, setMsg] = useState<{ err?: string }>({});
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
    try { await fn(); await load(); toast.ok(ok); } catch (e) { setMsg({ err: describe(e) }); } finally { setBusy(false); }
  }

  if (!sub) return <><Alert kind="error">{msg.err}</Alert>{!msg.err && <PageSkeleton />}</>;
  const open = invoices.find((x) => x.status === "awaiting_payment" || x.status === "submitted");
  const u = sub.usage;
  const L = sub.plan.limits;
  const st = statusOf(SUBSCRIPTION_STATUS, sub.status);
  const meters: { label: string; used: number; max: number | null; fmt?: (n: number | null) => string; none?: boolean }[] = [
    { label: "Produk", used: u.products, max: L.max_products },
    { label: "Anggota tim (termasuk undangan)", used: u.members + u.pending_invitations, max: L.max_members },
    { label: "Penyimpanan file", used: u.storage_bytes, max: L.storage_bytes, fmt: mb },
    { label: "Percakapan bulan ini", used: u.conversations_month, max: L.conversations_monthly },
    { label: "Token AI platform bulan ini", used: u.platform_tokens_month, max: L.platform_ai_tokens_monthly, none: L.platform_ai_tokens_monthly === 0 },
  ];
  return (
    <>
      <PageHeader title="Langganan" description="Paket toko, pemakaian terhadap batas paket, dan tagihan." />
      <Alert kind="error">{msg.err}</Alert>
      {sub.status === "expired" && (
        <div className="d-alert d-alert-error" role="alert"><div>Langganan berakhir: website, katalog, checkout, dan chat toko sedang nonaktif. Pilih paket dan bayar untuk mengaktifkan kembali.</div></div>
      )}
      <section className="d-card" aria-label="Paket saat ini">
        <header className="d-card-head">
          <div className="d-card-head-text">
            <h2>Paket {sub.plan.name}</h2>
            <p>
              {sub.status === "trialing" && <>Uji coba berakhir {date(sub.trial_ends_at)}.</>}
              {sub.status === "active" && <>Aktif sampai {date(sub.current_period_end)}.</>}
              {sub.status === "expired" && <>Berakhir.</>}
            </p>
          </div>
          <Badge tone={st.tone}>{st.label}</Badge>
        </header>
        <div className="d-grid2">
          {meters.map((m) => {
            const pct = m.max ? Math.min(m.used / m.max, 1) : 0;
            const f = m.fmt ?? ((n: number | null) => limit(n));
            return (
              <div key={m.label} className="d-meter">
                <div className="d-meter-head"><span>{m.label}</span><span>{m.none ? "Tidak termasuk" : `${m.fmt ? m.fmt(m.used) : m.used.toLocaleString("id-ID")} / ${f(m.max)}`}</span></div>
                {m.max ? <div className="d-progress"><span style={{ transform: `scaleX(${pct})`, background: pct >= 1 ? "var(--danger)" : pct > 0.8 ? "var(--warn)" : undefined }} /></div> : null}
              </div>
            );
          })}
        </div>
        {L.byok_allowed && <p className="d-muted d-small" style={{ marginTop: 16 }}>Asisten AI dapat memakai kunci API milik toko sendiri.</p>}
      </section>

      {open ? (
        <section className="d-card" aria-label="Tagihan berjalan">
          <header className="d-card-head">
            <div className="d-card-head-text">
              <h2>Tagihan {open.code}</h2>
              <p>Paket {plans.find((p) => p.code === open.plan_code)?.name ?? open.plan_code} — <strong>{rupiah(open.amount_idr)}</strong> untuk {open.period_days} hari.</p>
            </div>
            <Badge tone={statusOf(INVOICE_STATUS, open.status).tone}>{INV[open.status]}</Badge>
          </header>
          <div className="d-alert d-alert-info" role="note">
            <div><strong>Cara bayar:</strong> {instructions || "Hubungi admin platform untuk instruksi pembayaran."} Cantumkan kode <strong>{open.code}</strong>.</div>
          </div>
          {canPay && (
            <label className="d-upload">
              <UploadSimple weight="bold" aria-hidden="true" />
              <span><strong>{open.has_proof ? "Ganti bukti bayar" : "Unggah bukti bayar"}</strong><span className="d-muted d-small">PNG, JPEG, atau WebP · maks. 2 MB</span></span>
              <input type="file" className="sr-only" aria-label={`${open.has_proof ? "Ganti bukti bayar" : "Unggah bukti bayar"} (PNG/JPEG/WebP, maks 2 MB)`} accept="image/png,image/jpeg,image/webp" disabled={busy} onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) void run(() => api("POST", `${base}/invoices/${open.id}/proof`, { bytes: f }), "Bukti terkirim. Menunggu verifikasi admin platform.");
              }} />
            </label>
          )}
          <div className="d-row" style={{ marginTop: 16 }}>
            {open.has_proof && <a className="d-btn" href={`${API_ORIGIN}${base}/invoices/${open.id}/proof`} target="_blank" rel="noreferrer">Lihat bukti yang diunggah</a>}
            {canPay && <button className="d-btn d-btn-ghost" disabled={busy} onClick={async () => {
              if (await confirm({ title: `Batalkan tagihan ${open.code}?`, body: "Anda bisa memilih paket lagi setelahnya.", confirmLabel: "Batalkan tagihan", cancelLabel: "Kembali", danger: true })) void run(() => api("POST", `${base}/invoices/${open.id}/cancel`), "Tagihan dibatalkan.");
            }}>Batalkan tagihan</button>}
          </div>
        </section>
      ) : canPay && (
        <section aria-label="Pilih paket" style={{ marginBottom: 16 }}>
          <h2 className="d-section-title" style={{ marginTop: 8 }}>Pilih paket</h2>
          <div className="d-plans">
            {plans.map((p) => {
              const current = sub.plan.code === p.code;
              return (
                <div key={p.code} className="d-card d-plan" data-current={current ? "true" : undefined}>
                  <div className="d-row d-row-between"><strong>{p.name}</strong>{current && <Badge tone="brand" plain>Paket saat ini</Badge>}</div>
                  <div className="d-plan-price">{rupiah(p.price_idr)} <small>/ {p.period_days} hari</small></div>
                  <ul className="d-checklist">
                    <li><Check weight="bold" aria-hidden="true" />{limit(p.limits.max_products)} produk</li>
                    <li><Check weight="bold" aria-hidden="true" />{limit(p.limits.max_members)} anggota tim</li>
                    <li><Check weight="bold" aria-hidden="true" />{mb(p.limits.storage_bytes)} penyimpanan</li>
                    <li><Check weight="bold" aria-hidden="true" />{limit(p.limits.conversations_monthly)} percakapan/bulan</li>
                    <li><Check weight="bold" aria-hidden="true" />{p.limits.platform_ai_tokens_monthly === 0 ? "Tanpa token AI platform" : `${limit(p.limits.platform_ai_tokens_monthly)} token AI/bulan`}</li>
                  </ul>
                  <button className={`d-btn ${current ? "" : "d-btn-primary"} d-btn-block`} disabled={busy} onClick={() => void run(() => api("POST", `${base}/invoices`, { json: { plan_code: p.code } }), `Tagihan paket ${p.name} dibuat.`)}>
                    {sub.status === "active" && current ? "Perpanjang" : `Pilih ${p.name}`}
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      )}

      {invoices.length > 0 && (
        <section className="d-table-wrap" aria-label="Riwayat tagihan">
          <table className="d-table">
            <caption className="an-caption">Riwayat tagihan</caption>
            <thead><tr><th scope="col">Kode</th><th scope="col">Paket</th><th scope="col" className="d-num">Nominal</th><th scope="col">Status</th></tr></thead>
            <tbody>
              {invoices.map((x) => {
                const ist = statusOf(INVOICE_STATUS, x.status);
                return (
                  <tr key={x.id}>
                    <th scope="row">{x.code}</th><td>{plans.find((p) => p.code === x.plan_code)?.name ?? x.plan_code}</td><td className="d-num">{rupiah(x.amount_idr)}</td>
                    <td><Badge tone={ist.tone}>{INV[x.status]}</Badge>{x.status === "paid" && x.period_end && <span className="d-sub">s.d. {date(x.period_end)}</span>}{x.rejection_reason && <span className="d-sub">{x.rejection_reason}</span>}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </section>
      )}
      <p className="d-muted d-small">Ketentuan paket & pembayaran: lihat <a href="/legal/syarat" target="_blank" rel="noopener">Syarat & Ketentuan</a> bagian 3.</p>
    </>
  );
}
