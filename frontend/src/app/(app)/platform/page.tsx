"use client";

// Admin platform (P7-3, ADR-0016): verifikasi pembayaran langganan lintas toko. Hanya super_admin dengan sesi
// lolos MFA (ditegakkan API). Melihat bukti & setiap keputusan diaudit.

import Link from "@/ui/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { Receipt, ShieldWarning } from "@phosphor-icons/react/ssr";

import { Alert } from "@/dashboard/shell";
import { INVOICE_STATUS, SUBSCRIPTION_STATUS, persen, rupiah, status, tanggal } from "@/lib/labels";
import { Badge, Card, EmptyState, PageHeader, PageSkeleton, Tabs } from "@/ui/page";
import { toast } from "@/ui/toast";
import { API_ORIGIN, ApiError, api, describe, isMfaRequired, loadSession } from "@/lib/api";
import { t as tx, intlLocale } from "@/i18n";
import { Tr } from "@/i18n/react";

interface Invoice {
  id: string; code: string; plan_code: string; amount_idr: number; period_days: number; status: string;
  has_proof: boolean; rejection_reason: string | null; created_at: string; tenant_name: string | null; tenant_slug: string | null;
}

const nf = { format: (n: number) => new Intl.NumberFormat(intlLocale()).format(n) };

interface PlatformStats {
  from: string; to: string;
  tenants: { active: number; suspended: number; closed: number; new_in_range: number };
  subscriptions: { plan: string; status: string; count: number }[];
  api_usage: { requests_total: number; responses_2xx: number; responses_4xx: number; responses_5xx: number };
  ai: { runs: number; platform_tokens: number; byok_tokens: number; provider_calls: number; provider_errors: number };
  provider_error_rate: number | null;
  health: { checks: { name: string; ok: boolean }[]; outbox_pending: number; outbox_dead: number };
}

