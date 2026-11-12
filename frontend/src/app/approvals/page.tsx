"use client";

// Persetujuan (P4-2): antrean permintaan, putuskan langkah aktif, kebijakan per tipe, delegasi.

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Step { role_ids: string[]; user_ids: string[] }
interface Req {
  id: string; entity_type: string; action: string; summary: string; status: string; current_step: number; steps: Step[];
  requested_by: string; expires_at: string; execution_error: string | null; executed_at: string | null; created_at: string;
  decisions: { step: number | null; decision: string; actor_user_id: string | null; on_behalf_user_id: string | null; reason: string | null; created_at: string }[];
}
interface Policy { entity_type: string; enabled: boolean; steps: Step[]; timeout_hours: number }
interface Role { id: string; name: string }
interface MemberRow { user_id: string; email: string }

const TYPE: Record<string, string> = {
  order: "Keputusan pesanan", refund: "Refund", stock_adjustment: "Penyesuaian stok",
  price_change: "Perubahan harga", ai_response: "Jawaban AI", business_config: "Pengaturan toko",
};
const STATUS: Record<string, string> = { pending: "Menunggu", approved: "Disetujui", rejected: "Ditolak", expired: "Kedaluwarsa", cancelled: "Dibatalkan" };

export default function ApprovalsPage() {
  return <WithTenant>{(ctx) => <Approvals ctx={ctx} />}</WithTenant>;
}

