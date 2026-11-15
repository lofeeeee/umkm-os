"use client";

// Tiket (P4-3): daftar & filter, detail, balasan publik/catatan internal, assign, status, prioritas.

import { useCallback, useEffect, useState } from "react";

import { Headset, LockSimple, Paperclip, Plus, Warning } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { API_ORIGIN, api, describe } from "@/lib/api";
import { TICKET_PRIORITY, TICKET_STATUS, relatif, status as statusOf, waktu } from "@/lib/labels";
import { Badge, BusyButton, Card, EmptyState, PageHeader, PageSkeleton, SearchInput, Skeleton, Tabs } from "@/ui/page";
import { toast } from "@/ui/toast";

interface Ticket {
  id: string; code: string; source: string; subject: string; description: string | null; priority: string; status: string;
  assignee_user_id: string | null; tags: string[]; first_response_due_at: string; resolution_due_at: string;
  sla_first_response_breached: boolean; sla_resolution_breached: boolean; created_at: string; order_id: string | null;
  messages: { id: string; visibility: string; author_type: string; body: string; created_at: string }[];
  attachments: { id: string; visibility: string }[];
  events: { kind: string; created_at: string }[];
}
interface MemberRow { user_id: string; email: string; status: string }

const NEXT: Record<string, string[]> = {
  OPEN: ["IN_PROGRESS", "RESOLVED", "CLOSED"], ASSIGNED: ["IN_PROGRESS", "WAITING_CUSTOMER", "WAITING_INTERNAL", "RESOLVED"],
  IN_PROGRESS: ["WAITING_CUSTOMER", "WAITING_INTERNAL", "RESOLVED"], WAITING_CUSTOMER: ["IN_PROGRESS", "RESOLVED", "CLOSED"],
  WAITING_INTERNAL: ["IN_PROGRESS", "RESOLVED"], RESOLVED: ["CLOSED", "IN_PROGRESS"], CLOSED: ["IN_PROGRESS"],
};
const SOURCE: Record<string, string> = { web: "Website", chat: "Percakapan", staff: "Dibuat staf", order: "Pesanan", email: "Email" };
const TABS = [
  { value: "", label: "Semua" },
  { value: "OPEN", label: "Baru" },
  { value: "IN_PROGRESS", label: "Dikerjakan" },
  { value: "WAITING_CUSTOMER", label: "Menunggu pelanggan" },
  { value: "RESOLVED", label: "Selesai" },
  { value: "CLOSED", label: "Ditutup" },
];

export default function TicketsPage() {
  return <WithTenant>{(ctx) => <Tickets ctx={ctx} />}</WithTenant>;
}

