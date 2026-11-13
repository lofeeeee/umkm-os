"use client";

// Tim & izin (P4-1): anggota (peran/status), peran kustom, undangan. Server menolak
// eskalasi hak; UI hanya menampilkan pilihan dan pesan penolakannya.

import { useCallback, useEffect, useState } from "react";

import { EnvelopeSimple, Plus, ShieldCheck, Trash } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe, roleLabel } from "@/lib/api";
import { MEMBER_STATUS, PERMISSION_GROUPS, PERMISSION_TEXT, status, tanggal } from "@/lib/labels";
import { confirm, Sheet } from "@/ui/dialog";
import { RowMenu } from "@/ui/menu";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";

interface MemberRow { membership_id: string; user_id: string; email: string; role_id: string; role_name: string; status: string }
interface Role { id: string; name: string; is_system: boolean; permissions: string[]; members: number }
interface Invitation { id: string; email: string; role_name: string; expires_at: string }

export default function TeamPage() {
  return <WithTenant>{(ctx) => <Team ctx={ctx} />}</WithTenant>;
}

function Team({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [members, setMembers] = useState<MemberRow[] | null>(null);
  const [roles, setRoles] = useState<Role[]>([]);
  const [invites, setInvites] = useState<Invitation[]>([]);
  const [perms, setPerms] = useState<string[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [inv, setInv] = useState({ email: "", role_id: "" });
  const [busy, setBusy] = useState(false);
  const [role, setRole] = useState<{ id?: string; name: string; permissions: string[] } | null>(null);
  const load = useCallback(() => Promise.all([
    api<MemberRow[]>("GET", `${base}/members`).then(setMembers),
    api<Role[]>("GET", `${base}/roles`).then(setRoles),
    api<Invitation[]>("GET", `${base}/invitations`).then(setInvites).catch(() => setInvites([])),
    api<string[]>("GET", "/api/v1/permissions").then(setPerms),
  ]).catch((e) => setErr(describe(e))), [base]);
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setErr(null);
    try { await f(); toast.ok(ok); await load(); return true; } catch (e) { setErr(describe(e)); return false; }
  }
  const manage = ctx.can("members.manage");
  if (!members) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  return (
    <>
      <PageHeader
        title="Tim"
        description="Undang staf dan atur apa saja yang boleh mereka lakukan lewat peran."
        actions={manage && <button className="d-btn" onClick={() => setRole({ name: "", permissions: [] })}><Plus weight="bold" aria-hidden="true" />Peran kustom baru</button>}
      />
      <Alert kind="error">{err}</Alert>

      {ctx.can("members.invite") && (
        <Card title="Undang anggota" description="Tautan undangan berlaku 7 hari dan hanya bisa dipakai sekali.">
          <form className="d-row" style={{ alignItems: "flex-end" }} onSubmit={(e) => {
            e.preventDefault();
            setBusy(true);
            void run(async () => { await api("POST", `${base}/invitations`, { json: inv }); setInv({ email: "", role_id: "" }); }, "Undangan dibuat.").finally(() => setBusy(false));
          }}>
            <label className="d-field" style={{ flex: "2 1 240px", margin: 0 }}><span>Email</span>
              <input type="email" placeholder="nama@contoh.id" value={inv.email} onChange={(e) => setInv({ ...inv, email: e.target.value })} />
            </label>
            <label className="d-field" style={{ flex: "1 1 180px", margin: 0 }}><span>Peran</span>
              <select value={inv.role_id} onChange={(e) => setInv({ ...inv, role_id: e.target.value })}>
                <option value="">Pilih peran…</option>
                {roles.filter((r) => r.name !== "Owner" || ctx.can("members.manage")).map((r) => <option key={r.id} value={r.id}>{roleLabel(r.name)}</option>)}
              </select>
            </label>
            <BusyButton busy={busy} disabled={!inv.email || !inv.role_id}><EnvelopeSimple weight="bold" aria-hidden="true" />Undang</BusyButton>
          </form>
          {invites.length > 0 && (
            <>
              <h3 style={{ marginTop: 20 }}>Menunggu diterima</h3>
              <ul className="d-list">
                {invites.map((i) => (
                  <li key={i.id}>
                    <div className="d-list-main"><strong>{i.email}</strong><span className="d-muted">{roleLabel(i.role_name)} · berlaku s.d. {tanggal(i.expires_at)}</span></div>
                    <button className="d-btn d-btn-sm d-btn-ghost" onClick={() => void run(() => api("DELETE", `${base}/invitations/${i.id}`), "Undangan dicabut.")}>Cabut</button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </Card>
      )}

      <Card flush title={`Anggota (${members.length})`}>
        <div className="d-table-wrap">
          <table className="d-table d-table-stack">
            <thead><tr><th>Anggota</th><th>Peran</th><th>Status</th><th><span className="sr-only">Aksi</span></th></tr></thead>
            <tbody>
              {members.map((m) => {
                const me = m.user_id === ctx.session.user_id;
                const st = status(MEMBER_STATUS, m.status);
                return (
                  <tr key={m.membership_id}>
                    <td>
                      <div className="d-cell-main">
                        <span className="d-avatar d-avatar-muted" aria-hidden="true">{m.email[0]?.toUpperCase()}</span>
                        <div><span className="d-strong">{m.email}</span>{me && <span className="d-sub">Anda</span>}</div>
                      </div>
                    </td>
                    <td>
                      <select aria-label={`Peran ${m.email}`} disabled={!manage || me} value={m.role_id} style={{ maxWidth: 200 }}
                        onChange={(e) => void run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { role_id: e.target.value } }), "Peran diperbarui.")}>
                        {roles.map((r) => <option key={r.id} value={r.id}>{roleLabel(r.name)}</option>)}
                      </select>
                    </td>
                    <td><Badge tone={st.tone}>{st.label}</Badge></td>
                    <td className="d-actions">
                      {manage && !me && (
                        <RowMenu label={`Aksi untuk ${m.email}`} items={[
                          {
                            label: m.status === "active" ? "Nonaktifkan" : "Aktifkan",
                            run: () => void run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { status: m.status === "active" ? "suspended" : "active" } }), "Status diperbarui."),
                          },
                          {
                            label: "Keluarkan dari tim", danger: true,
                            run: async () => {
                              if (await confirm({ title: `Keluarkan ${m.email}?`, body: "Akses orang ini ke toko langsung dicabut. Anda bisa mengundangnya lagi nanti.", confirmLabel: "Keluarkan", danger: true })) {
                                void run(() => api("PATCH", `${base}/members/${m.membership_id}`, { json: { status: "removed" } }), "Anggota dikeluarkan.");
                              }
                            },
                          },
                        ]} />
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Card>

      <Card title="Peran" description="Peran bawaan tidak bisa diubah. Buat peran kustom untuk kebutuhan khusus, mis. Kasir.">
        <ul className="d-list">
          {roles.map((r) => (
            <li key={r.id}>
              <div className="d-list-main">
                <strong>{roleLabel(r.name)} {r.is_system && <Badge plain>Bawaan</Badge>}</strong>
                <span className="d-muted">{r.permissions.length} izin · {r.members} anggota</span>
              </div>
              {manage && !r.is_system && (
                <div className="d-row">
                  <button className="d-btn d-btn-sm" onClick={() => setRole({ id: r.id, name: r.name, permissions: r.permissions })}>Ubah</button>
                  <button className="d-icon-btn" aria-label={`Hapus peran ${r.name}`} onClick={async () => {
                    if (await confirm({ title: `Hapus peran “${r.name}”?`, body: r.members > 0 ? `${r.members} anggota masih memakai peran ini.` : undefined, confirmLabel: "Hapus", danger: true })) void run(() => api("DELETE", `${base}/roles/${r.id}`), "Peran dihapus.");
                  }}><Trash weight="bold" /></button>
                </div>
              )}
            </li>
          ))}
        </ul>
      </Card>

      <Sheet
        open={!!role}
        onClose={() => setRole(null)}
        title={role?.id ? "Ubah peran" : "Peran kustom baru"}
        description="Izin yang tidak Anda miliki tidak dapat diberikan."
        footer={role && (
          <>
            <span className="d-muted">{role.permissions.length} izin dipilih</span>
            <button className="d-btn" onClick={() => setRole(null)}>Batal</button>
            <BusyButton type="button" busy={busy} disabled={!role.name.trim()} onClick={async () => {
              setBusy(true);
              const ok = await run(() => api(role.id ? "PUT" : "POST", role.id ? `${base}/roles/${role.id}` : `${base}/roles`, { json: { name: role.name, permissions: role.permissions } }), "Peran tersimpan.");
              setBusy(false);
              if (ok) setRole(null);
            }}>Simpan peran</BusyButton>
          </>
        )}
      >
        {role && (
          <>
            <Alert kind="error">{err}</Alert>
            <label className="d-field"><span>Nama peran</span><input value={role.name} maxLength={60} placeholder="mis. Kasir" onChange={(e) => setRole({ ...role, name: e.target.value })} /></label>
            {PERMISSION_GROUPS.map((g) => {
              const items = perms.filter((p) => g.prefix.includes(p.split(".")[0] ?? ""));
              if (items.length === 0) return null;
              return (
                <fieldset key={g.label} className="d-permset">
                  <legend><ShieldCheck weight="bold" aria-hidden="true" /> {g.label}</legend>
                  {items.map((p) => (
                    <label key={p} className="d-check">
                      <input type="checkbox" checked={role.permissions.includes(p)} disabled={!ctx.can(p)}
                        onChange={(e) => setRole({ ...role, permissions: e.target.checked ? [...role.permissions, p] : role.permissions.filter((x) => x !== p) })} />
                      <span>{PERMISSION_TEXT[p] ?? p} <code className="d-muted" style={{ background: "none", padding: 0 }}>{p}</code></span>
                    </label>
                  ))}
                </fieldset>
              );
            })}
          </>
        )}
      </Sheet>
    </>
  );
}
