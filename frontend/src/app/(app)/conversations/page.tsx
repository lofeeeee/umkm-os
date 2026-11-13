"use client";

// Percakapan (P6-3/P6-4): daftar + Conversation Viewer + aksi staf (balas, ambil alih, kembalikan ke AI,
// tutup, draft AI mode Assisted). Pembaruan realtime: SSE tenant hanya mengirim ping `conversation_id`;
// isi diambil ulang lewat API (masking sesuai permission).

import { useCallback, useEffect, useState, type FormEvent } from "react";

import { confirm } from "@/ui/dialog";

import { ChatCircleDots } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { ConversationView, when, type ConversationDetail, type ConversationSummary } from "@/inbox/conversation";
import { API_ORIGIN, ApiError, api, describe } from "@/lib/api";
import { CONVERSATION_STATUS, relatif, status } from "@/lib/labels";
import { BusyButton, EmptyState, PageHeader, PageSkeleton, Skeleton, Tabs } from "@/ui/page";

export default function ConversationsPage() {
  return <WithTenant>{(ctx) => <Conversations ctx={ctx} />}</WithTenant>;
}

interface Counts { ai: number; waiting_human: number; human: number; closed: number; mine: number }
type Tab = "waiting" | "mine" | "ai" | "human" | "closed" | "all";
const TABS: Record<Tab, { label: string; query: string; count?: (c: Counts) => number }> = {
  waiting: { label: "Menunggu staf", query: "&status=WAITING_HUMAN", count: (c) => c.waiting_human },
  mine: { label: "Ditangani saya", query: "&mine=true", count: (c) => c.mine },
  ai: { label: "Dijawab AI", query: "&status=AI", count: (c) => c.ai },
  human: { label: "Ditangani staf", query: "&status=HUMAN", count: (c) => c.human },
  closed: { label: "Selesai", query: "&status=CLOSED" },
  all: { label: "Semua", query: "" },
};

const REASON: Record<string, string> = {
  assigned_to_other: "Percakapan sedang ditangani staf lain.",
  conversation_closed: "Percakapan sudah ditutup.",
  draft_decided: "Draft sudah diputus.",
};

function errText(e: unknown): string {
  if (e instanceof ApiError) {
    const r = (e.details as { reason?: string } | null)?.reason;
    if (r && REASON[r]) return REASON[r];
  }
  return describe(e);
}

/** Ping SSE `chat.*` dari tenant → panggil `onPing(conversation_id)`. */
function useChatPings(tenantId: string, onPing: (conversationId: string) => void) {
  useEffect(() => {
    let es: EventSource | null = null;
    try {
      es = new EventSource(`${API_ORIGIN}/api/v1/tenants/${tenantId}/events`, { withCredentials: true });
      const h = (ev: Event) => {
        try {
          const d = JSON.parse((ev as MessageEvent).data) as { conversation_id?: string };
          if (d.conversation_id) onPing(d.conversation_id);
        } catch { /* abaikan */ }
      };
      for (const k of ["chat.message", "chat.status", "chat.draft"]) es.addEventListener(k, h);
    } catch {
      es = null;
    }
    return () => es?.close();
  }, [tenantId, onPing]);
}

