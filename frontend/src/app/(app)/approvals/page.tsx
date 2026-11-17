"use client";

// Persetujuan (P4-2): antrean permintaan, putuskan langkah aktif, kebijakan per tipe, delegasi.

import { useCallback, useEffect, useState } from "react";

import { CheckSquare } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe, roleLabel } from "@/lib/api";
import { APPROVAL_STATUS, APPROVAL_TYPE as TYPE, status as statusOf, waktu } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, Card, EmptyState, PageHeader, Skeleton, Tabs } from "@/ui/page";
import { toast } from "@/ui/toast";
import { t } from "@/i18n";

interface Step { role_ids: string[]; user_ids: string[] }
interface Req {
  id: string; entity_type: string; action: string; summary: string; status: string; current_step: number; steps: Step[];
  requested_by: string; expires_at: string; execution_error: string | null; executed_at: string | null; created_at: string;
  decisions: { step: number | null; decision: string; actor_user_id: string | null; on_behalf_user_id: string | null; reason: string | null; created_at: string }[];
}
interface Policy { entity_type: string; enabled: boolean; steps: Step[]; timeout_hours: number }
interface Role { id: string; name: string }
interface MemberRow { user_id: string; email: string }

const DECISION: Record<string, string> = { approve: "menyetujui", approved: "menyetujui", reject: "menolak", rejected: "menolak", cancel: "membatalkan", cancelled: "membatalkan", expire: "kedaluwarsa", expired: "kedaluwarsa" };
const TABS = [
  { value: "pending", get label() { return t("Menunggu"); } },
  { value: "approved", get label() { return t("Disetujui"); } },
  { value: "rejected", get label() { return t("Ditolak"); } },
  { value: "", get label() { return t("Semua"); } },
];

export default function ApprovalsPage() {
  return <WithTenant>{(ctx) => <Approvals ctx={ctx} />}</WithTenant>;
}

