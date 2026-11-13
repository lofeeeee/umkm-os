"use client";

// Conversation Viewer (P6-3, FR-CNV-002/003): timeline, konteks pelanggan, provider/model, token &
// latensi, ringkasan tool, riwayat assignment/handover, status, approval. Data sensitif sudah
// dimasking oleh API sesuai permission; UI hanya menandai bila tampilan dimasking.

export interface CustomerContext { id: string; name: string; email: string | null; phone: string | null; has_account: boolean }
export interface ConversationSummary {
  id: string; channel: string; status: string; customer: CustomerContext | null; assignee_user_id: string | null;
  assignee_email: string | null; human_involved: boolean; message_count: number; last_message: string | null;
  last_message_at: string; created_at: string;
}
export interface AiRun {
  run_id: string; provider_name: string | null; source: string; model: string; status: string; decision: string | null;
  error_code: string | null; prompt_tokens: number; completion_tokens: number; latency_ms: number;
  tool_calls: { name: string; status: string; args_summary: Record<string, unknown> | null }[];
}
export interface TimelineMessage {
  id: string; seq: number; sender: "CUSTOMER" | "AI" | "ADMIN" | "SYSTEM"; sender_user_id: string | null;
  sender_email: string | null; body: string; created_at: string; ai: AiRun | null;
}
export interface ConversationDetail extends ConversationSummary {
  consent_version: string; consented_at: string; closed_at: string | null; timeline: TimelineMessage[];
  other_ai_runs: AiRun[];
  history: { kind: string; from_status: string | null; to_status: string | null; actor_type: string; actor_email: string | null; created_at: string }[];
  approvals: { id: string; status: string; action: string; summary: string; current_step: number; created_at: string }[];
  drafts: { id: string; body: string; status: string; ai_run_id: string | null; created_at: string }[];
  tickets: { id: string; code: string; subject: string; status: string; created_at: string }[];
  masked: boolean;
}

export const CONV_STATUS: Record<string, string> = {
  AI: "Dijawab AI", WAITING_HUMAN: "Menunggu staf", HUMAN: "Ditangani staf", CLOSED: "Selesai",
};
const SENDER: Record<string, string> = { CUSTOMER: "Pelanggan", AI: "Asisten AI", ADMIN: "Staf", SYSTEM: "Sistem" };
const EVENT: Record<string, string> = {
  created: "Percakapan dimulai", assigned: "Ditugaskan", unassigned: "Penugasan dilepas",
  handover_requested: "Diteruskan ke staf", taken_over: "Diambil alih staf", released: "Dikembalikan ke AI", closed: "Ditutup",
  draft_created: "Draft AI dibuat", draft_sent: "Draft AI dikirim", draft_discarded: "Draft AI dibuang",
  draft_approval_requested: "Draft AI menunggu persetujuan",
};
const ACTOR: Record<string, string> = { customer: "pelanggan", ai: "AI", staff: "staf", system: "sistem" };
const DECISION: Record<string, string> = {
  send_auto: "dikirim otomatis", draft_for_admin: "draft untuk staf", handover: "dialihkan ke staf", human_queue: "antre staf",
};

export function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("id-ID", { dateStyle: "medium", timeStyle: "short" });
}

