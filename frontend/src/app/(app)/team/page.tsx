"use client";

// Tim & permission (P4-1): anggota (role/status), role kustom, undangan. Server menolak
// eskalasi hak; UI hanya menampilkan pilihan dan pesan penolakannya.

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe, roleLabel } from "@/lib/api";

interface MemberRow { membership_id: string; user_id: string; email: string; role_id: string; role_name: string; status: string }
interface Role { id: string; name: string; is_system: boolean; permissions: string[]; members: number }
interface Invitation { id: string; email: string; role_name: string; expires_at: string }

export default function TeamPage() {
  return <WithTenant>{(ctx) => <Team ctx={ctx} />}</WithTenant>;
}

function Team({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [members, setMembers] = useState<MemberRow[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [invites, setInvites] = useState<Invitation[]>([]);
  const [perms, setPerms] = useState<string[]>([]);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const [inv, setInv] = useState({ email: "", role_id: "" });
  const [role, setRole] = useState<{ id?: string; name: string; permissions: string[] } | null>(null);
  const load = useCallback(() => Promise.all([
    api<MemberRow[]>("GET", `${base}/members`).then(setMembers),
    api<Role[]>("GET", `${base}/roles`).then(setRoles),
    api<Invitation[]>("GET", `${base}/invitations`).then(setInvites).catch(() => setInvites([])),
    api<string[]>("GET", "/api/v1/permissions").then(setPerms),
  ]).catch((e) => setMsg({ err: describe(e) })), [base]);
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await f(); setMsg({ ok }); await load(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  const manage = ctx.can("members.manage");
  return (
    <>
      <h1>Tim</h1>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <div className="d-card" style={{ overflowX: "auto" }}>
        <h2>Anggota</h2>
        <table className="d-table">
          <thead><tr><th>Email</th><th>Peran</th><th>Status</th><th /></tr></thead>
          <tbody>
            {members.map((m) => (
              <tr key={m.membership_id}>
                <td>{m.email}{m.user_id === ctx.session.user_id && <span className="d-muted"> (Anda)</span>}</td>
                <td>
                  <select aria-label={`Peran ${m.email}`} disabled={!manage || m.user_id === ctx.session.user_id} value={m.role_id}
                    onChange={(e) => run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { role_id: e.target.value } }), "Peran diperbarui.")}>
                    {roles.map((r) => <option key={r.id} value={r.id}>{roleLabel(r.name)}</option>)}
                  </select>
                </td>
                <td>{m.status === "active" ? "Aktif" : "Nonaktif"}</td>
                <td className="d-row">
                  {manage && m.user_id !== ctx.session.user_id && (
                    <>
                      <button className="d-btn d-btn-sm" onClick={() => run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { status: m.status === "active" ? "suspended" : "active" } }), "Status diperbarui.")}>
                        {m.status === "active" ? "Nonaktifkan" : "Aktifkan"}
                      </button>
                      <button className="d-btn d-btn-sm d-btn-danger" onClick={() => window.confirm(`Keluarkan ${m.email}?`) && run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { status: "removed" } }), "Anggota dikeluarkan.")}>Keluarkan</button>
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {ctx.can("members.invite") && (
        <div className="d-card">
          <h2>Undang anggota</h2>
          <div className="d-row">
            <input aria-label="Email" type="email" placeholder="email@contoh.id" value={inv.email} onChange={(e) => setInv({ ...inv, email: e.target.value })} />
            <select aria-label="Role" value={inv.role_id} onChange={(e) => setInv({ ...inv, role_id: e.target.value })}>
              <option value="">Pilih role</option>
              {roles.filter((r) => r.name !== "Owner" || ctx.can("members.manage")).map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
            </select>
            <button className="d-btn d-btn-primary" disabled={!inv.email || !inv.role_id} onClick={() => run(async () => { await api("POST", `${base}/invitations`, { json: inv }); setInv({ email: "", role_id: "" }); }, "Undangan dibuat.")}>Undang</button>
          </div>
          <p className="d-muted">Email undangan berlaku 7 hari dan hanya bisa dipakai sekali.</p>
          <ul>
            {invites.map((i) => (
              <li key={i.id}>{i.email} — {i.role_name} (s.d. {new Date(i.expires_at).toLocaleDateString("id-ID")}){" "}
                <button className="d-btn d-btn-sm" onClick={() => run(() => api("DELETE", `${base}/invitations/${i.id}`), "Undangan dicabut.")}>Cabut</button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="d-card">
        <h2>Role</h2>
        <ul>
          {roles.map((r) => (
            <li key={r.id}>
              <strong>{r.name}</strong>{r.is_system ? " (bawaan)" : ""} · {r.permissions.length} izin · {r.members} anggota{" "}
              {manage && !r.is_system && (
                <>
                  <button className="d-btn d-btn-sm" onClick={() => setRole({ id: r.id, name: r.name, permissions: r.permissions })}>Ubah</button>{" "}
                  <button className="d-btn d-btn-sm d-btn-danger" onClick={() => run(() => api("DELETE", `${base}/roles/${r.id}`), "Role dihapus.")}>Hapus</button>
                </>
              )}
            </li>
          ))}
        </ul>
        {manage && !role && <button className="d-btn" onClick={() => setRole({ name: "", permissions: [] })}>Role kustom baru</button>}
        {role && (
          <div className="d-card">
            <label className="d-field"><span>Nama role</span><input value={role.name} maxLength={60} onChange={(e) => setRole({ ...role, name: e.target.value })} /></label>
            <div className="d-perms">
              {perms.map((p) => (
                <label key={p} className="d-check">
                  <input type="checkbox" checked={role.permissions.includes(p)} disabled={!ctx.can(p)}
                    onChange={(e) => setRole({ ...role, permissions: e.target.checked ? [...role.permissions, p] : role.permissions.filter((x) => x !== p) })} />
                  {p}
                </label>
              ))}
            </div>
            <p className="d-muted">Izin yang tidak Anda miliki tidak dapat diberikan.</p>
            <div className="d-row">
              <button className="d-btn d-btn-primary" onClick={() => run(async () => {
                await api(role.id ? "PUT" : "POST", role.id ? `${base}/roles/${role.id}` : `${base}/roles`, { json: { name: role.name, permissions: role.permissions } });
                setRole(null);
              }, "Role tersimpan.")}>Simpan role</button>
              <button className="d-btn" onClick={() => setRole(null)}>Batal</button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
