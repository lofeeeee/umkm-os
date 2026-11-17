"use client";

// Analitik percakapan (P6-6, FR-CNV-004): 8 metrik sebagai stat tile + tabel harian. Data dari agregat
// harian yang bertahan setelah purge isi percakapan (ADR-0012). Hari = tanggal Asia/Jakarta.

import Link from "@/ui/link";
import { useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { tanggal } from "@/lib/labels";
import { Card, PageHeader, PageSkeleton } from "@/ui/page";
import { t, intlLocale } from "@/i18n";

interface Analytics {
  from: string; to: string; total_conversations: number; ai_resolved: number; human_resolved: number;
  avg_first_response_ms: number | null; avg_resolution_ms: number | null; handover_rate: number | null;
  fallback_rate: number | null; provider_error_rate: number | null; ai_turns: number; provider_calls: number;
  tokens: { prompt: number; completion: number; total: number };
  daily: { day: string; started: number; handed_over: number; closed_ai: number; closed_human: number; ai_turns: number; tokens: number }[];
}

const nf = { format: (n: number) => new Intl.NumberFormat(intlLocale()).format(n) };

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
  return t("Rp{0}", Number(v).toLocaleString(intlLocale(), { maximumFractionDigits: 0 }));
}

function pct(v: number | null): string {
  return v === null ? "—" : `${(v * 100).toLocaleString(intlLocale(), { maximumFractionDigits: 1 })}%`;
}

