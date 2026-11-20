"use client";

// Percakapan (P6-3/P6-4): daftar + Conversation Viewer + aksi staf (balas, ambil alih, kembalikan ke AI,
// tutup, draft AI mode Assisted). Pembaruan realtime: SSE tenant hanya mengirim ping `conversation_id`;
// isi diambil ulang lewat API (masking sesuai permission).

import Link from "@/ui/link";
import { useCallback, useEffect, useState, type FormEvent } from "react";

import { confirm } from "@/ui/dialog";

import { ChatCircleDots } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { ConversationView, when, type ConversationDetail, type ConversationSummary } from "@/inbox/conversation";
import { API_ORIGIN, ApiError, api, describe } from "@/lib/api";
import { CONVERSATION_STATUS, relatif, status } from "@/lib/labels";
import { BusyButton, EmptyState, PageHeader, PageSkeleton, Skeleton, Tabs } from "@/ui/page";
import { t, tmap } from "@/i18n";

export default function ConversationsPage() {
  return <WithTenant>{(ctx) => <Conversations ctx={ctx} />}</WithTenant>;
}

interface Counts { ai: number; waiting_human: number; human: number; closed: number; mine: number }
type Tab = "waiting" | "mine" | "ai" | "human" | "closed" | "all";
const TABS: Record<Tab, { label: string; query: string; count?: (c: Counts) => number }> = {
  waiting: { get label() { return t("Menunggu staf"); }, query: "&status=WAITING_HUMAN", count: (c) => c.waiting_human },
  mine: { get label() { return t("Ditangani saya"); }, query: "&mine=true", count: (c) => c.mine },
  ai: { get label() { return t("Dijawab AI"); }, query: "&status=AI", count: (c) => c.ai },
  human: { get label() { return t("Ditangani staf"); }, query: "&status=HUMAN", count: (c) => c.human },
  closed: { get label() { return t("Selesai"); }, query: "&status=CLOSED" },
  all: { get label() { return t("Semua"); }, query: "" },
};

