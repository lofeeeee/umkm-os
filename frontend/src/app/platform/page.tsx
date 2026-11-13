"use client";

// Admin platform (P7-3, ADR-0016): verifikasi pembayaran langganan lintas toko. Hanya super_admin dengan sesi
// lolos MFA (ditegakkan API). Melihat bukti & setiap keputusan diaudit.

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { Alert, TopBar } from "@/dashboard/shell";
import { API_ORIGIN, ApiError, api, describe, isMfaRequired, loadSession } from "@/lib/api";

interface Invoice {
  id: string; code: string; plan_code: string; amount_idr: number; period_days: number; status: string;
  has_proof: boolean; rejection_reason: string | null; created_at: string; tenant_name: string | null; tenant_slug: string | null;
}

const rupiah = (n: number) => `Rp${n.toLocaleString("id-ID")}`;
const nf = new Intl.NumberFormat("id-ID");

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
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
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
        await api("GET", "/api/v1/platform/me");
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
      setMsg({ ok: approve ? "Pembayaran disetujui; langganan toko aktif." : "Pembayaran ditolak." });
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  return (
    <>
      <TopBar />
      <main className="d-main">
        <h1>Admin platform</h1>
        <Alert kind="error">{msg.err}</Alert>
        <Alert kind="ok">{msg.ok}</Alert>
        {state === "loading" && !msg.err && <p className="d-muted" aria-busy="true">Memuat…</p>}
        {state === "denied" && <Alert kind="error">Halaman ini khusus admin platform.</Alert>}
        {state === "ready" && stats && (
          <section aria-label="Ringkasan platform">
            <p className="d-muted">Periode {stats.from} s.d. {stats.to} (WIB). Pemakaian API dihitung sejak instance API ini berjalan.</p>
            <div className="an-grid">
              <div className="an-tile"><div className="an-label">Toko aktif</div><div className="an-value">{nf.format(stats.tenants.active)}</div>
                <div className="an-note">{nf.format(stats.tenants.suspended)} ditangguhkan · +{nf.format(stats.tenants.new_in_range)} baru</div></div>
              <div className="an-tile"><div className="an-label">Langganan</div>
                <ul className="sub-feat">{stats.subscriptions.map((s) => <li key={`${s.plan}-${s.status}`}>{s.plan} · {s.status}: {nf.format(s.count)}</li>)}</ul></div>
              <div className="an-tile"><div className="an-label">Permintaan API</div><div className="an-value">{nf.format(stats.api_usage.requests_total)}</div>
                <div className="an-note">4xx {nf.format(stats.api_usage.responses_4xx)} · 5xx {nf.format(stats.api_usage.responses_5xx)}</div></div>
              <div className="an-tile"><div className="an-label">Token AI</div><div className="an-value">{nf.format(stats.ai.platform_tokens + stats.ai.byok_tokens)}</div>
                <div className="an-note">platform {nf.format(stats.ai.platform_tokens)} · kunci toko {nf.format(stats.ai.byok_tokens)}</div></div>
              <div className="an-tile"><div className="an-label">Error provider AI</div>
                <div className="an-value">{stats.provider_error_rate === null ? "—" : `${(stats.provider_error_rate * 100).toLocaleString("id-ID", { maximumFractionDigits: 1 })}%`}</div>
                <div className="an-note">{nf.format(stats.ai.provider_errors)} dari {nf.format(stats.ai.provider_calls)} panggilan</div></div>
              <div className="an-tile"><div className="an-label">Kesehatan</div>
                <ul className="sub-feat">
                  {stats.health.checks.map((c) => <li key={c.name}>{c.name}: {c.ok ? "OK" : "GAGAL"}</li>)}
                  <li>Antrean notifikasi: {nf.format(stats.health.outbox_pending)} · gagal: {nf.format(stats.health.outbox_dead)}</li>
                </ul></div>
            </div>
          </section>
        )}
        {state === "ready" && (
          <>
            <div className="d-row" style={{ marginBottom: 12 }}>
              <label className="d-field" style={{ margin: 0 }}><span>Status</span>
                <select value={filter} onChange={(e) => setFilter(e.target.value)}>
                  <option value="submitted">Menunggu verifikasi</option>
                  <option value="paid">Lunas</option>
                  <option value="rejected">Ditolak</option>
                  <option value="all">Semua</option>
                </select>
              </label>
            </div>
            {items.length === 0 ? <p className="d-muted">Tidak ada tagihan.</p> : (
              <ul className="cv-list" aria-label="Tagihan langganan">
                {items.map((x) => (
                  <li key={x.id} className="d-card" style={{ padding: 12 }}>
                    <strong>{x.tenant_name}</strong> <span className="d-muted">({x.tenant_slug})</span>
                    <p>{x.code} · paket {x.plan_code} · <strong>{rupiah(x.amount_idr)}</strong> / {x.period_days} hari · {x.status}</p>
                    {x.has_proof && (
                      // Gambar dari API (cookie host-only api.*); akses tercatat di audit platform.
                      // eslint-disable-next-line @next/next/no-img-element
                      <img className="pf-proof" src={`${API_ORIGIN}/api/v1/platform/invoices/${x.id}/proof`} alt={`Bukti bayar ${x.code}`} />
                    )}
                    {x.rejection_reason && <p className="d-muted">Alasan: {x.rejection_reason}</p>}
                    {x.status === "submitted" && (
                      <div className="d-row">
                        <button className="d-btn d-btn-primary" onClick={() => void decide(x.id, true)}>Setujui</button>
                        <input aria-label={`Alasan penolakan ${x.code}`} placeholder="Alasan penolakan" maxLength={300}
                          value={reasons[x.id] ?? ""} onChange={(e) => setReasons({ ...reasons, [x.id]: e.target.value })} />
                        <button className="d-btn" onClick={() => void decide(x.id, false)}>Tolak</button>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </main>
    </>
  );
}