export default function PlatformPage() {
  const router = useRouter();
  const [state, setState] = useState<"loading" | "denied" | "ready">("loading");
  const [items, setItems] = useState<Invoice[]>([]);
  const [filter, setFilter] = useState("submitted");
  const [msg, setMsg] = useState<{ err?: string }>({});
  const [reasons, setReasons] = useState<Record<string, string>>({});
  const [stats, setStats] = useState<PlatformStats | null>(null);

  const load = useCallback(async () => {
    setItems(await api<Invoice[]>("GET", `/api/v1/platform/invoices?status=${filter}`));
  }, [filter]);

  useEffect(() => {
    (async () => {
      if (!(await loadSession())) {
        router.replace("/login");
        return;
      }
      try {
        // Cek akses tanpa memicu error jaringan di console: user non-admin memang ditolak.
        const probe = await fetch(`${API_ORIGIN}/api/v1/platform/me`, { credentials: "include" });
        if (probe.status === 403) {
          setState("denied");
          return;
        }
        if (!probe.ok) {
          setMsg({ err: probe.status === 401 ? tx("Sesi berakhir. Masuk lagi.") : tx("Gagal memeriksa akses admin platform.") });
          return;
        }
        await load();
        setStats(await api<PlatformStats>("GET", "/api/v1/platform/analytics"));
        setState("ready");
      } catch (e) {
        if (isMfaRequired(e)) {
          router.replace("/security?reason=mfa");
          return;
        }
        if (e instanceof ApiError && e.status === 403) setState("denied");
        else setMsg({ err: describe(e) });
      }
    })();
  }, [router, load]);

  async function decide(id: string, approve: boolean) {
    setMsg({});
    try {
      if (approve) await api("POST", `/api/v1/platform/invoices/${id}/approve`, { json: {} });
      else await api("POST", `/api/v1/platform/invoices/${id}/reject`, { json: { reason: reasons[id] ?? "" } });
      await load();
      toast.ok(approve ? tx("Pembayaran disetujui; langganan toko aktif.") : tx("Pembayaran ditolak."));
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  const planName = (c: string) => (c === "pro" ? tx("Pro") : c === "starter" ? tx("Starter") : c);
  return (
    <>
      <PageHeader title={tx("Admin platform")} description={tx("Verifikasi pembayaran langganan dan pantau kesehatan platform. Setiap tindakan di halaman ini tercatat di audit.")} />
      <Alert kind="error">{msg.err}</Alert>
      {state === "loading" && !msg.err && <PageSkeleton />}
      {state === "denied" && (
        <Card>
          <EmptyState icon={ShieldWarning} title={tx("Khusus admin platform")} action={<Link className="d-btn" href="/">{tx("Kembali ke beranda")}</Link>}>
            {tx("Akun Anda tidak punya akses ke halaman ini.")}
          </EmptyState>
        </Card>
      )}
      {state === "ready" && stats && (
        <section aria-label={tx("Ringkasan platform")}>
          <div className="an-grid">
            <div className="an-tile"><div className="an-label">{tx("Toko aktif")}</div><div className="an-value">{nf.format(stats.tenants.active)}</div>
              <div className="an-note">{nf.format(stats.tenants.suspended)}{" "}<Tr s="ditangguhkan · +{0} baru" a={[nf.format(stats.tenants.new_in_range)]} /></div></div>
            <div className="an-tile"><div className="an-label">{tx("Langganan")}</div>
              <ul className="sub-feat">{stats.subscriptions.map((x) => <li key={`${x.plan}-${x.status}`}>{planName(x.plan)} · {status(SUBSCRIPTION_STATUS, x.status).label}: {nf.format(x.count)}</li>)}</ul></div>
            <div className="an-tile"><div className="an-label">{tx("Permintaan API")}</div><div className="an-value">{nf.format(stats.api_usage.requests_total)}</div>
              <div className="an-note"><Tr s="4xx {0} · 5xx" a={[nf.format(stats.api_usage.responses_4xx)]} />{" "}{nf.format(stats.api_usage.responses_5xx)}</div></div>
            <div className="an-tile"><div className="an-label">{tx("Token AI")}</div><div className="an-value">{nf.format(stats.ai.platform_tokens + stats.ai.byok_tokens)}</div>
              <div className="an-note"><Tr s="platform {0} · kunci toko" a={[nf.format(stats.ai.platform_tokens)]} />{" "}{nf.format(stats.ai.byok_tokens)}</div></div>
            <div className="an-tile"><div className="an-label">{tx("Error provider AI")}</div>
              <div className="an-value">{persen(stats.provider_error_rate)}</div>
              <div className="an-note">{nf.format(stats.ai.provider_errors)}{" "}<Tr s="dari {0} panggilan" a={[nf.format(stats.ai.provider_calls)]} /></div></div>
            <div className="an-tile"><div className="an-label">{tx("Kesehatan")}</div>
              <ul className="sub-feat">
                {stats.health.checks.map((c) => <li key={c.name}><Badge tone={c.ok ? "ok" : "danger"}>{c.name}</Badge></li>)}
                <li><Tr s="Antrean notifikasi: {0} · gagal:" a={[nf.format(stats.health.outbox_pending)]} />{" "}{nf.format(stats.health.outbox_dead)}</li>
              </ul></div>
          </div>
          <p className="d-muted d-small"><Tr s="Periode {0} s.d. {1} (WIB). Pemakaian API dihitung sejak instance API ini berjalan." a={[stats.from, stats.to]} /></p>
        </section>
      )}
      {state === "ready" && <TenantsSection />}
      {state === "ready" && (
        <>
          <h2 className="d-section-title">{tx("Tagihan langganan")}</h2>
          <Tabs
            label={tx("Filter tagihan")}
            value={filter}
            onChange={setFilter}
            items={[
              { value: "submitted", label: tx("Menunggu verifikasi") },
              { value: "paid", label: tx("Lunas") },
              { value: "rejected", label: tx("Ditolak") },
              { value: "all", label: tx("Semua") },
            ]}
          />
          {items.length === 0 ? (
            <Card><EmptyState icon={Receipt} title={tx("Tidak ada tagihan")}>{tx("Belum ada tagihan dengan status ini.")}</EmptyState></Card>
          ) : (
            <ul className="d-list" aria-label={tx("Tagihan langganan")}>
              {items.map((x) => {
                const st = status(INVOICE_STATUS, x.status);
                return (
                  <li key={x.id} style={{ alignItems: "flex-start" }}>
                    <div className="d-list-main">
                      <div className="d-row">
                        <strong>{x.tenant_name}</strong>
                        <span className="d-muted d-small">{x.tenant_slug}</span>
                        <Badge tone={st.tone}>{st.label}</Badge>
                      </div>
                      <p className="d-muted d-small" style={{ margin: "4px 0 0" }}>
                        <Tr s="{0} · Paket {1} ·" a={[x.code, planName(x.plan_code)]} />{" "}<strong style={{ color: "var(--ink)" }}>{rupiah(x.amount_idr)}</strong> <Tr s="/ {0} hari · dibuat {1}" a={[x.period_days, tanggal(x.created_at)]} />
                      </p>
                      {x.rejection_reason && <p className="d-muted d-small" style={{ margin: "4px 0 0" }}>{tx("Alasan ditolak:")}{" "}{x.rejection_reason}</p>}
                      {x.has_proof && (
                        // Gambar dari API (cookie host-only api.*); akses tercatat di audit platform.
                        // eslint-disable-next-line @next/next/no-img-element
                        <img className="pf-proof" src={`${API_ORIGIN}/api/v1/platform/invoices/${x.id}/proof`} alt={tx("Bukti bayar {0}", x.code)} />
                      )}
                    </div>
                    {x.status === "submitted" && (
                      <div className="d-stack" style={{ minWidth: 260 }}>
                        <input aria-label={tx("Alasan penolakan {0}", x.code)} placeholder={tx("Alasan penolakan (wajib bila menolak)")} maxLength={300}
                          value={reasons[x.id] ?? ""} onChange={(e) => setReasons({ ...reasons, [x.id]: e.target.value })} />
                        <div className="d-row" style={{ justifyContent: "flex-end" }}>
                          <button type="button" className="d-btn d-btn-danger" onClick={() => void decide(x.id, false)}>{tx("Tolak")}</button>
                          <button type="button" className="d-btn d-btn-primary" onClick={() => void decide(x.id, true)}>{tx("Setujui")}</button>
                        </div>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
    </>
  );
}

// P11: pengawasan per-toko (UMKM) — daftar + omzet/volume + drill-down. Akses diaudit sebagai platform.
interface TenantRow {
  id: string; name: string; slug: string; tenant_status: string;
  plan: string | null; sub_status: string | null; created_at: string;
  orders: number; paid_orders: number; gmv: string; last_order_at: string | null;
}
interface TenantDetail {
  name: string; slug: string; tenant_status: string; created_at: string; primary_hostname: string | null;
  plan: string | null; sub_status: string | null; current_period_end: string | null;
  products_active: number; customers_total: number; last_order_at: string | null;
  range: { orders: number; paid_orders: number; gmv: string; by_status: Record<string, number> };
}

const LIMIT = 20;
const planLabel = (c: string | null) => (c === "pro" ? tx("Pro") : c === "starter" ? tx("Starter") : c ?? "—");
const tenantTone = (s: string) => (s === "active" ? "ok" : s === "suspended" ? "warn" : "danger");

function TenantsSection() {
  const [q, setQ] = useState("");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [data, setData] = useState<{ total: number; items: TenantRow[] } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ id: string; data: TenantDetail } | null>(null);

  useEffect(() => {
    const p = new URLSearchParams({ limit: String(LIMIT), offset: String(page * LIMIT) });
    if (query) p.set("q", query);
    api<{ total: number; items: TenantRow[] }>("GET", `/api/v1/platform/tenants?${p}`)
      .then(setData, (e) => setErr(describe(e)));
  }, [query, page]);

  useEffect(() => {
    if (!open) return;
    let live = true;
    api<{ tenant: TenantDetail }>("GET", `/api/v1/platform/tenants/${open}`)
      .then((d) => { if (live) setDetail({ id: open, data: d.tenant }); }, (e) => { if (live) setErr(describe(e)); });
    return () => { live = false; };
  }, [open]);

  // Tampilkan detail hanya bila cocok dengan baris yang dibuka (hindari data basi saat ganti toko).
  const shown = detail && detail.id === open ? detail.data : null;
  const total = data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / LIMIT));
  return (
    <section aria-label={tx("Toko (UMKM)")} style={{ marginTop: 8 }}>
      <h2 className="d-section-title">{tx("Toko (UMKM)")}</h2>
      <Alert kind="error">{err}</Alert>
      <form className="d-row" style={{ marginBottom: 12 }} onSubmit={(e) => { e.preventDefault(); setPage(0); setQuery(q.trim()); }}>
        <input aria-label={tx("Cari toko")} placeholder={tx("Cari nama atau alamat toko…")} value={q} onChange={(e) => setQ(e.target.value)} style={{ maxWidth: 320 }} />
        <button className="d-btn" type="submit">{tx("Cari")}</button>
        {query && <button className="d-btn d-btn-ghost" type="button" onClick={() => { setQ(""); setQuery(""); setPage(0); }}>{tx("Reset")}</button>}
      </form>
      {!data ? <PageSkeleton /> : data.items.length === 0 ? (
        <Card><EmptyState icon={Receipt} title={tx("Tidak ada toko")}>{tx("Tidak ada toko yang cocok.")}</EmptyState></Card>
      ) : (
        <>
          <div className="d-table-wrap">
            <table className="d-table">
              <thead><tr>
                <th>{tx("Toko")}</th><th>{tx("Paket")}</th><th>{tx("Status")}</th>
                <th className="d-num">{tx("Pesanan")}</th><th className="d-num">{tx("Omzet")}</th><th className="d-hide-sm">{tx("Terakhir aktif")}</th>
              </tr></thead>
              <tbody>
                {data.items.map((t) => (
                  <tr key={t.id} onClick={() => setOpen(open === t.id ? null : t.id)} style={{ cursor: "pointer" }}
                      tabIndex={0} aria-expanded={open === t.id}
                      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setOpen(open === t.id ? null : t.id); } }}>
                    <td><strong className="d-strong">{t.name}</strong><span className="d-sub">{t.slug}</span></td>
                    <td>{planLabel(t.plan)}{t.sub_status ? <span className="d-sub">{status(SUBSCRIPTION_STATUS, t.sub_status).label}</span> : null}</td>
                    <td><Badge tone={tenantTone(t.tenant_status)}>{t.tenant_status}</Badge></td>
                    <td className="d-num">{nf.format(t.orders)}<span className="d-sub">{nf.format(t.paid_orders)} bayar</span></td>
                    <td className="d-num">{rupiah(t.gmv)}</td>
                    <td className="d-hide-sm">{t.last_order_at ? tanggal(t.last_order_at) : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="d-row" style={{ justifyContent: "space-between", marginTop: 12 }}>
            <span className="d-muted d-small">{nf.format(total)}{" "}<Tr s="toko · halaman {0}/" a={[page + 1]} />{pages}</span>
            <div className="d-row">
              <button type="button" className="d-btn d-btn-sm d-btn-ghost" disabled={page === 0} onClick={() => setPage((p) => Math.max(0, p - 1))}>{tx("Sebelumnya")}</button>
              <button type="button" className="d-btn d-btn-sm d-btn-ghost" disabled={page + 1 >= pages} onClick={() => setPage((p) => p + 1)}>{tx("Berikutnya")}</button>
            </div>
          </div>
        </>
      )}
      {open && (
        <Card title={shown ? shown.name : tx("Memuat…")} actions={<button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => setOpen(null)}>{tx("Tutup")}</button>}>
          {!shown ? <PageSkeleton /> : (
            <div className="an-grid">
              <div className="an-tile"><div className="an-label">{tx("Omzet (periode)")}</div><div className="an-value">{rupiah(shown.range.gmv)}</div>
                <div className="an-note">{nf.format(shown.range.paid_orders)}{" "}<Tr s="dari {0} pesanan" a={[nf.format(shown.range.orders)]} /></div></div>
              <div className="an-tile"><div className="an-label">{tx("Paket")}</div><div className="an-value" style={{ fontSize: 20 }}>{planLabel(shown.plan)}</div>
                <div className="an-note">{shown.sub_status ? status(SUBSCRIPTION_STATUS, shown.sub_status).label : tx("tanpa langganan")}{shown.current_period_end ? tx(" · s.d. {0}", tanggal(shown.current_period_end)) : ""}</div></div>
              <div className="an-tile"><div className="an-label">{tx("Katalog & pelanggan")}</div><div className="an-value" style={{ fontSize: 20 }}>{nf.format(shown.products_active)} produk</div>
                <div className="an-note">{nf.format(shown.customers_total)}{" "}{tx("pelanggan · bergabung")}{" "}{tanggal(shown.created_at)}</div></div>
              <div className="an-tile"><div className="an-label">{tx("Pesanan per status (periode)")}</div>
                <ul className="sub-feat">{Object.entries(shown.range.by_status).map(([s, n]) => <li key={s}>{s}: {nf.format(n)}</li>)}
                  {Object.keys(shown.range.by_status).length === 0 && <li className="d-muted">{tx("Tidak ada pesanan.")}</li>}</ul></div>
              {shown.primary_hostname && (
                <div className="an-tile"><div className="an-label">{tx("Situs")}</div>
                  <a className="d-link" href={`https://${shown.primary_hostname}`} target="_blank" rel="noreferrer">{shown.primary_hostname}</a></div>
              )}
            </div>
          )}
        </Card>
      )}
    </section>
  );
}