function Tickets({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/tickets`;
  const [items, setItems] = useState<Ticket[] | null>(null);
  const [tab, setTab] = useState("");
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [members, setMembers] = useState<MemberRow[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [subject, setSubject] = useState("");
  const [busy, setBusy] = useState(false);
  const load = useCallback(
    () => api<{ items: Ticket[] }>("GET", `${base}?limit=100${tab ? `&status=${tab}` : ""}${q ? `&q=${encodeURIComponent(q)}` : ""}`).then((r) => { setItems(r.items); setErr(null); }, (e) => setErr(describe(e))),
    [base, tab, q],
  );
  useEffect(() => {
    const t = setTimeout(() => void load(), q ? 250 : 0);
    return () => clearTimeout(t);
  }, [load, q]);
  useEffect(() => {
    api<MemberRow[]>("GET", `/api/v1/tenants/${ctx.tenant.id}/members`).then(setMembers, () => setMembers([]));
  }, [ctx.tenant.id]);
  if (open) return <Detail ctx={ctx} url={`${base}/${open}`} members={members} onBack={() => { setOpen(null); void load(); }} />;
  async function create() {
    setBusy(true);
    try { await api("POST", base, { json: { subject } }); setSubject(""); await load(); toast.ok("Tiket dibuat."); } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  return (
    <>
      <PageHeader title="Tiket" description="Masalah yang perlu ditindaklanjuti: keluhan pesanan, permintaan khusus, atau eskalasi dari percakapan." />
      <Alert kind="error">{err}</Alert>
      {ctx.can("tickets.reply") && (
        <form className="d-card d-row" style={{ padding: 12, flexWrap: "nowrap" }} onSubmit={(e) => { e.preventDefault(); if (subject.trim()) void create(); }}>
          <input aria-label="Subjek tiket baru" placeholder="Tulis subjek tiket baru, mis. “Pesanan belum sampai”" value={subject} maxLength={200} onChange={(e) => setSubject(e.target.value)} />
          <BusyButton busy={busy} disabled={!subject.trim()}><Plus weight="bold" aria-hidden="true" />Buat tiket</BusyButton>
        </form>
      )}
      <Tabs label="Filter status tiket" value={tab} onChange={setTab} items={TABS} />
      <div className="d-toolbar">
        <SearchInput label="Cari" placeholder="Cari kode atau subjek…" value={q} onChange={setQ} />
      </div>
      <div className="d-table-wrap">
        {items === null ? <div style={{ padding: 20 }}><Skeleton lines={5} /></div> : items.length === 0 ? (
          <EmptyState icon={Headset} title={tab || q ? "Tidak ada tiket yang cocok" : "Belum ada tiket"}
            action={tab || q ? undefined : <button type="button" className="d-btn d-btn-primary" onClick={() => document.querySelector<HTMLElement>('[aria-label="Subjek tiket baru"]')?.focus()}>Tulis subjek tiket pertama</button>}>
            {tab || q ? "Coba tab atau kata kunci lain." : "Tiket dibuat saat pelanggan butuh bantuan, atau saat staf mengeskalasi percakapan."}
          </EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>Tiket</th><th>Subjek</th><th>Prioritas</th><th>Status</th><th className="d-hide-sm">Tenggat</th><th><span className="sr-only">Aksi</span></th></tr></thead>
            <tbody>
              {items.map((t) => {
                const st = statusOf(TICKET_STATUS, t.status);
                const pr = statusOf(TICKET_PRIORITY, t.priority);
                const late = t.sla_first_response_breached || t.sla_resolution_breached;
                return (
                  <tr key={t.id}>
                    <td className="d-nowrap"><span className="d-strong">{t.code}</span><span className="d-sub">{SOURCE[t.source] ?? t.source} · {relatif(t.created_at)}</span></td>
                    <td>{t.subject}</td>
                    <td><Badge tone={pr.tone} plain>{pr.label}</Badge></td>
                    <td><Badge tone={st.tone}>{st.label}</Badge></td>
                    <td className="d-hide-sm">{late ? <Badge tone="danger" plain>Terlewat</Badge> : <span className="d-muted">Aman</span>}</td>
                    <td className="d-actions"><button className="d-btn d-btn-sm" onClick={() => setOpen(t.id)}>Buka</button></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}

function Detail({ ctx, url, members, onBack }: { ctx: TenantCtx; url: string; members: MemberRow[]; onBack: () => void }) {
  const [t, setT] = useState<Ticket | null>(null);
  const [body, setBody] = useState("");
  const [internal, setInternal] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(() => api<Ticket>("GET", url).then(setT, (e) => setErr(describe(e))), [url]);
  useEffect(() => { void load(); }, [load]);
  async function run(key: string, f: () => Promise<unknown>, ok: string) {
    setErr(null);
    setBusy(key);
    try { await f(); toast.ok(ok); await load(); } catch (e) { setErr(describe(e)); } finally { setBusy(null); }
  }
  if (!t) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const email = (u: string | null) => members.find((m) => m.user_id === u)?.email ?? "Belum ditugaskan";
  const st = statusOf(TICKET_STATUS, t.status);
  const pr = statusOf(TICKET_PRIORITY, t.priority);
  const next = (NEXT[t.status] ?? []).filter((s) => ctx.can(s === "RESOLVED" || s === "CLOSED" ? "tickets.close" : "tickets.reply"));
  return (
    <>
      <PageHeader
        back={{ onClick: onBack, label: "Semua tiket" }}
        title={<>{t.code} · {t.subject}</>}
        description={`${st.label} · prioritas ${pr.label} · ditugaskan: ${email(t.assignee_user_id)}`}
        actions={next.map((s, i) => (
          <BusyButton key={s} type="button" busy={busy === s} className={i === 0 ? "d-btn d-btn-primary" : "d-btn"}
            onClick={() => run(s, () => api("POST", `${url}/status`, { json: { status: s } }), "Status diperbarui.")}>
            {statusOf(TICKET_STATUS, s).label}
          </BusyButton>
        ))}
      />
      <Alert kind="error">{err}</Alert>
      <div className="d-split">
        <Card title="Percakapan" label="Percakapan tiket">
          {t.description && <p style={{ whiteSpace: "pre-wrap" }}>{t.description}</p>}
          {t.messages.length === 0 ? <p className="d-muted">Belum ada balasan.</p> : (
            <ul className="d-thread">
              {t.messages.map((m) => (
                <li key={m.id} className={m.visibility === "internal" ? "d-internal" : m.author_type === "customer" ? "" : "d-me"}>
                  <span className="d-muted">
                    <strong style={{ color: "var(--ink)" }}>{m.author_type === "customer" ? "Pelanggan" : m.visibility === "internal" ? "Catatan internal" : "Staf"}</strong> · {waktu(m.created_at)}
                  </span>
                  <span style={{ whiteSpace: "pre-wrap" }}>{m.body}</span>
                </li>
              ))}
            </ul>
          )}
          {t.attachments.length > 0 && (
            <p className="d-row">
              <Paperclip weight="bold" aria-hidden="true" />
              {t.attachments.map((a, i) => (
                <a key={a.id} href={`${API_ORIGIN}${url}/attachments/${a.id}`} target="_blank" rel="noopener">Gambar {i + 1}{a.visibility === "internal" ? " (internal)" : ""}</a>
              ))}
            </p>
          )}
          {ctx.can("tickets.reply") && (
            <form className={`d-composer${internal ? " d-composer-internal" : ""}`} onSubmit={(e) => {
              e.preventDefault();
              void run("send", async () => { await api("POST", `${url}/messages`, { json: { body, internal } }); setBody(""); }, internal ? "Catatan tersimpan." : "Balasan terkirim.");
            }}>
              <textarea aria-label="Balasan" rows={3} maxLength={5000} value={body}
                placeholder={internal ? "Catatan untuk tim (tidak terlihat pelanggan)…" : "Tulis balasan untuk pelanggan…"}
                onChange={(e) => setBody(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && body.trim()) e.currentTarget.form?.requestSubmit(); }} />
              <div className="d-row d-row-between">
                <label className="d-check" style={{ margin: 0 }}>
                  <input type="checkbox" checked={internal} onChange={(e) => setInternal(e.target.checked)} />
                  <span><LockSimple weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> Catatan internal (tidak terlihat pelanggan)</span>
                </label>
                <BusyButton busy={busy === "send"} disabled={!body.trim()}>Kirim</BusyButton>
              </div>
            </form>
          )}
        </Card>
        <div className="d-stack" style={{ alignContent: "start" }}>
          <Card title="Detail">
            {ctx.can("tickets.assign") && (
              <label className="d-field"><span>Ditugaskan ke</span>
                <select aria-label="Tugaskan ke" value={t.assignee_user_id ?? ""} onChange={(e) => run("assign", () => api("POST", `${url}/assign`, { json: { assignee_user_id: e.target.value || null } }), "Penugasan diperbarui.")}>
                  <option value="">Belum ditugaskan</option>
                  {members.filter((m) => m.status === "active").map((m) => <option key={m.user_id} value={m.user_id}>{m.email}</option>)}
                </select>
              </label>
            )}
            {ctx.can("tickets.reply") && (
              <label className="d-field"><span>Prioritas</span>
                <select aria-label="Prioritas" value={t.priority} onChange={(e) => run("prio", () => api("PATCH", url, { json: { priority: e.target.value } }), "Prioritas diperbarui.")}>
                  {["low", "normal", "high", "urgent"].map((p) => <option key={p} value={p}>{statusOf(TICKET_PRIORITY, p).label}</option>)}
                </select>
              </label>
            )}
            <dl className="d-dl">
              <dt>Sumber</dt><dd>{SOURCE[t.source] ?? t.source}</dd>
              <dt>Dibuat</dt><dd>{waktu(t.created_at)}</dd>
              <dt>Respons pertama</dt><dd>{waktu(t.first_response_due_at)}{t.sla_first_response_breached && <> <Badge tone="danger" plain>Terlewat</Badge></>}</dd>
              <dt>Target selesai</dt><dd>{waktu(t.resolution_due_at)}{t.sla_resolution_breached && <> <Badge tone="danger" plain>Terlewat</Badge></>}</dd>
            </dl>
            {(t.sla_first_response_breached || t.sla_resolution_breached) && (
              <p className="d-small" style={{ color: "var(--danger)", marginTop: 12 }}><Warning weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> Tenggat layanan terlewat — prioritaskan tiket ini.</p>
            )}
          </Card>
        </div>
      </div>
    </>
  );
}
