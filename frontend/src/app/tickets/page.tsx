"use client";

// Tiket (P4-3): daftar & filter, detail, balasan publik/catatan internal, assign, status, prioritas.

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { API_ORIGIN, api, describe } from "@/lib/api";

interface Ticket {
  id: string; code: string; source: string; subject: string; description: string | null; priority: string; status: string;
  assignee_user_id: string | null; tags: string[]; first_response_due_at: string; resolution_due_at: string;
  sla_first_response_breached: boolean; sla_resolution_breached: boolean; created_at: string; order_id: string | null;
  messages: { id: string; visibility: string; author_type: string; body: string; created_at: string }[];
  attachments: { id: string; visibility: string }[];
  events: { kind: string; created_at: string }[];
}
interface MemberRow { user_id: string; email: string; status: string }

const STATUS: Record<string, string> = {
  OPEN: "Baru", ASSIGNED: "Ditugaskan", IN_PROGRESS: "Dikerjakan", WAITING_CUSTOMER: "Menunggu pelanggan",
  WAITING_INTERNAL: "Menunggu internal", RESOLVED: "Selesai", CLOSED: "Ditutup",
};
const NEXT: Record<string, string[]> = {
  OPEN: ["IN_PROGRESS", "RESOLVED", "CLOSED"], ASSIGNED: ["IN_PROGRESS", "WAITING_CUSTOMER", "WAITING_INTERNAL", "RESOLVED"],
  IN_PROGRESS: ["WAITING_CUSTOMER", "WAITING_INTERNAL", "RESOLVED"], WAITING_CUSTOMER: ["IN_PROGRESS", "RESOLVED", "CLOSED"],
  WAITING_INTERNAL: ["IN_PROGRESS", "RESOLVED"], RESOLVED: ["CLOSED", "IN_PROGRESS"], CLOSED: ["IN_PROGRESS"],
};

export default function TicketsPage() {
  return <WithTenant>{(ctx) => <Tickets ctx={ctx} />}</WithTenant>;
}

