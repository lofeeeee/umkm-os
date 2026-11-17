"use client";

// Log aktivitas (P4-5): hanya pemilik izin audit.read. Data append-only; tidak ada aksi ubah.

import { useCallback, useEffect, useState } from "react";

import { ClockCounterClockwise } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { angka, auditLabel, waktu } from "@/lib/labels";
import { EmptyState, PageHeader, Skeleton } from "@/ui/page";
import { t, tmap, m } from "@/i18n";
import { Tr } from "@/i18n/react";

interface Row { id: string; actor_type: string; actor_user_id: string | null; action: string; resource_type: string; resource_id: string | null; summary: Record<string, unknown>; created_at: string }

const PREFIXES: [string, string][] = [
  ["auth.", m("Masuk")], ["member.", m("Anggota")], ["role.", m("Peran")], ["business.", m("Profil bisnis")], ["website.", m("Website")],
  ["commerce.", m("Pengaturan toko")], ["product.", m("Produk")], ["inventory.", m("Stok")], ["order.", m("Pesanan")], ["payment.", m("Pembayaran")],
  ["ticket.", m("Tiket")], ["approval.", m("Persetujuan")], ["customer.", m("Pelanggan")],
];
const ACTOR: Record<string, string> = tmap({ user: "Staf", staff: "Staf", system: "Sistem", customer: "Pelanggan", ai: "Asisten AI", platform: "Admin platform" });
const PAGE = 50;

export default function AuditPage() {
  return <WithTenant>{(ctx) => <Audit ctx={ctx} />}</WithTenant>;
}

function Audit({ ctx }: { ctx: TenantCtx }) {
  const [rows, setRows] = useState<Row[] | null>(null);
  const [total, setTotal] = useState(0);
  const [action, setAction] = useState("");
  const [offset, setOffset] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Row[]; total: number }>("GET", `/api/v1/tenants/${ctx.tenant.id}/audit-logs?limit=${PAGE}&offset=${offset}${action ? `&action=${encodeURIComponent(action)}` : ""}`)
      .then((r) => { setRows(r.items); setTotal(r.total); setErr(null); }, (e) => setErr(describe(e))),
    [ctx.tenant.id, action, offset],
  );
  useEffect(() => { void load(); }, [load]);
  return (
    <>
      <PageHeader title={t("Log aktivitas")} description={t("Jejak setiap perubahan penting di toko: siapa, kapan, dan apa. Catatan tidak bisa diubah atau dihapus.")} />
      <Alert kind="error">{err}</Alert>
      <div className="d-toolbar">
        <select aria-label={t("Jenis aksi")} value={action} onChange={(e) => { setAction(e.target.value); setOffset(0); }} style={{ maxWidth: 240 }}>
          <option value="">{t("Semua aktivitas")}</option>
          {PREFIXES.map(([p, l]) => <option key={p} value={p}>{t(l)}</option>)}
        </select>
        <span className="d-muted d-small">{angka(total)} catatan</span>
      </div>
      <div className="d-table-wrap">
        {rows === null ? <div style={{ padding: 20 }}><Skeleton lines={6} /></div> : rows.length === 0 ? (
          <EmptyState icon={ClockCounterClockwise} title={t("Belum ada aktivitas")}>{t("Aktivitas tercatat otomatis saat tim mengubah data toko.")}</EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>{t("Aktivitas")}</th><th>{t("Pelaku")}</th><th className="d-hide-sm">{t("Objek")}</th><th>{t("Waktu")}</th></tr></thead>
            <tbody>
              {rows.map((r) => {
                const summary = Object.entries(r.summary ?? {}).filter(([, v]) => v !== null && typeof v !== "object").slice(0, 3);
                return (
                  <tr key={r.id}>
                    <td>
                      <span className="d-strong">{auditLabel(r.action)}</span>
                      <code className="d-sub" style={{ background: "none", padding: 0 }}>{r.action}</code>
                      {summary.length > 0 && <span className="d-sub">{summary.map(([k, v]) => t("{0}: {1}", k, String(v))).join(" · ").slice(0, 160)}</span>}
                    </td>
                    <td>{ACTOR[r.actor_type] ?? r.actor_type}{r.actor_user_id && <span className="d-sub">{r.actor_user_id.slice(0, 8)}</span>}</td>
                    <td className="d-hide-sm d-muted">{r.resource_type}{r.resource_id && <span className="d-sub">{r.resource_id.slice(0, 12)}</span>}</td>
                    <td className="d-nowrap d-muted">{waktu(r.created_at)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      {total > PAGE && (
        <div className="d-row d-row-between">
          <span className="d-muted d-small">{offset + 1}<Tr s="–{0} dari" a={[Math.min(offset + PAGE, total)]} />{" "}{angka(total)}</span>
          <div className="d-row">
            <button type="button" className="d-btn d-btn-sm" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>{t("Sebelumnya")}</button>
            <button type="button" className="d-btn d-btn-sm" disabled={offset + PAGE >= total} onClick={() => setOffset(offset + PAGE)}>{t("Berikutnya")}</button>
          </div>
        </div>
      )}
    </>
  );
}