function RunMeta({ r }: { r: AiRun }) {
  return (
    <div className="d-muted cv-run" data-testid="ai-meta">
      {r.provider_name ?? "provider"} · {r.model} ({r.source}) · {r.prompt_tokens}+{r.completion_tokens} token · {r.latency_ms} ms
      {r.decision && <> · {DECISION[r.decision] ?? r.decision}</>}
      {r.status !== "ok" && <> · status {r.status}{r.error_code ? ` (${r.error_code})` : ""}</>}
      {r.tool_calls.length > 0 && (
        <ul aria-label="Tool">
          {r.tool_calls.map((t, i) => (
            <li key={i}>{t.name} — {t.status}{t.args_summary && Object.keys(t.args_summary).length > 0 ? ` ${JSON.stringify(t.args_summary)}` : ""}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function ConversationView({ d }: { d: ConversationDetail }) {
  return (
    <div className="cv">
      {d.masked && (
        <p className="d-alert" role="note">Data sensitif (kontak pelanggan, email/nomor dalam pesan) disamarkan untuk peran Anda.</p>
      )}
      <div className="d-grid2">
        <section className="d-card" aria-label="Pelanggan">
          <h2>Pelanggan</h2>
          {d.customer ? (
            <dl className="cv-dl">
              <dt>Nama</dt><dd>{d.customer.name}</dd>
              <dt>Email</dt><dd>{d.customer.email ?? "—"}</dd>
              <dt>Telepon</dt><dd>{d.customer.phone ?? "—"}</dd>
              <dt>Akun</dt><dd>{d.customer.has_account ? "terverifikasi" : "tamu"}</dd>
            </dl>
          ) : <p className="d-muted">Pengunjung anonim (belum masuk akun).</p>}
        </section>
        <section className="d-card" aria-label="Status percakapan">
          <h2>Status</h2>
          <dl className="cv-dl">
            <dt>Status</dt><dd>{CONV_STATUS[d.status] ?? d.status}</dd>
            <dt>Channel</dt><dd>{d.channel === "web" ? "Web chat" : d.channel}</dd>
            <dt>Ditangani</dt><dd>{d.assignee_email ?? "—"}</dd>
            <dt>Persetujuan AI</dt><dd>{d.consent_version} · {when(d.consented_at)}</dd>
            {d.closed_at && <><dt>Ditutup</dt><dd>{when(d.closed_at)}</dd></>}
          </dl>
        </section>
      </div>

      <section className="d-card" aria-label="Timeline">
        <h2>Timeline</h2>
        <ol className="cv-timeline">
          {d.timeline.map((m) => (
            <li key={m.id} className={`cv-msg cv-${m.sender.toLowerCase()}`}>
              <div className="cv-head">
                <strong>{SENDER[m.sender] ?? m.sender}</strong>
                {m.sender_email && <span className="d-muted"> · {m.sender_email}</span>}
                <span className="d-muted"> · {when(m.created_at)}</span>
              </div>
              <div className="cv-body">{m.body}</div>
              {m.ai && <RunMeta r={m.ai} />}
            </li>
          ))}
        </ol>
        {d.other_ai_runs.length > 0 && (
          <>
            <h3>Pemanggilan AI tanpa balasan</h3>
            {d.other_ai_runs.map((r) => <RunMeta key={r.run_id} r={r} />)}
          </>
        )}
      </section>

      <div className="d-grid2">
        <section className="d-card" aria-label="Riwayat">
          <h2>Riwayat penugasan & handover</h2>
          <ol className="cv-history">
            {d.history.map((e, i) => (
              <li key={i}>
                {EVENT[e.kind] ?? e.kind} — oleh {ACTOR[e.actor_type] ?? e.actor_type}{e.actor_email ? ` (${e.actor_email})` : ""}
                <span className="d-muted"> · {when(e.created_at)}</span>
              </li>
            ))}
          </ol>
        </section>
        <section className="d-card" aria-label="Tiket">
          <h2>Tiket terkait</h2>
          {d.tickets.length === 0 ? <p className="d-muted">Belum ada.</p> : (
            <ul>{d.tickets.map((x) => <li key={x.id}><strong>{x.code}</strong> {x.subject} — {x.status}</li>)}</ul>
          )}
        </section>
        <section className="d-card" aria-label="Approval">
          <h2>Approval</h2>
          {d.approvals.length === 0 ? <p className="d-muted">Tidak ada.</p> : (
            <ul>
              {d.approvals.map((a) => <li key={a.id}>{a.summary} — <strong>{a.status}</strong> (langkah {a.current_step + 1})</li>)}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}
