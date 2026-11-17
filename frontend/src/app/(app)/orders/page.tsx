"use client";

// Pesanan (P3-5..P3-8): daftar, detail, verifikasi pembayaran QRIS (ADR-0004), approval,
// status pemrosesan, refund.

import Link from "@/ui/link";
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

import { ArrowCounterClockwise, Check, Image as ImageIcon, MapPin, Note, Phone, Receipt, Storefront, Truck, User } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, apiGet, describe } from "@/lib/api";
import { ORDER_STATUS, PAYMENT_STATUS, relatif, rupiah, status as statusOf, waktu } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, BusyButton, Card, EmptyState, PageHeader, PageSkeleton, Skeleton, Tabs, type Tone } from "@/ui/page";
import { toast } from "@/ui/toast";
import { t, tmap } from "@/i18n";
import { Tr } from "@/i18n/react";

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

/** Label pembayaran ringkas untuk tabel; label lengkap ada di detail. */
const PAY_SHORT: Record<string, { label: string; tone: Tone }> = {
  pending: { get label() { return t("Belum bayar"); }, tone: "neutral" },
  claimed: { get label() { return t("Perlu dicek"); }, tone: "warn" },
  confirmed: { get label() { return t("Lunas"); }, tone: "ok" },
  rejected: { get label() { return t("Ditolak"); }, tone: "danger" },
  expired: { get label() { return t("Kedaluwarsa"); }, tone: "neutral" },
};
const ACTOR: Record<string, string> = tmap({ customer: "Pembeli", staff: "Staf", system: "Sistem", ai: "Asisten AI" });

type TabKey = "" | "AWAITING_PAYMENT" | "APPROVAL_PENDING" | "PROCESSING" | "SHIPPED" | "READY_FOR_PICKUP" | "COMPLETED" | "CANCELLED";
const TABS: { value: TabKey; label: string }[] = [
  { value: "", get label() { return t("Semua"); } },
  { value: "AWAITING_PAYMENT", get label() { return t("Menunggu pembayaran"); } },
  { value: "APPROVAL_PENDING", get label() { return t("Perlu persetujuan"); } },
  { value: "PROCESSING", get label() { return t("Diproses"); } },
  { value: "READY_FOR_PICKUP", get label() { return t("Siap diambil"); } },
  { value: "SHIPPED", get label() { return t("Dikirim"); } },
  { value: "COMPLETED", get label() { return t("Selesai"); } },
  { value: "CANCELLED", get label() { return t("Dibatalkan"); } },
];

const openParam = () => new URLSearchParams(window.location.search).get("open");

export default function OrdersPage() {
  return <WithTenant>{(ctx) => <Orders ctx={ctx} />}</WithTenant>;
}

function setOpenParam(id: string | null) {
  const url = new URL(window.location.href);
  if (id) url.searchParams.set("open", id);
  else url.searchParams.delete("open");
  window.history.pushState(null, "", url.toString());
  window.dispatchEvent(new PopStateEvent("popstate"));
}
function subscribeUrl(cb: () => void) {
  window.addEventListener("popstate", cb);
  return () => window.removeEventListener("popstate", cb);
}

