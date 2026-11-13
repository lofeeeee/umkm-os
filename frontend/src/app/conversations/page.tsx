"use client";

// Percakapan (P6-3/P6-4): daftar + Conversation Viewer + aksi staf (balas, ambil alih, kembalikan ke AI,
// tutup, draft AI mode Assisted). Pembaruan realtime: SSE tenant hanya mengirim ping `conversation_id`;
// isi diambil ulang lewat API (masking sesuai permission).

import { useCallback, useEffect, useState, type FormEvent } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { CONV_STATUS, ConversationView, when, type ConversationDetail, type ConversationSummary } from "@/inbox/conversation";
import { API_ORIGIN, ApiError, api, describe } from "@/lib/api";

export default function ConversationsPage() {
  return <WithTenant>{(ctx) => <Conversations ctx={ctx} />}</WithTenant>;
}

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
  const [items, setItems] = useState<ConversationSummary[]>([]);
  const [masked, setMasked] = useState(false);
  const [status, setStatus] = useState("");
  const [openId, setOpenId] = useState<string | null>(null);
  const [open, setOpen] = useState<ConversationDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: ConversationSummary[]; masked: boolean }>("GET", `${base}?limit=100${status ? `&status=${status}` : ""}`)
      .then((r) => { setItems(r.items); setMasked(r.masked); }, (e) => setErr(describe(e))),
    [base, status],
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
  if (openId) {
    return (
      <>
        <button className="d-btn d-btn-sm" onClick={() => { setOpenId(null); setOpen(null); void load(); }}>← Kembali</button>
        <h1>Percakapan</h1>
        <Alert kind="error">{err}</Alert>
        {open && (
          <>
            {ctx.can("conversations.reply") && (
              <StaffPanel ctx={ctx} d={open} url={`${base}/${open.id}`} onDone={() => loadOne(open.id)} />
            )}
            <ConversationView d={open} />
          </>
        )}
      </>
    );
  }
  return (
    <>
      <h1>Percakapan</h1>
      <Alert kind="error">{err}</Alert>
      {masked && <p className="d-muted">Data sensitif disamarkan untuk peran Anda.</p>}
      <div className="d-row" style={{ marginBottom: 12 }}>
        <select aria-label="Status" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Semua status</option>
          {Object.entries(CONV_STATUS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
      </div>
      {items.length === 0 ? <p className="d-muted">Belum ada percakapan.</p> : (
        <ul className="cv-list" aria-label="Daftar percakapan">
          {items.map((c) => (
            <li key={c.id} className="d-card">
              <button className="cv-open" onClick={() => { setErr(null); setOpenId(c.id); }}>
                <strong>{c.customer?.name ?? "Pengunjung"}</strong>
                <span className="cv-badge" data-status={c.status}>{CONV_STATUS[c.status] ?? c.status}</span>
                <span className="cv-preview">{c.last_message ?? "—"}</span>
                <span className="d-muted">{c.message_count} pesan · {when(c.last_message_at)}{c.assignee_email ? ` · ${c.assignee_email}` : ""}</span>
              </button>
            </li>
          ))}
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
      <h2>Tindakan</h2>
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
        <form onSubmit={reply}>
          <label className="d-field"><span>Balas pelanggan</span>
            <textarea name="body" rows={3} maxLength={4000} required />
          </label>
          <button className="d-btn d-btn-primary" disabled={busy}>Kirim balasan</button>
          {d.status !== "HUMAN" && <span className="d-muted"> Membalas = mengambil alih (AI berhenti).</span>}
        </form>
      )}
      <div className="d-row" style={{ marginTop: 12 }}>
        {canTake && !closed && d.status !== "HUMAN" && (
          <button className="d-btn" disabled={busy} onClick={() => void run(() => api("POST", `${url}/takeover`), "Percakapan Anda tangani.")}>Ambil alih</button>
        )}
        {canTake && !closed && d.status !== "AI" && !byOther && (
          <button className="d-btn" disabled={busy} onClick={() => void run(() => api("POST", `${url}/release`), "Dikembalikan ke asisten AI.")}>Kembalikan ke AI</button>
        )}
        {!closed && !byOther && (
          <button className="d-btn" disabled={busy} onClick={() => void run(() => api("POST", `${url}/close`), "Percakapan ditutup.")}>Tutup percakapan</button>
        )}
      </div>
    </section>
  );
}
