"use client";

// Analitik percakapan (P6-6, FR-CNV-004): 8 metrik sebagai stat tile + tabel harian. Data dari agregat
// harian yang bertahan setelah purge isi percakapan (ADR-0012). Hari = tanggal Asia/Jakarta.

import { useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Analytics {
  from: string; to: string; total_conversations: number; ai_resolved: number; human_resolved: number;
  avg_first_response_ms: number | null; avg_resolution_ms: number | null; handover_rate: number | null;
  fallback_rate: number | null; provider_error_rate: number | null; ai_turns: number; provider_calls: number;
  tokens: { prompt: number; completion: number; total: number };
  daily: { day: string; started: number; handed_over: number; closed_ai: number; closed_human: number; ai_turns: number; tokens: number }[];
}

const nf = new Intl.NumberFormat("id-ID");

interface Overview {
  revenue: string; orders: number; paid_orders: number; average_order_value: string | null;
  top_products: { name: string; quantity: number; revenue: string }[];
  stock_alerts: number; stock_alert_items: { product: string; variant: string; available: number; min_stock: number }[];
  customers_total: number; customers_new: number; cancellation_rate: number | null;
  tickets_created: number; tickets_resolved: number; avg_ticket_resolution_ms: number | null;
  conversations: number; ai_resolved: number; human_resolved: number; handover_rate: number | null;
  ai_platform_tokens: number; ai_byok_tokens: number; ai_turns: number;
}

function rp(v: string | null): string {
  if (v === null) return "—";
  return `Rp${Number(v).toLocaleString("id-ID", { maximumFractionDigits: 0 })}`;
}

function pct(v: number | null): string {
  return v === null ? "—" : `${(v * 100).toLocaleString("id-ID", { maximumFractionDigits: 1 })}%`;
}

function duration(ms: number | null): string {
  if (ms === null) return "—";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} dtk`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} mnt`;
  const h = Math.floor(m / 60);
  return `${h} j ${m % 60} mnt`;
}

export default function AnalyticsPage() {
  return <WithTenant>{(ctx) => <View ctx={ctx} />}</WithTenant>;
}

function Tile({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="an-tile">
      <div className="an-label">{label}</div>
      <div className="an-value">{value}</div>
      {note && <div className="an-note">{note}</div>}
    </div>
  );
}