function Approvals({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [items, setItems] = useState<Req[]>([]);
  const [status, setStatus] = useState("pending");
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [members, setMembers] = useState<MemberRow[]>([]);
  const [reason, setReason] = useState<Record<string, string>>({});
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const load = useCallback(() => Promise.all([
    api<{ items: Req[] }>("GET", `${base}/approvals?limit=100${status ? `&status=${status}` : ""}`).then((r) => setItems(r.items)),
    api<Policy[]>("GET", `${base}/approval-policies`).then(setPolicies),
    api<Role[]>("GET", `${base}/roles`).then(setRoles).catch(() => setRoles([])),
    api<MemberRow[]>("GET", `${base}/members`).then(setMembers).catch(() => setMembers([])),
  ]).catch((e) => setMsg({ err: describe(e) })), [base, status]);
  useEffect(() => { void load(); }, [load]);
  const email = (u: string | null) => members.find((m) => m.user_id === u)?.email ?? (u ? u.slice(0, 8) : "sistem");
  async function run(f: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await f(); setMsg({ ok }); await load(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  return (
    <>
      <h1>Persetujuan</h1>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <div className="d-row" style={{ marginBottom: 12 }}>
        <select aria-label="Status" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Semua</option>
          {Object.entries(STATUS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
      </div>
      {items.length === 0 && <p className="d-muted">Tidak ada permintaan.</p>}
      {items.map((r) => (
        <div className="d-card" key={r.id}>
          <h2>{TYPE[r.entity_type] ?? r.entity_type}: {r.summary}</h2>
          <p className="d-muted">
            {STATUS[r.status] ?? r.status} · langkah {Math.min(r.current_step + 1, r.steps.length)} dari {r.steps.length} · oleh {email(r.requested_by)} · batas {new Date(r.expires_at).toLocaleString("id-ID")}
          </p>
          {r.execution_error && <Alert kind="error">Gagal diterapkan: {r.execution_error}</Alert>}
          {r.executed_at && <p>Perubahan sudah diterapkan.</p>}
          <ol>
            {r.decisions.map((d, i) => (
              <li key={i}>{new Date(d.created_at).toLocaleString("id-ID")} — {d.decision} {d.actor_user_id && `oleh ${email(d.actor_user_id)}`}{d.on_behalf_user_id && ` atas nama ${email(d.on_behalf_user_id)}`}{d.reason && `: ${d.reason}`}</li>
            ))}
          </ol>
          {r.status === "pending" && (
            <div className="d-row">
              {ctx.can("approvals.decide") && (
                <>
                  <input aria-label="Alasan" placeholder="Alasan (wajib bila menolak)" value={reason[r.id] ?? ""} onChange={(e) => setReason({ ...reason, [r.id]: e.target.value })} />
                  <button className="d-btn d-btn-primary" onClick={() => run(() => api("POST", `${base}/approvals/${r.id}/approve`, { json: { step: r.current_step, reason: reason[r.id] || null } }), "Disetujui.")}>Setujui langkah ini</button>
                  <button className="d-btn d-btn-danger" onClick={() => run(() => api("POST", `${base}/approvals/${r.id}/reject`, { json: { step: r.current_step, reason: reason[r.id] || null } }), "Ditolak.")}>Tolak</button>
                </>
              )}
              {r.requested_by === ctx.session.user_id && <button className="d-btn" onClick={() => run(() => api("POST", `${base}/approvals/${r.id}/cancel`), "Dibatalkan.")}>Batalkan permintaan saya</button>}
            </div>
          )}
        </div>
      ))}
      <Policies ctx={ctx} base={base} policies={policies} roles={roles} run={run} />
      {ctx.can("approvals.decide") && <Delegate base={base} members={members.filter((m) => m.user_id !== ctx.session.user_id)} run={run} />}
    </>
  );
}

function Policies({ ctx, base, policies, roles, run }: {
  ctx: TenantCtx; base: string; policies: Policy[]; roles: Role[]; run: (f: () => Promise<unknown>, ok: string) => Promise<void>;
}) {
  const can = ctx.can("tenant.update");
  return (
    <div className="d-card">
      <h2>Kebijakan persetujuan</h2>
      <p className="d-muted">Bila aktif, perubahan jenis ini menunggu persetujuan sebelum diterapkan. Pemohon tidak dapat menyetujui permintaannya sendiri.</p>
      <table className="d-table">
        <thead><tr><th>Jenis</th><th>Langkah 1 (role)</th><th>Langkah 2 (opsional)</th><th>Aktif</th></tr></thead>
        <tbody>
          {Object.keys(TYPE).map((et) => {
            const p = policies.find((x) => x.entity_type === et);
            const step = (i: number) => p?.steps[i]?.role_ids[0] ?? "";
            const save = (enabled: boolean, s1: string, s2: string) => run(() => api("PUT", `${base}/approval-policies/${et}`, {
              json: { enabled, timeout_hours: p?.timeout_hours ?? 48, steps: [{ role_ids: s1 ? [s1] : [] }, ...(s2 ? [{ role_ids: [s2] }] : [])] },
            }), "Kebijakan tersimpan.");
            const pick = (i: number, onPick: (v: string) => void) => (
              <select aria-label={`Langkah ${i + 1} ${TYPE[et]}`} disabled={!can} value={step(i)} onChange={(e) => onPick(e.target.value)}>
                <option value="">{i === 0 ? "Siapa pun berwenang" : "—"}</option>
                {roles.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
              </select>
            );
            return (
              <tr key={et}>
                <td>{TYPE[et]}</td>
                <td>{pick(0, (v) => void save(p?.enabled ?? false, v, step(1)))}</td>
                <td>{pick(1, (v) => void save(p?.enabled ?? false, step(0), v))}</td>
                <td><input type="checkbox" aria-label={`Aktif ${TYPE[et]}`} disabled={!can} checked={p?.enabled ?? false} onChange={(e) => void save(e.target.checked, step(0), step(1))} /></td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Delegate({ base, members, run }: { base: string; members: MemberRow[]; run: (f: () => Promise<unknown>, ok: string) => Promise<void> }) {
  const [to, setTo] = useState("");
  const [days, setDays] = useState(7);
  return (
    <div className="d-card">
      <h2>Delegasikan wewenang saya</h2>
      <div className="d-row">
        <select aria-label="Delegasi ke" value={to} onChange={(e) => setTo(e.target.value)}>
          <option value="">Pilih anggota</option>
          {members.map((m) => <option key={m.user_id} value={m.user_id}>{m.email}</option>)}
        </select>
        <input aria-label="Hari" type="number" min={1} max={90} value={days} onChange={(e) => setDays(Number(e.target.value))} />
        <button className="d-btn" disabled={!to} onClick={() => run(() => api("POST", `${base}/approval-delegations`, { json: { to_user: to, hours: days * 24 } }), "Delegasi dibuat.")}>Delegasikan</button>
      </div>
    </div>
  );
}