function Tickets({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/tickets`;
  const [items, setItems] = useState<Ticket[]>([]);
  const [status, setStatus] = useState("");
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [members, setMembers] = useState<MemberRow[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [subject, setSubject] = useState("");
  const load = useCallback(
    () => api<{ items: Ticket[] }>("GET", `${base}?limit=100${status ? `&status=${status}` : ""}${q ? `&q=${encodeURIComponent(q)}` : ""}`).then((r) => setItems(r.items), (e) => setErr(describe(e))),
    [base, status, q],
  );
  useEffect(() => {
    void load();
    api<MemberRow[]>("GET", `/api/v1/tenants/${ctx.tenant.id}/members`).then(setMembers, () => setMembers([]));
  }, [load, ctx.tenant.id]);
  if (open) return <Detail ctx={ctx} url={`${base}/${open}`} members={members} onBack={() => { setOpen(null); void load(); }} />;
  return (
    <>
      <h1>Tiket</h1>
      <Alert kind="error">{err}</Alert>
      <div className="d-row" style={{ marginBottom: 12 }}>
        <input aria-label="Cari" placeholder="Cari kode / subjek" value={q} onChange={(e) => setQ(e.target.value)} />
        <select aria-label="Status" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Semua status</option>
          {Object.entries(STATUS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        {ctx.can("tickets.reply") && (
          <>
            <input aria-label="Subjek tiket baru" placeholder="Subjek tiket baru" value={subject} maxLength={200} onChange={(e) => setSubject(e.target.value)} />
            <button className="d-btn" disabled={!subject.trim()} onClick={async () => {
              try { await api("POST", base, { json: { subject } }); setSubject(""); await load(); } catch (e) { setErr(describe(e)); }
            }}>Buat tiket</button>
          </>
        )}
      </div>
      <div className="d-card" style={{ overflowX: "auto" }}>
        <table className="d-table">
          <thead><tr><th>Kode</th><th>Subjek</th><th>Prioritas</th><th>Status</th><th>SLA</th><th /></tr></thead>
          <tbody>
            {items.map((t) => (
              <tr key={t.id}>
                <td>{t.code}<div className="d-muted">{t.source}</div></td>
                <td>{t.subject}</td>
                <td>{t.priority}</td>
                <td>{STATUS[t.status] ?? t.status}</td>
                <td>{t.sla_first_response_breached || t.sla_resolution_breached ? <strong>Terlewat</strong> : "Aman"}</td>
                <td><button className="d-btn d-btn-sm" onClick={() => setOpen(t.id)}>Buka</button></td>
              </tr>
            ))}
            {items.length === 0 && <tr><td colSpan={6} className="d-muted">Tidak ada tiket.</td></tr>}
          </tbody>
        </table>
      </div>
    </>
  );
}

function Detail({ ctx, url, members, onBack }: { ctx: TenantCtx; url: string; members: MemberRow[]; onBack: () => void }) {
  const [t, setT] = useState<Ticket | null>(null);
  const [body, setBody] = useState("");
  const [internal, setInternal] = useState(false);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const load = useCallback(() => api<Ticket>("GET", url).then(setT, (e) => setMsg({ err: describe(e) })), [url]);
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await f(); setMsg({ ok }); await load(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  if (!t) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  const email = (u: string | null) => members.find((m) => m.user_id === u)?.email ?? "—";
  return (
    <div className="d-card">
      <button className="d-btn d-btn-sm" onClick={onBack}>← Daftar tiket</button>
      <h2>{t.code} · {t.subject}</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      <p>
        {STATUS[t.status]} · prioritas {t.priority} · ditugaskan: {email(t.assignee_user_id)}<br />
        <span className="d-muted">Respons pertama s.d. {new Date(t.first_response_due_at).toLocaleString("id-ID")}{t.sla_first_response_breached && " (terlewat)"} · selesai s.d. {new Date(t.resolution_due_at).toLocaleString("id-ID")}{t.sla_resolution_breached && " (terlewat)"}</span>
      </p>
      {t.description && <p>{t.description}</p>}
      <div className="d-row">
        {ctx.can("tickets.assign") && (
          <select aria-label="Tugaskan ke" value={t.assignee_user_id ?? ""} onChange={(e) => run(() => api("POST", `${url}/assign`, { json: { assignee_user_id: e.target.value || null } }), "Penugasan diperbarui.")}>
            <option value="">Belum ditugaskan</option>
            {members.filter((m) => m.status === "active").map((m) => <option key={m.user_id} value={m.user_id}>{m.email}</option>)}
          </select>
        )}
        {ctx.can("tickets.reply") && (
          <select aria-label="Prioritas" value={t.priority} onChange={(e) => run(() => api("PATCH", url, { json: { priority: e.target.value } }), "Prioritas diperbarui.")}>
            {["low", "normal", "high", "urgent"].map((p) => <option key={p} value={p}>{p}</option>)}
          </select>
        )}
        {(NEXT[t.status] ?? []).filter((s) => ctx.can(s === "RESOLVED" || s === "CLOSED" ? "tickets.close" : "tickets.reply")).map((s) => (
          <button key={s} className="d-btn d-btn-sm" onClick={() => run(() => api("POST", `${url}/status`, { json: { status: s } }), "Status diperbarui.")}>{STATUS[s]}</button>
        ))}
      </div>
      <h3>Percakapan</h3>
      <ul className="d-thread">
        {t.messages.map((m) => (
          <li key={m.id} className={m.visibility === "internal" ? "d-internal" : ""}>
            <strong>{m.author_type === "customer" ? "Pelanggan" : m.visibility === "internal" ? "Catatan internal" : "Staf"}</strong>
            <span className="d-muted"> {new Date(m.created_at).toLocaleString("id-ID")}</span>
            <p>{m.body}</p>
          </li>
        ))}
      </ul>
      {t.attachments.length > 0 && (
        <p>Lampiran: {t.attachments.map((a, i) => (
          <a key={a.id} href={`${API_ORIGIN}${url}/attachments/${a.id}`} target="_blank" rel="noopener">Gambar {i + 1}{a.visibility === "internal" ? " (internal)" : ""} </a>
        ))}</p>
      )}
      {ctx.can("tickets.reply") && (
        <div>
          <textarea aria-label="Balasan" rows={3} maxLength={5000} value={body} onChange={(e) => setBody(e.target.value)} style={{ width: "100%" }} />
          <div className="d-row">
            <label className="d-check"><input type="checkbox" checked={internal} onChange={(e) => setInternal(e.target.checked)} /> Catatan internal (tidak terlihat pelanggan)</label>
            <button className="d-btn d-btn-primary" disabled={!body.trim()} onClick={() => run(async () => { await api("POST", `${url}/messages`, { json: { body, internal } }); setBody(""); }, "Terkirim.")}>Kirim</button>
          </div>
        </div>
      )}
    </div>
  );
}