function Conversations({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/conversations`;
  const [items, setItems] = useState<ConversationSummary[] | null>(null);
  const [masked, setMasked] = useState(false);
  const [tab, setTab] = useState<Tab>("waiting");
  const [counts, setCounts] = useState<Counts | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [open, setOpen] = useState<ConversationDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: ConversationSummary[]; masked: boolean; counts: Counts }>("GET", `${base}?limit=100${TABS[tab].query}`)
      .then((r) => { setItems(r.items); setMasked(r.masked); setCounts(r.counts); }, (e) => setErr(describe(e))),
    [base, tab],
  );
  const loadOne = useCallback(
    (id: string) => api<ConversationDetail>("GET", `${base}/${id}`).then(setOpen, (e) => setErr(describe(e))),
    [base],
  );
  useEffect(() => { void load(); }, [load]);
  useEffect(() => { if (openId) void loadOne(openId); }, [openId, loadOne]);
  const onPing = useCallback((id: string) => {
    if (id === openId) void loadOne(id);
    else if (!openId) void load();
  }, [openId, loadOne, load]);
  useChatPings(ctx.tenant.id, onPing);

  if (!ctx.can("conversations.read")) return <Alert kind="error">Anda tidak punya izin melihat percakapan.</Alert>;
  const back = () => { setOpenId(null); setOpen(null); void load(); };
  if (openId) {
    return (
      <>
        <PageHeader
          back={{ onClick: back, label: "Semua percakapan" }}
          title={open?.customer?.name ?? "Pengunjung"}
          description={open ? `${status(CONVERSATION_STATUS, open.status).label} · ${open.message_count} pesan · terakhir ${when(open.last_message_at)}` : undefined}
        />
        <Alert kind="error">{err}</Alert>
        {!open ? <PageSkeleton /> : (
          <ConversationView
            d={open}
            staff={ctx.can("conversations.reply") ? <StaffPanel ctx={ctx} d={open} url={`${base}/${open.id}`} onDone={() => loadOne(open.id)} /> : undefined}
          />
        )}
      </>
    );
  }
  return (
    <>
      <PageHeader title="Percakapan" description="Chat pelanggan dari website. Asisten AI menjawab otomatis; yang butuh manusia masuk ke antrean Menunggu staf." />
      <Alert kind="error">{err}</Alert>
      {masked && <Alert kind="info">Data sensitif disamarkan untuk peran Anda.</Alert>}
      <Tabs
        label="Antrean"
        value={tab}
        onChange={setTab}
        items={(Object.keys(TABS) as Tab[]).map((k) => ({ value: k, label: TABS[k].label, count: counts && TABS[k].count ? TABS[k].count(counts) : undefined }))}
      />
      {items === null ? <div className="d-card"><Skeleton lines={6} /></div> : items.length === 0 ? (
        <div className="d-card"><EmptyState icon={ChatCircleDots} title="Antrean kosong">{tab === "waiting" ? "Tidak ada pelanggan yang menunggu staf. Kerja bagus!" : "Belum ada percakapan di tab ini."}</EmptyState></div>
      ) : (
        <ul className="cv-list" aria-label="Daftar percakapan">
          {items.map((c) => {
            const st = status(CONVERSATION_STATUS, c.status);
            return (
              <li key={c.id}>
                <button className="cv-open" onClick={() => { setErr(null); setOpenId(c.id); }}>
                  <span className="d-cell-main">
                    <span className="d-avatar d-avatar-muted" aria-hidden="true">{(c.customer?.name?.trim()[0] ?? "?").toUpperCase()}</span>
                    <span style={{ minWidth: 0 }}>
                      <strong>{c.customer?.name ?? "Pengunjung"}</strong>
                      <span className="cv-preview">{c.last_message ?? "—"}</span>
                    </span>
                  </span>
                  <span className="cv-side">
                    <span className="d-muted d-small">{relatif(c.last_message_at)}</span>
                    <span className="cv-badge" data-status={c.status}>{st.label}</span>
                  </span>
                  <span className="d-muted">{c.message_count} pesan{c.assignee_email ? ` · ${c.assignee_email}` : ""}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}

function StaffPanel({ ctx, d, url, onDone }: { ctx: TenantCtx; d: ConversationDetail; url: string; onDone: () => Promise<void> }) {
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const [busy, setBusy] = useState(false);
  const [edit, setEdit] = useState<Record<string, string>>({});
  const mine = d.assignee_user_id === ctx.session.user_id;
  const closed = d.status === "CLOSED";
  const byOther = d.status === "HUMAN" && !mine;
  const canTake = ctx.can("conversations.takeover");

  async function run(fn: () => Promise<unknown>, ok: string) {
    setBusy(true);
    setMsg({});
    try {
      const r = await fn();
      const pending = (r as { approval_request_id?: string } | undefined)?.approval_request_id;
      setMsg({ ok: pending ? "Menunggu persetujuan sebelum dikirim." : ok });
      await onDone();
    } catch (e) {
      setMsg({ err: errText(e) });
    } finally {
      setBusy(false);
    }
  }

  function reply(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const body = String(new FormData(form).get("body") ?? "");
    void run(async () => { await api("POST", `${url}/messages`, { json: { body } }); form.reset(); }, "Balasan terkirim.");
  }

  const pending = d.drafts.filter((x) => x.status === "pending" || x.status === "approval_pending");
  return (
    <section className="d-card" aria-label="Tindakan staf">
      <header className="d-card-head">
        <div className="d-card-head-text"><h2>Tindakan staf</h2>{d.status !== "HUMAN" && !closed && <p>Membalas = mengambil alih; AI berhenti menjawab.</p>}</div>
        <div className="d-card-head-actions">
          {canTake && !closed && d.status !== "HUMAN" && (
            <button className="d-btn d-btn-sm" disabled={busy} onClick={() => void run(() => api("POST", `${url}/takeover`), "Percakapan Anda tangani.")}>Ambil alih</button>
          )}
          {canTake && !closed && d.status !== "AI" && !byOther && (
            <button className="d-btn d-btn-sm" disabled={busy} onClick={() => void run(() => api("POST", `${url}/release`), "Dikembalikan ke asisten AI.")}>Kembalikan ke AI</button>
          )}
          {!closed && !byOther && (
            <button className="d-btn d-btn-sm d-btn-danger" disabled={busy} onClick={async () => {
              if (await confirm({ title: "Tutup percakapan?", body: "Pelanggan perlu memulai chat baru untuk menghubungi toko lagi.", confirmLabel: "Tutup percakapan", danger: true })) void run(() => api("POST", `${url}/close`), "Percakapan ditutup.");
            }}>Tutup percakapan</button>
          )}
        </div>
      </header>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {byOther && <p className="d-muted">Sedang ditangani {d.assignee_email ?? "staf lain"}.</p>}
      {pending.length > 0 && !closed && (
        <div aria-label="Draft AI">
          <h3>Draft AI menunggu keputusan</h3>
          {pending.map((x) => (
            <div key={x.id} className="cv-draft">
              {x.status === "approval_pending" ? (
                <p>{x.body} <strong className="d-muted">— menunggu persetujuan</strong></p>
              ) : (
                <>
                  <label className="d-field"><span>Draft (boleh diedit)</span>
                    <textarea rows={3} maxLength={4000} value={edit[x.id] ?? x.body} onChange={(e) => setEdit({ ...edit, [x.id]: e.target.value })} />
                  </label>
                  <div className="d-row">
                    <button className="d-btn d-btn-primary" disabled={busy || byOther} onClick={() => void run(
                      () => api("POST", `${url}/drafts/${x.id}/send`, { json: { body: edit[x.id] ?? null } }), "Draft terkirim.")}>Kirim draft</button>
                    <button className="d-btn" disabled={busy} onClick={() => void run(
                      () => api("POST", `${url}/drafts/${x.id}/discard`), "Draft dibuang.")}>Buang</button>
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      )}
      {!closed && !byOther && (
        <form className="d-composer" onSubmit={reply} style={{ marginTop: 0 }}>
          <label className="sr-only" htmlFor="cv-reply">Balas pelanggan</label>
          <textarea id="cv-reply" name="body" rows={3} maxLength={4000} required placeholder="Tulis balasan untuk pelanggan… (Ctrl+Enter untuk kirim)"
            onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) e.currentTarget.form?.requestSubmit(); }} />
          <div className="d-row" style={{ justifyContent: "flex-end" }}>
            <BusyButton busy={busy}>Kirim balasan</BusyButton>
          </div>
        </form>
      )}
      {canTake && !closed && <AssignForm ctx={ctx} d={d} url={url} run={run} busy={busy} />}
      {ctx.can("tickets.reply") && (
        <details className="d-details cv-ticket" open>
        <summary>Buat tiket dari percakapan</summary>
        <form onSubmit={(e) => {
          e.preventDefault();
          const form = e.currentTarget;
          const f = new FormData(form);
          void run(async () => {
            await api("POST", `${url}/tickets`, { json: { subject: f.get("subject"), priority: f.get("priority"), description: f.get("description") || null } });
            form.reset();
          }, "Tiket dibuat.");
        }}>
          <div className="d-grid">
          <label className="d-field"><span>Subjek tiket</span><input name="subject" maxLength={200} required /></label>
          <label className="d-field"><span>Prioritas</span>
            <select name="priority" defaultValue="normal">
              <option value="low">Rendah</option><option value="normal">Normal</option><option value="high">Tinggi</option><option value="urgent">Mendesak</option>
            </select>
          </label>
          </div>
          <label className="d-field"><span>Keterangan <span className="d-opt">(opsional)</span></span><textarea name="description" rows={2} maxLength={5000} /></label>
          <button className="d-btn" disabled={busy}>Buat tiket</button>
        </form>
        </details>
      )}
    </section>
  );
}

interface MemberRow { user_id: string; email: string; status: string; role_name: string }

/** Tugaskan ke anggota lain (butuh daftar anggota; tanpa `members.read` form tidak tampil). */
function AssignForm({ ctx, d, url, run, busy }: {
  ctx: TenantCtx; d: ConversationDetail; url: string; busy: boolean;
  run: (fn: () => Promise<unknown>, ok: string) => Promise<void>;
}) {
  const [members, setMembers] = useState<MemberRow[] | null>(null);
  const [to, setTo] = useState("");
  useEffect(() => {
    if (!ctx.can("members.read")) return;
    api<MemberRow[]>("GET", `/api/v1/tenants/${ctx.tenant.id}/members`).then(
      (m) => setMembers(m.filter((x) => x.status === "active" && x.user_id !== d.assignee_user_id)),
      () => setMembers(null),
    );
  }, [ctx, d.assignee_user_id]);
  if (!members || members.length === 0) return null;
  return (
    <div className="d-row" style={{ marginTop: 16, alignItems: "flex-end" }}>
      <label className="d-field" style={{ margin: 0, flex: 1 }}><span>Tugaskan ke</span>
        <select value={to} onChange={(e) => setTo(e.target.value)}>
          <option value="">Pilih anggota…</option>
          {members.map((m) => <option key={m.user_id} value={m.user_id}>{m.email} ({m.role_name})</option>)}
        </select>
      </label>
      <button className="d-btn" disabled={busy || !to} onClick={() => void run(() => api("POST", `${url}/assign`, { json: { user_id: to } }), "Percakapan ditugaskan.")}>Tugaskan</button>
    </div>
  );
}
