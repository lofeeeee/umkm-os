"use client";

// Admin platform (P7-3, ADR-0016): verifikasi pembayaran langganan lintas toko. Hanya super_admin dengan sesi
// lolos MFA (ditegakkan API). Melihat bukti & setiap keputusan diaudit.

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { Receipt, ShieldWarning } from "@phosphor-icons/react/ssr";

import { Alert } from "@/dashboard/shell";
import { INVOICE_STATUS, SUBSCRIPTION_STATUS, persen, rupiah, status, tanggal } from "@/lib/labels";
import { Badge, Card, EmptyState, PageHeader, PageSkeleton, Tabs } from "@/ui/page";
import { toast } from "@/ui/toast";
import { API_ORIGIN, ApiError, api, describe, isMfaRequired, loadSession } from "@/lib/api";

interface Invoice {
  id: string; code: string; plan_code: string; amount_idr: number; period_days: number; status: string;
  has_proof: boolean; rejection_reason: string | null; created_at: string; tenant_name: string | null; tenant_slug: string | null;
}

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
          setMsg({ err: probe.status === 401 ? "Sesi berakhir. Masuk lagi." : "Gagal memeriksa akses admin platform." });
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
      toast.ok(approve ? "Pembayaran disetujui; langganan toko aktif." : "Pembayaran ditolak.");
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  const planName = (c: string) => (c === "pro" ? "Pro" : c === "starter" ? "Starter" : c);
  return (
    <>
      <PageHeader title="Admin platform" description="Verifikasi pembayaran langganan dan pantau kesehatan platform. Setiap tindakan di halaman ini tercatat di audit." />
      <Alert kind="error">{msg.err}</Alert>
      {state === "loading" && !msg.err && <PageSkeleton />}
      {state === "denied" && (
        <Card>
          <EmptyState icon={ShieldWarning} title="Khusus admin platform" action={<Link className="d-btn" href="/">Kembali ke beranda</Link>}>
            Akun Anda tidak punya akses ke halaman ini.
          </EmptyState>
        </Card>
      )}
      {state === "ready" && stats && (
        <section aria-label="Ringkasan platform">
          <div className="an-grid">
            <div className="an-tile"><div className="an-label">Toko aktif</div><div className="an-value">{nf.format(stats.tenants.active)}</div>
              <div className="an-note">{nf.format(stats.tenants.suspended)} ditangguhkan · +{nf.format(stats.tenants.new_in_range)} baru</div></div>
            <div className="an-tile"><div className="an-label">Langganan</div>
              <ul className="sub-feat">{stats.subscriptions.map((x) => <li key={`${x.plan}-${x.status}`}>{planName(x.plan)} · {status(SUBSCRIPTION_STATUS, x.status).label}: {nf.format(x.count)}</li>)}</ul></div>
            <div className="an-tile"><div className="an-label">Permintaan API</div><div className="an-value">{nf.format(stats.api_usage.requests_total)}</div>
              <div className="an-note">4xx {nf.format(stats.api_usage.responses_4xx)} · 5xx {nf.format(stats.api_usage.responses_5xx)}</div></div>
            <div className="an-tile"><div className="an-label">Token AI</div><div className="an-value">{nf.format(stats.ai.platform_tokens + stats.ai.byok_tokens)}</div>
              <div className="an-note">platform {nf.format(stats.ai.platform_tokens)} · kunci toko {nf.format(stats.ai.byok_tokens)}</div></div>
            <div className="an-tile"><div className="an-label">Error provider AI</div>
              <div className="an-value">{persen(stats.provider_error_rate)}</div>
              <div className="an-note">{nf.format(stats.ai.provider_errors)} dari {nf.format(stats.ai.provider_calls)} panggilan</div></div>
            <div className="an-tile"><div className="an-label">Kesehatan</div>
              <ul className="sub-feat">
                {stats.health.checks.map((c) => <li key={c.name}><Badge tone={c.ok ? "ok" : "danger"}>{c.name}</Badge></li>)}
                <li>Antrean notifikasi: {nf.format(stats.health.outbox_pending)} · gagal: {nf.format(stats.health.outbox_dead)}</li>
              </ul></div>
          </div>
          <p className="d-muted d-small">Periode {stats.from} s.d. {stats.to} (WIB). Pemakaian API dihitung sejak instance API ini berjalan.</p>
        </section>
      )}
      {state === "ready" && (
        <>
          <h2 className="d-section-title">Tagihan langganan</h2>
          <Tabs
            label="Filter tagihan"
            value={filter}
            onChange={setFilter}
            items={[
              { value: "submitted", label: "Menunggu verifikasi" },
              { value: "paid", label: "Lunas" },
              { value: "rejected", label: "Ditolak" },
              { value: "all", label: "Semua" },
            ]}
          />
          {items.length === 0 ? (
            <Card><EmptyState icon={Receipt} title="Tidak ada tagihan">Belum ada tagihan dengan status ini.</EmptyState></Card>
          ) : (
            <ul className="d-list" aria-label="Tagihan langganan">
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
                        {x.code} · Paket {planName(x.plan_code)} · <strong style={{ color: "var(--ink)" }}>{rupiah(x.amount_idr)}</strong> / {x.period_days} hari · dibuat {tanggal(x.created_at)}
                      </p>
                      {x.rejection_reason && <p className="d-muted d-small" style={{ margin: "4px 0 0" }}>Alasan ditolak: {x.rejection_reason}</p>}
                      {x.has_proof && (
                        // Gambar dari API (cookie host-only api.*); akses tercatat di audit platform.
                        // eslint-disable-next-line @next/next/no-img-element
                        <img className="pf-proof" src={`${API_ORIGIN}/api/v1/platform/invoices/${x.id}/proof`} alt={`Bukti bayar ${x.code}`} />
                      )}
                    </div>
                    {x.status === "submitted" && (
                      <div className="d-stack" style={{ minWidth: 260 }}>
                        <input aria-label={`Alasan penolakan ${x.code}`} placeholder="Alasan penolakan (wajib bila menolak)" maxLength={300}
                          value={reasons[x.id] ?? ""} onChange={(e) => setReasons({ ...reasons, [x.id]: e.target.value })} />
                        <div className="d-row" style={{ justifyContent: "flex-end" }}>
                          <button className="d-btn d-btn-danger" onClick={() => void decide(x.id, false)}>Tolak</button>
                          <button className="d-btn d-btn-primary" onClick={() => void decide(x.id, true)}>Setujui</button>
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