const REASON: Record<string, string> = tmap({
  assigned_to_other: "Percakapan sedang ditangani staf lain.",
  conversation_closed: "Percakapan sudah ditutup.",
  draft_decided: "Draf sudah diputus.",
});

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

  if (!ctx.can("conversations.read")) return <Alert kind="error">{t("Anda tidak punya izin melihat percakapan.")}</Alert>;
  const back = () => { setOpenId(null); setOpen(null); void load(); };
  if (openId) {
    return (
      <>
        <PageHeader
          back={{ onClick: back, label: t("Semua percakapan") }}
          title={open?.customer?.name ?? t("Pengunjung")}
          description={open ? t("{0}, {1} pesan, terakhir {2}", status(CONVERSATION_STATUS, open.status).label, open.message_count, when(open.last_message_at)) : undefined}
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
      <PageHeader title={t("Percakapan")} description={t("Chat pelanggan dari website. Asisten AI menjawab otomatis; yang butuh manusia masuk ke antrean Menunggu staf.")} />
      <Alert kind="error">{err}</Alert>
      {masked && <Alert kind="info">{t("Data sensitif disamarkan untuk peran Anda.")}</Alert>}
      <Tabs
        label={t("Antrean")}
        value={tab}
        onChange={setTab}
        items={(Object.keys(TABS) as Tab[]).map((k) => ({ value: k, label: TABS[k].label, count: counts && TABS[k].count ? TABS[k].count(counts) : undefined }))}
      />
      {items === null ? <div className="d-card"><Skeleton lines={6} /></div> : items.length === 0 ? (
        <div className="d-card"><EmptyState icon={ChatCircleDots} title={t("Antrean kosong")}
          action={<Link className="d-btn" href={`/ai?tenant=${ctx.tenant.id}`}>{t("Atur Asisten AI & chat website")}</Link>}>{tab === "waiting" ? t("Tidak ada pelanggan yang menunggu staf. Kerja bagus!") : t("Belum ada percakapan di tab ini.")}</EmptyState></div>
      ) : (
        <ul className="cv-list" aria-label={t("Daftar percakapan")}>
          {items.map((c) => {
            const st = status(CONVERSATION_STATUS, c.status);
            return (
              <li key={c.id}>
                <button type="button" className="cv-open" onClick={() => { setErr(null); setOpenId(c.id); }}>
                  <span className="d-cell-main">
                    <span className="d-avatar d-avatar-muted" aria-hidden="true">{(c.customer?.name?.trim()[0] ?? "?").toUpperCase()}</span>
                    <span className="u-min-w-0">
                      <strong>{c.customer?.name ?? t("Pengunjung")}</strong>
                      <span className="cv-preview">{c.last_message ?? "—"}</span>
                    </span>
                  </span>
                  <span className="cv-side">
                    <span className="d-muted d-small">{relatif(c.last_message_at)}</span>
                    <span className="cv-badge" data-status={c.status}>{st.label}</span>
                  </span>
                  <span className="d-muted">{t("{0} pesan", c.message_count)}{c.assignee_email ? t(" · {0}", c.assignee_email) : ""}</span>
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
      setMsg({ ok: pending ? t("Menunggu persetujuan sebelum dikirim.") : ok });
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
    void run(async () => { await api("POST", `${url}/messages`, { json: { body } }); form.reset(); }, t("Balasan terkirim."));
  }

  const pending = d.drafts.filter((x) => x.status === "pending" || x.status === "approval_pending");
  return (
    <section className="d-card" aria-label={t("Tindakan staf")}>
      <header className="d-card-head">
        <div className="d-card-head-text"><h2>{t("Tindakan staf")}</h2>{d.status !== "HUMAN" && !closed && <p>{t("Membalas = mengambil alih; AI berhenti menjawab.")}</p>}</div>
        <div className="d-card-head-actions">
          {canTake && !closed && d.status !== "HUMAN" && (
            <button type="button" className="d-btn d-btn-sm" disabled={busy} onClick={() => void run(() => api("POST", `${url}/takeover`), t("Percakapan Anda tangani."))}>{t("Ambil alih")}</button>
          )}
          {canTake && !closed && d.status !== "AI" && !byOther && (
            <button type="button" className="d-btn d-btn-sm" disabled={busy} onClick={() => void run(() => api("POST", `${url}/release`), t("Dikembalikan ke asisten AI."))}>{t("Kembalikan ke AI")}</button>
          )}
          {!closed && !byOther && (
            <button type="button" className="d-btn d-btn-sm d-btn-danger" disabled={busy} onClick={async () => {
              if (await confirm({ title: t("Tutup percakapan?"), body: t("Pelanggan perlu memulai chat baru untuk menghubungi toko lagi."), confirmLabel: t("Tutup percakapan"), danger: true })) void run(() => api("POST", `${url}/close`), t("Percakapan ditutup."));
            }}>{t("Tutup percakapan")}</button>
          )}
        </div>
      </header>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {byOther && <p className="d-muted">{t("Sedang ditangani")}{" "}{d.assignee_email ?? t("staf lain")}.</p>}
      {pending.length > 0 && !closed && (
        <div aria-label={t("Draf AI")}>
          <h3>{t("Draf AI menunggu keputusan")}</h3>
          {pending.map((x) => (
            <div key={x.id} className="cv-draft">
              {x.status === "approval_pending" ? (
                <p>{x.body} <strong className="d-muted">{t("— menunggu persetujuan")}</strong></p>
              ) : (
                <>
                  <label className="d-field"><span>{t("Draf (boleh diedit)")}</span>
                    <textarea rows={3} maxLength={4000} value={edit[x.id] ?? x.body} onChange={(e) => setEdit({ ...edit, [x.id]: e.target.value })} />
                  </label>
                  <div className="d-row">
                    <button type="button" className="d-btn d-btn-primary" disabled={busy || byOther} onClick={() => void run(
                      () => api("POST", `${url}/drafts/${x.id}/send`, { json: { body: edit[x.id] ?? null } }), t("Draf terkirim."))}>{t("Kirim draf")}</button>
                    <button type="button" className="d-btn" disabled={busy} onClick={() => void run(
                      () => api("POST", `${url}/drafts/${x.id}/discard`), t("Draf dibuang."))}>{t("Buang")}</button>
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      )}
      {!closed && !byOther && (
        <form className="d-composer u-mt-0" onSubmit={reply}>
          <label className="sr-only" htmlFor="cv-reply">{t("Balas pelanggan")}</label>
          <textarea id="cv-reply" name="body" rows={3} maxLength={4000} required placeholder={t("Tulis balasan untuk pelanggan… (Enter kirim, Shift+Enter baris baru)")} enterKeyHint="send"
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); e.currentTarget.form?.requestSubmit(); } }} />
          <div className="d-row u-justify-end">
            <BusyButton busy={busy}>{t("Kirim balasan")}</BusyButton>
          </div>
        </form>
      )}
      {canTake && !closed && <AssignForm ctx={ctx} d={d} url={url} run={run} busy={busy} />}
      {ctx.can("tickets.reply") && (
        <details className="d-details cv-ticket" open>
        <summary>{t("Buat tiket dari percakapan")}</summary>
        <form onSubmit={(e) => {
          e.preventDefault();
          const form = e.currentTarget;
          const f = new FormData(form);
          void run(async () => {
            await api("POST", `${url}/tickets`, { json: { subject: f.get("subject"), priority: f.get("priority"), description: f.get("description") || null } });
            form.reset();
          }, t("Tiket dibuat."));
        }}>
          <div className="d-grid">
          <label className="d-field"><span>{t("Subjek tiket")}</span><input name="subject" maxLength={200} required /></label>
          <label className="d-field"><span>{t("Prioritas")}</span>
            <select name="priority" defaultValue="normal">
              <option value="low">{t("Rendah")}</option><option value="normal">{t("Normal")}</option><option value="high">{t("Tinggi")}</option><option value="urgent">{t("Mendesak")}</option>
            </select>
          </label>
          </div>
          <label className="d-field"><span>{t("Keterangan")}{" "}<span className="d-opt">{t("(opsional)")}</span></span><textarea name="description" rows={2} maxLength={5000} /></label>
          <button type="submit" className="d-btn" disabled={busy}>{t("Buat tiket")}</button>
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
    <div className="d-row u-mt-4 u-items-end">
      <label className="d-field u-m-0 u-grow"><span>{t("Tugaskan ke")}</span>
        <select value={to} onChange={(e) => setTo(e.target.value)}>
          <option value="">{t("Pilih anggota…")}</option>
          {members.map((m) => <option key={m.user_id} value={m.user_id}>{m.email} ({m.role_name})</option>)}
        </select>
      </label>
      <button type="button" className="d-btn" disabled={busy || !to} onClick={() => void run(() => api("POST", `${url}/assign`, { json: { user_id: to } }), t("Percakapan ditugaskan."))}>{t("Tugaskan")}</button>
    </div>
  );
}