function duration(ms: number | null): string {
  if (ms === null) return "—";
  const s = Math.round(ms / 1000);
  if (s < 60) return t("{0} dtk", s);
  const m = Math.round(s / 60);
  if (m < 60) return t("{0} mnt", m);
  const h = Math.floor(m / 60);
  return t("{0} j {1} mnt", h, m % 60);
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
  // Rentang yang diterapkan (berubah hanya lewat tombol "Terapkan" atau preset).
  const [range, setRange] = useState({ from: "", to: "" });
  const [preset, setPreset] = useState<number | null>(30);
  const applyPreset = (days: number) => {
    const end = new Date();
    const start = new Date(end.getTime() - (days - 1) * 86400000);
    const iso = (d: Date) => d.toLocaleDateString("en-CA", { timeZone: "Asia/Jakarta" });
    setPreset(days);
    setRange({ from: iso(start), to: iso(end) });
  };
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
  if (!ctx.can("analytics.read")) return <Alert kind="error">{t("Anda tidak punya izin melihat analitik.")}</Alert>;
  return (
    <>
      <PageHeader
        title={t("Analitik")}
        description={a ? t("Periode {0} – {1} (WIB).", tanggal(a.from), tanggal(a.to)) : t("Kinerja toko dan layanan pelanggan.")}
        actions={
          <div className="d-seg" role="group" aria-label={t("Rentang cepat")}>
            {[7, 30, 90].map((d) => <button key={d} type="button" aria-pressed={preset === d} onClick={() => applyPreset(d)}>{d} hari</button>)}
          </div>
        }
      />
      <form className="d-toolbar" onSubmit={(e) => { e.preventDefault(); setPreset(null); setRange({ from, to }); }}>
        <label className="d-row u-nowrap"><span className="d-muted d-small">{t("Dari")}</span><input type="date" aria-label={t("Dari")} value={from} onChange={(e) => setFrom(e.target.value)} style={{ width: 160 }} /></label>
        <label className="d-row u-nowrap"><span className="d-muted d-small">{t("Sampai")}</span><input type="date" aria-label={t("Sampai")} value={to} onChange={(e) => setTo(e.target.value)} style={{ width: 160 }} /></label>
        <button type="submit" className="d-btn">{t("Terapkan")}</button>
      </form>
      <Alert kind="error">{err}</Alert>
      {!a && !err && <PageSkeleton />}
      {ov && (
        <>
          <h2 className="d-section-title u-mt-2">{t("Penjualan & toko")}</h2>
          <section className="an-grid" aria-label={t("Ringkasan toko")}>
            <Tile label={t("Omzet (pesanan terbayar)")} value={rp(ov.revenue)} note={t("{0} dari {1} pesanan", nf.format(ov.paid_orders), nf.format(ov.orders))} />
            <Tile label={t("Rata-rata nilai pesanan")} value={rp(ov.average_order_value)} />
            <Tile label={t("Tingkat pembatalan")} value={pct(ov.cancellation_rate)} note={t("batal, kedaluwarsa, ditolak")} />
            <Tile label={t("Pelanggan")} value={nf.format(ov.customers_total)} note={t("+{0} baru", nf.format(ov.customers_new))} />
            <Tile label={t("Stok menipis")} value={nf.format(ov.stock_alerts)} note={t("varian ≤ stok minimum")} />
            <Tile label={t("Tiket")} value={nf.format(ov.tickets_created)} note={t("{0} selesai · rata-rata {1}", nf.format(ov.tickets_resolved), duration(ov.avg_ticket_resolution_ms))} />
            <Tile label={t("Diselesaikan AI / staf")} value={t("{0} / {1}", nf.format(ov.ai_resolved), nf.format(ov.human_resolved))} note={t("dialihkan {0}", pct(ov.handover_rate))} />
            <Tile label={t("Pemakaian AI")} value={t("{0} token", nf.format(ov.ai_platform_tokens + ov.ai_byok_tokens))} note={t("platform {0} · kunci sendiri {1}", nf.format(ov.ai_platform_tokens), nf.format(ov.ai_byok_tokens))} />
          </section>
          <div className="d-grid2">
            <Card flush title={t("Produk terlaris")} label={t("Produk terlaris")}>
              {ov.top_products.length === 0 ? (
                <p className="d-muted u-p-5 u-m-0">
                  {t("Belum ada pesanan terbayar.")}{" "}<Link href={`/products?tenant=${ctx.tenant.id}`}>{t("Pastikan produk aktif")}</Link>{" "}{t("dan bagikan tautan website toko.")}
                </p>
              ) : (
                <ol className="an-rank">
                  {ov.top_products.map((p) => {
                    const max = Math.max(...ov.top_products.map((x) => x.quantity), 1);
                    return (
                      <li key={p.name}>
                        <span className="an-rank-name">{p.name}</span>
                        <span className="an-rank-val">{nf.format(p.quantity)}{" "}{t("terjual ·")}{" "}{rp(p.revenue)}</span>
                        <span className="an-rank-bar" aria-hidden="true"><span style={{ transform: `scaleX(${p.quantity / max})` }} /></span>
                      </li>
                    );
                  })}
                </ol>
              )}
            </Card>
            <Card flush title={t("Peringatan stok")} label={t("Peringatan stok")}>
              {ov.stock_alert_items.length === 0 ? <p className="d-muted u-p-5 u-m-0">{t("Semua stok aman.")}</p> : (
                <div className="d-table-wrap"><table className="d-table">
                  <thead><tr><th>{t("Produk")}</th><th className="d-num">{t("Tersisa")}</th><th className="d-num">{t("Minimum")}</th></tr></thead>
                  <tbody>{ov.stock_alert_items.map((s) => <tr key={`${s.product}-${s.variant}`}><td>{s.product}<span className="d-sub">{s.variant}</span></td><td className="d-num" style={{ color: "var(--warn)", fontWeight: 600 }}>{s.available}</td><td className="d-num d-muted">{s.min_stock}</td></tr>)}</tbody>
                </table></div>
              )}
            </Card>
          </div>
        </>
      )}
      {a && (
        <>
          <h2 className="d-section-title">{t("Layanan pelanggan")}</h2>
          <section className="an-grid" aria-label={t("Ringkasan")}>
            <Tile label={t("Total percakapan")} value={nf.format(a.total_conversations)} />
            <Tile label={t("Selesai oleh AI")} value={nf.format(a.ai_resolved)} />
            <Tile label={t("Selesai oleh staf")} value={nf.format(a.human_resolved)} />
            <Tile label={t("Rata-rata respons pertama")} value={duration(a.avg_first_response_ms)} />
            <Tile label={t("Rata-rata sampai selesai")} value={duration(a.avg_resolution_ms)} />
            <Tile label={t("Tingkat pengalihan ke staf")} value={pct(a.handover_rate)} note={t("dialihkan AI ke staf")} />
            <Tile label={t("Tingkat fallback")} value={pct(a.fallback_rate)} note={t("dari {0} giliran AI", nf.format(a.ai_turns))} />
            <Tile label={t("Token AI")} value={nf.format(a.tokens.total)} note={t("{0} masuk · {1} keluar", nf.format(a.tokens.prompt), nf.format(a.tokens.completion))} />
            <Tile label={t("Error provider")} value={pct(a.provider_error_rate)} note={t("dari {0} panggilan", nf.format(a.provider_calls))} />
          </section>
          <p className="d-muted d-small">{t("Angka tetap tersedia setelah isi percakapan dihapus sesuai kebijakan retensi.")}</p>
          {active.length === 0 && <p className="d-muted">{t("Belum ada aktivitas pada periode ini.")}</p>}
          {active.length > 0 && <div className="d-table-wrap">
            <table className="d-table an-table">
              <caption className="an-caption">{t("Rincian harian")}</caption>
              <thead>
                <tr><th scope="col">{t("Tanggal")}</th><th scope="col" className="d-num">{t("Baru")}</th><th scope="col" className="d-num">{t("Dialihkan")}</th><th scope="col" className="d-num">{t("Selesai AI")}</th><th scope="col" className="d-num">{t("Selesai staf")}</th><th scope="col" className="d-num">{t("Giliran AI")}</th><th scope="col" className="d-num">{t("Token")}</th></tr>
              </thead>
              <tbody>
                {active.map((d) => (
                  <tr key={d.day}>
                    <th scope="row">{tanggal(d.day)}</th><td className="d-num">{nf.format(d.started)}</td><td className="d-num">{nf.format(d.handed_over)}</td>
                    <td className="d-num">{nf.format(d.closed_ai)}</td><td className="d-num">{nf.format(d.closed_human)}</td><td className="d-num">{nf.format(d.ai_turns)}</td><td className="d-num">{nf.format(d.tokens)}</td>
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