function Approvals({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [items, setItems] = useState<Req[] | null>(null);
  const [status, setStatus] = useState("pending");
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [members, setMembers] = useState<MemberRow[]>([]);
  const [reason, setReason] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => Promise.all([
    api<{ items: Req[] }>("GET", `${base}/approvals?limit=100${status ? `&status=${status}` : ""}`).then((r) => setItems(r.items)),
    api<Policy[]>("GET", `${base}/approval-policies`).then(setPolicies),
    api<Role[]>("GET", `${base}/roles`).then(setRoles).catch(() => setRoles([])),
    api<MemberRow[]>("GET", `${base}/members`).then(setMembers).catch(() => setMembers([])),
  ]).catch((e) => setErr(describe(e))), [base, status]);
  useEffect(() => { void load(); }, [load]);
  const email = (u: string | null) => members.find((m) => m.user_id === u)?.email ?? (u ? u.slice(0, 8) : "sistem");
  async function run(f: () => Promise<unknown>, ok: string) {
    setErr(null);
    try { await f(); toast.ok(ok); await load(); } catch (e) { setErr(describe(e)); }
  }
  return (
    <>
      <PageHeader title={t("Persetujuan")} description={t("Perubahan sensitif yang menunggu keputusan sebelum diterapkan. Pemohon tidak dapat menyetujui permintaannya sendiri.")} />
      <Alert kind="error">{err}</Alert>
      <Tabs label={t("Status permintaan")} value={status} onChange={setStatus} items={TABS} />
      {items === null ? <div className="d-card"><Skeleton lines={4} /></div> : items.length === 0 ? (
        <div className="d-card">
          <EmptyState icon={CheckSquare} title={status === "pending" ? t("Tidak ada yang menunggu persetujuan") : t("Tidak ada permintaan")}
            action={<a className="d-btn" href="#kebijakan-persetujuan">{t("Atur kebijakan persetujuan")}</a>}>
            {t("Permintaan muncul di sini bila toko mewajibkan persetujuan untuk perubahan tertentu.")}
          </EmptyState>
        </div>
      ) : (
        <div className="d-stack u-mb-6">
          {items.map((r) => {
            const st = statusOf(APPROVAL_STATUS, r.status);
            return (
              <Card key={r.id}
                title={<>{TYPE[r.entity_type] ?? r.entity_type}: {r.summary}</>}
                description={t("Diminta {0} · langkah {1} dari {2} · batas {3}", email(r.requested_by), Math.min(r.current_step + 1, r.steps.length), r.steps.length, waktu(r.expires_at))}
                actions={<Badge tone={st.tone}>{st.label}</Badge>}
              >
                {r.execution_error && <Alert kind="error">{t("Gagal diterapkan:")}{" "}{r.execution_error}</Alert>}
                {r.executed_at && <Alert kind="ok">{t("Perubahan sudah diterapkan.")}</Alert>}
                {r.decisions.length > 0 && (
                  <ol className="d-timeline u-mb-4">
                    {r.decisions.map((d, i) => (
                      <li key={i}>
                        <span>{d.actor_user_id ? email(d.actor_user_id) : t("Sistem")} {DECISION[d.decision] ?? d.decision}{d.on_behalf_user_id && t(" atas nama {0}", email(d.on_behalf_user_id))}{d.reason && t(": “{0}”", d.reason)}</span>
                        <span className="d-muted d-small">{waktu(d.created_at)}</span>
                      </li>
                    ))}
                  </ol>
                )}
                {r.status === "pending" && (
                  <div className="d-row u-items-end">
                    {ctx.can("approvals.decide") && (
                      <>
                        <label className="d-field" style={{ flex: "1 1 240px", margin: 0 }}><span>{t("Alasan")}{" "}<span className="d-opt">{t("(wajib bila menolak)")}</span></span>
                          <input aria-label={t("Alasan")} value={reason[r.id] ?? ""} onChange={(e) => setReason({ ...reason, [r.id]: e.target.value })} />
                        </label>
                        <button type="button" className="d-btn d-btn-danger" disabled={!(reason[r.id] ?? "").trim()} onClick={() => run(() => api("POST", `${base}/approvals/${r.id}/reject`, { json: { step: r.current_step, reason: reason[r.id] || null } }), t("Ditolak."))}>{t("Tolak")}</button>
                        <button type="button" className="d-btn d-btn-primary" onClick={() => run(() => api("POST", `${base}/approvals/${r.id}/approve`, { json: { step: r.current_step, reason: reason[r.id] || null } }), t("Disetujui."))}>{t("Setujui langkah ini")}</button>
                      </>
                    )}
                    {r.requested_by === ctx.session.user_id && (
                      <button type="button" className="d-btn d-btn-ghost" onClick={async () => {
                        if (await confirm({ title: t("Batalkan permintaan ini?"), confirmLabel: t("Batalkan permintaan"), cancelLabel: t("Kembali"), danger: true })) void run(() => api("POST", `${base}/approvals/${r.id}/cancel`), t("Permintaan dibatalkan."));
                      }}>{t("Batalkan permintaan saya")}</button>
                    )}
                  </div>
                )}
              </Card>
            );
          })}
        </div>
      )}
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
    <Card flush id="kebijakan-persetujuan" title={t("Kebijakan persetujuan")} description={t("Bila aktif, perubahan jenis ini menunggu persetujuan sebelum diterapkan.")}>
      <div className="d-table-wrap">
        <table className="d-table d-table-stack">
          <thead><tr><th>{t("Jenis perubahan")}</th><th>{t("Langkah 1 (peran)")}</th><th>{t("Langkah 2")}{" "}<span className="d-opt">{t("(opsional)")}</span></th><th>{t("Aktif")}</th></tr></thead>
          <tbody>
            {Object.keys(TYPE).map((et) => {
              const p = policies.find((x) => x.entity_type === et);
              const step = (i: number) => p?.steps[i]?.role_ids[0] ?? "";
              const save = (enabled: boolean, s1: string, s2: string) => run(() => api("PUT", `${base}/approval-policies/${et}`, {
                json: { enabled, timeout_hours: p?.timeout_hours ?? 48, steps: [{ role_ids: s1 ? [s1] : [] }, ...(s2 ? [{ role_ids: [s2] }] : [])] },
              }), t("Kebijakan tersimpan."));
              const pick = (i: number, onPick: (v: string) => void) => (
                <select aria-label={t("Langkah {0} {1}", i + 1, TYPE[et])} disabled={!can} value={step(i)} onChange={(e) => onPick(e.target.value)}>
                  <option value="">{i === 0 ? t("Siapa pun berwenang") : "—"}</option>
                  {roles.map((r) => <option key={r.id} value={r.id}>{roleLabel(r.name)}</option>)}
                </select>
              );
              return (
                <tr key={et}>
                  <td className="d-strong">{TYPE[et]}</td>
                  <td>{pick(0, (v) => void save(p?.enabled ?? false, v, step(1)))}</td>
                  <td>{pick(1, (v) => void save(p?.enabled ?? false, step(0), v))}</td>
                  <td><label className="d-switch u-m-0"><input type="checkbox" aria-label={t("Aktif {0}", TYPE[et])} disabled={!can} checked={p?.enabled ?? false} onChange={(e) => void save(e.target.checked, step(0), step(1))} /></label></td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

function Delegate({ base, members, run }: { base: string; members: MemberRow[]; run: (f: () => Promise<unknown>, ok: string) => Promise<void> }) {
  const [to, setTo] = useState("");
  const [days, setDays] = useState(7);
  return (
    <Card title={t("Delegasikan wewenang saya")} description={t("Saat Anda cuti, anggota lain dapat memutuskan persetujuan atas nama Anda untuk sementara.")}>
      <div className="d-row u-items-end">
        <label className="d-field" style={{ flex: "2 1 220px", margin: 0 }}><span>{t("Delegasi ke")}</span>
          <select aria-label={t("Delegasi ke")} value={to} onChange={(e) => setTo(e.target.value)}>
            <option value="">{t("Pilih anggota…")}</option>
            {members.map((m) => <option key={m.user_id} value={m.user_id}>{m.email}</option>)}
          </select>
        </label>
        <label className="d-field" style={{ flex: "0 0 140px", margin: 0 }}><span>{t("Lama")}</span>
          <div className="d-input-suffix"><input aria-label={t("Hari")} type="number" min={1} max={90} value={days} onChange={(e) => setDays(Number(e.target.value))} /><em aria-hidden="true">hari</em></div>
        </label>
        <button type="button" className="d-btn" disabled={!to} onClick={() => run(() => api("POST", `${base}/approval-delegations`, { json: { to_user: to, hours: days * 24 } }), t("Delegasi dibuat."))}>{t("Delegasikan")}</button>
      </div>
    </Card>
  );
}