function View({ ctx }: { ctx: TenantCtx }) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [a, setA] = useState<Analytics | null>(null);
  const [ov, setOv] = useState<Overview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // Rentang yang diterapkan (berubah hanya lewat tombol "Terapkan").
  const [range, setRange] = useState({ from: "", to: "" });
  useEffect(() => {
    const q = new URLSearchParams();
    if (range.from) q.set("from", range.from);
    if (range.to) q.set("to", range.to);
    (async () => {
      try {
        const [r, o] = await Promise.all([
          api<Analytics>("GET", `/api/v1/tenants/${ctx.tenant.id}/analytics/conversations?${q}`),
          api<Overview>("GET", `/api/v1/tenants/${ctx.tenant.id}/analytics/overview?${q}`),
        ]);
        setErr(null);
        setA(r);
        setOv(o);
        setFrom(r.from);
        setTo(r.to);
      } catch (e) {
        setErr(describe(e));
      }
    })();
  }, [ctx.tenant.id, range]);

  // Hanya hari dengan aktivitas, terbaru dulu.
  const active = (a?.daily ?? []).filter((d) => d.started + d.handed_over + d.closed_ai + d.closed_human + d.ai_turns > 0).reverse();
  if (!ctx.can("analytics.read")) return <Alert kind="error">Anda tidak punya izin melihat analitik.</Alert>;
  return (
    <>
      <h1>Analitik</h1>
      <form className="d-row" style={{ marginBottom: 12 }} onSubmit={(e) => { e.preventDefault(); setRange({ from, to }); }}>
        <label className="d-field" style={{ margin: 0 }}><span>Dari</span><input type="date" value={from} onChange={(e) => setFrom(e.target.value)} /></label>
        <label className="d-field" style={{ margin: 0 }}><span>Sampai</span><input type="date" value={to} onChange={(e) => setTo(e.target.value)} /></label>
        <button className="d-btn">Terapkan</button>
      </form>
      <Alert kind="error">{err}</Alert>
      {!a && !err && <p className="d-muted" aria-busy="true">Memuat…</p>}
      {ov && (
        <>
          <h2>Toko</h2>
          <section className="an-grid" aria-label="Ringkasan toko">
            <Tile label="Omzet (pesanan terbayar)" value={rp(ov.revenue)} note={`${nf.format(ov.paid_orders)} dari ${nf.format(ov.orders)} pesanan`} />
            <Tile label="Rata-rata nilai pesanan" value={rp(ov.average_order_value)} />
            <Tile label="Tingkat pembatalan" value={pct(ov.cancellation_rate)} note="batal, kedaluwarsa, ditolak" />
            <Tile label="Pelanggan" value={nf.format(ov.customers_total)} note={`+${nf.format(ov.customers_new)} baru`} />
            <Tile label="Stok menipis" value={nf.format(ov.stock_alerts)} note="varian ≤ stok minimum" />
            <Tile label="Tiket" value={nf.format(ov.tickets_created)} note={`${nf.format(ov.tickets_resolved)} selesai · rata-rata ${duration(ov.avg_ticket_resolution_ms)}`} />
            <Tile label="Diselesaikan AI / staf" value={`${nf.format(ov.ai_resolved)} / ${nf.format(ov.human_resolved)}`} note={`handover ${pct(ov.handover_rate)}`} />
            <Tile label="Pemakaian AI" value={`${nf.format(ov.ai_platform_tokens + ov.ai_byok_tokens)} token`} note={`platform ${nf.format(ov.ai_platform_tokens)} · kunci sendiri ${nf.format(ov.ai_byok_tokens)}`} />
          </section>
          <div className="d-grid2">
            <section className="d-card" aria-label="Produk terlaris">
              <h3>Produk terlaris</h3>
              {ov.top_products.length === 0 ? <p className="d-muted">Belum ada pesanan terbayar.</p> : (
                <ol>{ov.top_products.map((p) => <li key={p.name}>{p.name} — {nf.format(p.quantity)} terjual · {rp(p.revenue)}</li>)}</ol>
              )}
            </section>
            <section className="d-card" aria-label="Peringatan stok">
              <h3>Peringatan stok</h3>
              {ov.stock_alert_items.length === 0 ? <p className="d-muted">Tidak ada.</p> : (
                <ul>{ov.stock_alert_items.map((s) => <li key={`${s.product}-${s.variant}`}>{s.product} ({s.variant}): tersisa {s.available}, minimum {s.min_stock}</li>)}</ul>
              )}
            </section>
          </div>
        </>
      )}
      {a && (
        <>
          <h2>Percakapan</h2>
          <section className="an-grid" aria-label="Ringkasan">
            <Tile label="Total percakapan" value={nf.format(a.total_conversations)} />
            <Tile label="Selesai oleh AI" value={nf.format(a.ai_resolved)} />
            <Tile label="Selesai oleh staf" value={nf.format(a.human_resolved)} />
            <Tile label="Rata-rata respons pertama" value={duration(a.avg_first_response_ms)} />
            <Tile label="Rata-rata sampai selesai" value={duration(a.avg_resolution_ms)} />
            <Tile label="Tingkat handover" value={pct(a.handover_rate)} note="dialihkan AI ke staf" />
            <Tile label="Tingkat fallback" value={pct(a.fallback_rate)} note={`dari ${nf.format(a.ai_turns)} giliran AI`} />
            <Tile label="Token AI" value={nf.format(a.tokens.total)} note={`${nf.format(a.tokens.prompt)} masuk · ${nf.format(a.tokens.completion)} keluar`} />
            <Tile label="Error provider" value={pct(a.provider_error_rate)} note={`dari ${nf.format(a.provider_calls)} panggilan`} />
          </section>
          <p className="d-muted">Periode {a.from} s.d. {a.to} (WIB). Angka tetap tersedia setelah isi percakapan dihapus sesuai kebijakan retensi.</p>
          {active.length === 0 && <p className="d-muted">Belum ada aktivitas pada periode ini.</p>}
          {active.length > 0 && <div className="d-card" style={{ overflowX: "auto" }}>
            <table className="d-table an-table">
              <caption className="an-caption">Rincian harian</caption>
              <thead>
                <tr><th scope="col">Tanggal</th><th scope="col">Baru</th><th scope="col">Handover</th><th scope="col">Selesai AI</th><th scope="col">Selesai staf</th><th scope="col">Giliran AI</th><th scope="col">Token</th></tr>
              </thead>
              <tbody>
                {active.map((d) => (
                  <tr key={d.day}>
                    <th scope="row">{d.day}</th><td>{nf.format(d.started)}</td><td>{nf.format(d.handed_over)}</td>
                    <td>{nf.format(d.closed_ai)}</td><td>{nf.format(d.closed_human)}</td><td>{nf.format(d.ai_turns)}</td><td>{nf.format(d.tokens)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>}
        </>
      )}
    </>
  );
}