function Orders({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/orders`;
  const [items, setItems] = useState<Summary[] | null>(null);
  const [tab, setTab] = useState<TabKey>("");
  const [err, setErr] = useState<string | null>(null);
  // Detail pesanan = `?open=<id>`: bisa ditautkan dari Beranda/notifikasi dan tombol Back browser bekerja.
  const open = useSyncExternalStore(subscribeUrl, openParam, () => null);
  const load = useCallback(
    () => api<{ items: Summary[] }>("GET", `${base}?limit=100${tab ? `&status=${tab}` : ""}`).then((r) => { setItems(r.items); setErr(null); }, (e) => setErr(describe(e))),
    [base, tab],
  );
  useEffect(() => { void load(); }, [load]);

  if (open) return <Detail ctx={ctx} base={base} id={open} onBack={() => { setOpenParam(null); void load(); }} />;

  return (
    <>
      <PageHeader
        title={t("Pesanan")}
        description={t("Pesanan dari website toko. Cek bukti bayar QRIS, konfirmasi, lalu proses sampai selesai.")}
        actions={<button type="button" className="d-btn" onClick={() => void load()}><ArrowCounterClockwise weight="bold" aria-hidden="true" />{t("Muat ulang")}</button>}
      />
      <Alert kind="error">{err}</Alert>
      <Tabs label={t("Filter status pesanan")} value={tab} onChange={setTab} items={TABS} />
      <div className="d-table-wrap">
        {items === null ? <div style={{ padding: 20 }}><Skeleton lines={5} /></div> : items.length === 0 ? (
          <EmptyState
            icon={Receipt}
            title={tab ? t("Tidak ada pesanan di tab ini") : t("Belum ada pesanan")}
            action={!tab ? <Link className="d-btn d-btn-primary" href={`/website?tenant=${ctx.tenant.id}`}>{t("Buka Website")}</Link> : undefined}
          >
            {tab ? t("Pesanan dengan status ini akan muncul di sini.") : t("Pesanan dari website toko akan muncul di sini. Pastikan website sudah terbit dan pembayaran sudah diatur.")}
          </EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>{t("Pesanan")}</th><th>{t("Pelanggan")}</th><th>{t("Status")}</th><th>{t("Pembayaran")}</th><th className="d-num">{t("Total")}</th><th><span className="sr-only">{t("Aksi")}</span></th></tr></thead>
            <tbody>
              {items.map((o) => {
                const st = statusOf(ORDER_STATUS, o.status);
                const pay = o.payment_status ? PAY_SHORT[o.payment_status] : null;
                return (
                  <tr key={o.id}>
                    <td className="d-nowrap"><span className="d-strong">{o.code}</span><span className="d-sub" title={waktu(o.created_at)}>{relatif(o.created_at)}</span></td>
                    <td>{o.customer_name ?? <span className="d-muted">{t("Tamu")}</span>}</td>
                    <td><Badge tone={st.tone}>{st.label}</Badge></td>
                    <td>{pay ? <Badge tone={pay.tone} plain>{pay.label}</Badge> : <span className="d-muted">—</span>}</td>
                    <td className="d-num d-strong">{rupiah(o.total)}</td>
                    <td className="d-actions"><button type="button" className="d-btn d-btn-sm" onClick={() => setOpenParam(o.id)}>{t("Buka")}</button></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}

const NEXT: Record<string, { to: string; label: string; perm: string; fulfillment?: string }[]> = {
  APPROVED: [{ to: "PROCESSING", get label() { return t("Mulai proses"); }, perm: "orders.approve" }],
  PROCESSING: [
    { to: "READY_FOR_PICKUP", get label() { return t("Tandai siap diambil"); }, perm: "orders.approve", fulfillment: "pickup" },
    { to: "SHIPPED", get label() { return t("Tandai dikirim"); }, perm: "orders.approve", fulfillment: "delivery" },
  ],
  READY_FOR_PICKUP: [{ to: "COMPLETED", get label() { return t("Tandai selesai"); }, perm: "orders.approve" }],
  SHIPPED: [{ to: "COMPLETED", get label() { return t("Tandai selesai"); }, perm: "orders.approve" }],
  AWAITING_PAYMENT: [{ to: "CANCELLED", get label() { return t("Batalkan pesanan"); }, perm: "orders.cancel" }],
  PENDING: [{ to: "CANCELLED", get label() { return t("Batalkan pesanan"); }, perm: "orders.cancel" }],
};

/** Langkah utama pesanan untuk penunjuk progres. */
const FLOW = ["AWAITING_PAYMENT", "PROCESSING", "SHIPPED", "COMPLETED"] as const;
function flowIndex(s: string): number {
  if (s === "AWAITING_PAYMENT" || s === "PENDING") return 0;
  if (["PAID", "APPROVAL_PENDING", "APPROVED", "PROCESSING"].includes(s)) return 1;
  if (s === "SHIPPED" || s === "READY_FOR_PICKUP") return 2;
  if (s === "COMPLETED" || s === "REFUNDED") return 3;
  return -1;
}

function Detail({ ctx, base, id, onBack }: { ctx: TenantCtx; base: string; id: string; onBack: () => void }) {
  const url = `${base}/${id}`;
  const [o, setO] = useState<Order | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [reason, setReason] = useState("");
  const [refund, setRefund] = useState({ amount: "", reason: "", restock: false });
  const [proof, setProof] = useState<string | null>(null);
  const load = useCallback(() => api<Order>("GET", url).then((x) => { setO(x); setAmount((a) => a || x.total); }, (e) => setErr(describe(e))), [url]);
  useEffect(() => { void load(); }, [load]);
  async function act(key: string, path: string, body: unknown, ok: string) {
    setErr(null);
    setBusy(key);
    try { setO(await api<Order>("POST", `${url}${path}`, { json: body })); toast.ok(ok); setReason(""); } catch (e) { setErr(describe(e)); } finally { setBusy(null); }
  }
  async function showProof() {
    // Bukti bayar = data pelanggan: diambil dengan sesi staf, ditampilkan sebagai object URL lokal.
    try {
      const r = await apiGet(`${url}/payment/proof`);
      if (!r.ok) throw new Error(String(r.status));
      setProof(URL.createObjectURL(await r.blob()));
    } catch {
      setErr(t("Bukti tidak dapat dimuat."));
    }
  }
  // Object URL bukti dilepas saat diganti/ditutup agar memori tidak bocor.
  useEffect(() => () => { if (proof) URL.revokeObjectURL(proof); }, [proof]);
  if (!o) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const p = o.payment;
  const payOpen = o.status === "AWAITING_PAYMENT" && p && (p.status === "pending" || p.status === "claimed");
  const st = statusOf(ORDER_STATUS, o.status);
  const ps = p ? statusOf(PAYMENT_STATUS, p.status) : null;
  const step = flowIndex(o.status);
  const next = (NEXT[o.status] ?? []).filter((n) => ctx.can(n.perm) && (!n.fulfillment || n.fulfillment === o.fulfillment));

  return (
    <>
      <PageHeader
        back={{ onClick: onBack, label: t("Semua pesanan") }}
        title={<><Tr s="Pesanan {0}" a={[o.code]} />{" "}<span style={{ verticalAlign: "middle", marginLeft: 6 }}><Badge tone={st.tone}>{st.label}</Badge></span></>}
        description={t("Dibuat {0} · {1}", waktu(o.created_at), o.fulfillment === "delivery" ? t("Dikirim") : t("Ambil di toko"))}
        actions={next.map((n) => (
          <BusyButton key={n.to} type="button" busy={busy === n.to}
            className={n.to === "CANCELLED" ? "d-btn d-btn-danger" : "d-btn d-btn-primary"}
            onClick={async () => {
              if (n.to === "CANCELLED" && !(await confirm({ title: t("Batalkan pesanan {0}?", o.code), body: t("Stok yang dipesan dikembalikan. Tindakan ini tidak bisa dibatalkan."), confirmLabel: t("Batalkan pesanan"), cancelLabel: t("Kembali"), danger: true }))) return;
              await act(n.to, "/status", { status: n.to, reason: reason || null }, t("Status diperbarui."));
            }}>
            {n.label}
          </BusyButton>
        ))}
      />
      <Alert kind="error">{err}</Alert>

      {step >= 0 && (
        <ol className="d-steps-h" aria-label={t("Progres pesanan")}>
          {FLOW.map((f, i) => (
            <li key={f} data-state={i < step ? "done" : i === step ? "current" : "todo"}>
              <span className="d-steps-h-dot" aria-hidden="true">{i < step ? <Check weight="bold" /> : i + 1}</span>
              {f === "SHIPPED" && o.fulfillment === "pickup" ? t("Siap diambil") : statusOf(ORDER_STATUS, f).label}
            </li>
          ))}
        </ol>
      )}

      <div className="d-split">
        <div style={{ minWidth: 0 }}>
          <Card flush title={t("Rincian pesanan")}>
            <div className="d-table-wrap">
              <table className="d-table">
                <thead><tr><th>{t("Produk")}</th><th className="d-hide-sm">SKU</th><th className="d-num">{t("Jumlah")}</th><th className="d-num">{t("Subtotal")}</th></tr></thead>
                <tbody>
                  {o.items.map((i, k) => (
                    <tr key={k}>
                      <td><span className="d-strong">{i.product_name}</span>{i.variant_name && i.variant_name !== "Default" && <span className="d-sub">{i.variant_name} · {rupiah(i.unit_price)}</span>}</td>
                      <td className="d-hide-sm d-muted">{i.sku}</td>
                      <td className="d-num">{i.quantity}×</td>
                      <td className="d-num">{rupiah(i.line_total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <dl className="d-dl d-totals">
              <dt>{t("Subtotal")}</dt><dd>{rupiah(o.subtotal)}</dd>
              <dt>{t("Ongkir")}</dt><dd>{rupiah(o.delivery_fee)}</dd>
              <dt className="d-strong">{t("Total")}</dt><dd className="d-strong">{rupiah(o.total)}</dd>
            </dl>
          </Card>

          {p && ps && (
            <Card title={t("Pembayaran QRIS")} actions={<Badge tone={ps.tone}>{ps.label}</Badge>}>
              <dl className="d-dl">
                <dt>{t("Tagihan")}</dt><dd>{rupiah(p.amount)}</dd>
                {p.payer_reference && <><dt>{t("Referensi pembeli")}</dt><dd>{p.payer_reference}</dd></>}
                {p.amount_received && <><dt>{t("Diterima")}</dt><dd>{rupiah(p.amount_received)}{p.verifier_note && t(" — {0}", p.verifier_note)}</dd></>}
                {p.refunded !== "0.00" && <><dt>{t("Dikembalikan")}</dt><dd>{rupiah(p.refunded)}</dd></>}
                {p.rejection_reason && <><dt>{t("Alasan ditolak")}</dt><dd>{p.rejection_reason}</dd></>}
                {o.status === "AWAITING_PAYMENT" && <><dt>{t("Batas bayar")}</dt><dd>{waktu(o.expires_at)}</dd></>}
              </dl>
              {p.has_proof && (proof
                // eslint-disable-next-line @next/next/no-img-element -- object URL lokal
                ? <img className="pf-proof" src={proof} alt={t("Bukti pembayaran")} />
                : <button type="button" className="d-btn d-btn-sm" style={{ marginTop: 16 }} onClick={showProof}><ImageIcon weight="bold" aria-hidden="true" />{t("Lihat bukti pembayaran")}</button>)}
              {payOpen && ctx.can("payments.verify") && (
                <div className="d-verify">
                  <p className="d-muted d-small" style={{ margin: "0 0 12px" }}>{t("Cocokkan dengan mutasi di aplikasi merchant QRIS Anda sebelum mengonfirmasi.")}</p>
                  <div className="d-grid">
                    <label className="d-field"><span>{t("Jumlah diterima")}</span>
                      <div className="d-input-prefix"><span aria-hidden="true">{t("Rp")}</span><input aria-label={t("Jumlah diterima")} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} /></div>
                    </label>
                    <label className="d-field"><span>{t("Catatan")}{" "}<span className="d-opt">{t("(wajib bila jumlah berbeda)")}</span></span>
                      <input aria-label={t("Catatan")} maxLength={500} value={note} onChange={(e) => setNote(e.target.value)} />
                    </label>
                  </div>
                  <BusyButton type="button" busy={busy === "confirm"} onClick={() => act("confirm", "/payment/confirm", { amount_received: amount, note: note || null }, t("Pembayaran dikonfirmasi."))}>{t("Konfirmasi diterima")}</BusyButton>
                  <details className="d-details" style={{ marginTop: 16 }}>
                    <summary>{t("Tolak pembayaran")}</summary>
                    <div className="d-row" style={{ alignItems: "flex-end" }}>
                      <label className="d-field" style={{ flex: 1, margin: 0 }}><span>{t("Alasan menolak")}</span>
                        <input aria-label={t("Alasan tolak")} placeholder={t("mis. Dana belum masuk")} maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} />
                      </label>
                      <BusyButton type="button" className="d-btn d-btn-danger" busy={busy === "reject"} disabled={!reason.trim()}
                        onClick={async () => {
                          if (!(await confirm({ title: t("Tolak pembayaran ini?"), body: t("Pesanan akan dibatalkan dan stok dikembalikan."), confirmLabel: t("Tolak pembayaran"), danger: true }))) return;
                          await act("reject", "/payment/reject", { reason }, t("Pembayaran ditolak; pesanan dibatalkan."));
                        }}>{t("Tolak pembayaran")}</BusyButton>
                    </div>
                  </details>
                </div>
              )}
            </Card>
          )}

          {o.status === "APPROVAL_PENDING" && (
            <Card title={t("Persetujuan")} description={t("Staf yang mengonfirmasi pembayaran tidak dapat menyetujui sendiri bila pemisahan tugas aktif.")}>
              <label className="d-field"><span>{t("Alasan")}{" "}<span className="d-opt">{t("(wajib bila menolak)")}</span></span>
                <input aria-label={t("Alasan")} maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} />
              </label>
              <div className="d-row">
                {ctx.can("orders.approve") && <BusyButton type="button" busy={busy === "approve"} onClick={() => act("approve", "/approve", { reason: reason || null }, t("Pesanan disetujui."))}>{t("Setujui")}</BusyButton>}
                {ctx.can("orders.reject") && <BusyButton type="button" className="d-btn d-btn-danger" busy={busy === "rejectOrder"} disabled={!reason.trim()} onClick={() => act("rejectOrder", "/reject", { reason }, t("Pesanan ditolak; stok dikembalikan."))}>{t("Tolak")}</BusyButton>}
              </div>
            </Card>
          )}

          {p?.status === "confirmed" && o.status !== "REFUNDED" && ctx.can("payments.refund") && (
            <Card title={t("Catat pengembalian dana")} description={t("Kembalikan dana ke pembeli di luar aplikasi (transfer/QRIS), lalu catat di sini.")}>
              <div className="d-grid">
                <label className="d-field"><span>{t("Jumlah")}</span>
                  <div className="d-input-prefix"><span aria-hidden="true">{t("Rp")}</span><input aria-label={t("Jumlah pengembalian dana")} inputMode="decimal" value={refund.amount} onChange={(e) => setRefund({ ...refund, amount: e.target.value })} /></div>
                </label>
                <label className="d-field"><span>{t("Alasan")}</span>
                  <input aria-label={t("Alasan pengembalian dana")} maxLength={500} value={refund.reason} onChange={(e) => setRefund({ ...refund, reason: e.target.value })} />
                </label>
              </div>
              <label className="d-check"><input type="checkbox" checked={refund.restock} onChange={(e) => setRefund({ ...refund, restock: e.target.checked })} />{" "}{t("Kembalikan barang ke stok")}</label>
              <BusyButton type="button" className="d-btn" busy={busy === "refund"} disabled={!refund.amount || !refund.reason.trim()} onClick={() => act("refund", "/refunds", refund, t("Pengembalian dana tercatat."))}>{t("Catat pengembalian dana")}</BusyButton>
            </Card>
          )}
        </div>

        <div className="d-stack" style={{ alignContent: "start" }}>
          <Card title={t("Pelanggan")}>
            <ul className="d-facts">
              <li><User weight="bold" aria-hidden="true" /><span>{o.customer_name ?? t("Tamu")}</span></li>
              {o.customer_email && <li><Note weight="bold" aria-hidden="true" /><span>{o.customer_email}</span></li>}
              {o.customer_phone && <li><Phone weight="bold" aria-hidden="true" /><a href={`tel:${o.customer_phone}`}>{o.customer_phone}</a></li>}
              <li>{o.fulfillment === "delivery" ? <Truck weight="bold" aria-hidden="true" /> : <Storefront weight="bold" aria-hidden="true" />}
                <span>{o.fulfillment === "delivery" ? t("Kirim ke {0}", o.delivery_address ?? "—") : t("Ambil di toko")}</span></li>
              {o.customer_note && <li><MapPin weight="bold" aria-hidden="true" /><span><Tr s="Catatan: “{0}”" a={[o.customer_note]} /></span></li>}
            </ul>
          </Card>
          <Card title={t("Riwayat")} label={t("Riwayat")}>
            <ol className="d-timeline">
              {[...o.events].reverse().map((e, i) => (
                <li key={i}>
                  <strong>{statusOf(ORDER_STATUS, e.to_status).label}</strong>
                  <span className="d-muted d-small">{ACTOR[e.actor_type] ?? e.actor_type} · {waktu(e.created_at)}</span>
                  {e.reason && <span className="d-small">“{e.reason}”</span>}
                </li>
              ))}
            </ol>
          </Card>
        </div>
      </div>
    </>
  );
}
