"use client";

// Audit log (P4-5): hanya pemilik izin audit.read. Data append-only; tidak ada aksi ubah.

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Row { id: string; actor_type: string; actor_user_id: string | null; action: string; resource_type: string; resource_id: string | null; summary: Record<string, unknown>; created_at: string }

const PREFIXES = ["auth.", "member.", "role.", "business.", "website.", "commerce.", "product.", "inventory.", "order.", "payment.", "ticket.", "approval.", "customer."];

export default function AuditPage() {
  return <WithTenant>{(ctx) => <Audit ctx={ctx} />}</WithTenant>;
}

function Audit({ ctx }: { ctx: TenantCtx }) {
  const [rows, setRows] = useState<Row[]>([]);
  const [total, setTotal] = useState(0);
  const [action, setAction] = useState("");
  const [offset, setOffset] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Row[]; total: number }>("GET", `/api/v1/tenants/${ctx.tenant.id}/audit-logs?limit=50&offset=${offset}${action ? `&action=${encodeURIComponent(action)}` : ""}`)
      .then((r) => { setRows(r.items); setTotal(r.total); setErr(null); }, (e) => setErr(describe(e))),
    [ctx.tenant.id, action, offset],
  );
  useEffect(() => { void load(); }, [load]);
  return (
    <>
      <h1>Audit log</h1>
      <Alert kind="error">{err}</Alert>
      <div className="d-row" style={{ marginBottom: 12 }}>
        <select aria-label="Jenis aksi" value={action} onChange={(e) => { setAction(e.target.value); setOffset(0); }}>
          <option value="">Semua aksi</option>
          {PREFIXES.map((p) => <option key={p} value={p}>{p}*</option>)}
        </select>
        <span className="d-muted">{total} catatan</span>
      </div>
      <div className="d-card" style={{ overflowX: "auto" }}>
        <table className="d-table">
          <thead><tr><th>Waktu</th><th>Aksi</th><th>Pelaku</th><th>Objek</th><th>Ringkasan</th></tr></thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.id}>
                <td>{new Date(r.created_at).toLocaleString("id-ID")}</td>
                <td>{r.action}</td>
                <td>{r.actor_type}{r.actor_user_id ? ` ${r.actor_user_id.slice(0, 8)}` : ""}</td>
                <td>{r.resource_type}{r.resource_id ? ` ${r.resource_id.slice(0, 12)}` : ""}</td>
                <td><code style={{ fontSize: "0.75rem" }}>{JSON.stringify(r.summary).slice(0, 160)}</code></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="d-row">
        <button className="d-btn d-btn-sm" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 50))}>Sebelumnya</button>
        <button className="d-btn d-btn-sm" disabled={offset + 50 >= total} onClick={() => setOffset(offset + 50)}>Berikutnya</button>
      </div>
    </>
  );
}
