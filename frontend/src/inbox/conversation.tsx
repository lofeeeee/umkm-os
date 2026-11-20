"use client";

// Conversation Viewer (P6-3, FR-CNV-002/003): timeline, konteks pelanggan, provider/model, token &
// latensi, ringkasan tool, riwayat assignment/handover, status, approval. Data sensitif sudah
// dimasking oleh API sesuai permission; UI hanya menandai bila tampilan dimasking.

import type { ReactNode } from "react";

import { APPROVAL_STATUS, CONVERSATION_STATUS, TICKET_STATUS, status } from "@/lib/labels";
import { Badge } from "@/ui/page";
import { t as tx, tmap, intlLocale } from "@/i18n";
import { Tr } from "@/i18n/react";

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

export const CONV_STATUS: Record<string, string> = tmap({
  AI: "Dijawab AI", WAITING_HUMAN: "Menunggu staf", HUMAN: "Ditangani staf", CLOSED: "Selesai",
});
const SENDER: Record<string, string> = tmap({ CUSTOMER: "Pelanggan", AI: "Asisten AI", ADMIN: "Staf", SYSTEM: "Sistem" });
const EVENT: Record<string, string> = tmap({
  created: "Percakapan dimulai", assigned: "Ditugaskan", unassigned: "Penugasan dilepas",
  handover_requested: "Diteruskan ke staf", taken_over: "Diambil alih staf", released: "Dikembalikan ke AI", closed: "Ditutup",
  draft_created: "Draf AI dibuat", draft_sent: "Draf AI dikirim", draft_discarded: "Draf AI dibuang",
  draft_approval_requested: "Draf AI menunggu persetujuan",
});
const ACTOR: Record<string, string> = tmap({ customer: "pelanggan", ai: "AI", staff: "staf", system: "sistem" });
const DECISION: Record<string, string> = tmap({
  send_auto: "dikirim otomatis", draft_for_admin: "draf untuk staf", handover: "dialihkan ke staf", human_queue: "antre staf",
});

export function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString(intlLocale(), { dateStyle: "medium", timeStyle: "short" });
}

function RunMeta({ r }: { r: AiRun }) {
  return (
    <div className="d-muted cv-run" data-testid="ai-meta">
      <Tr s="{0}: {1} ({2}), {3}+{4} token, {5} ms" a={[r.provider_name ?? "provider", r.model, r.source, r.prompt_tokens, r.completion_tokens, r.latency_ms]} />
      {r.decision && <>, {DECISION[r.decision] ?? r.decision}</>}
      {r.status !== "ok" && <>{tx(", status")}{" "}{r.status}{r.error_code ? ` (${r.error_code})` : ""}</>}
      {r.tool_calls.length > 0 && (
        <ul aria-label={tx("Tool")}>
          {r.tool_calls.map((t, i) => (
            <li key={i}>{t.name} — {t.status}{t.args_summary && Object.keys(t.args_summary).length > 0 ? ` ${JSON.stringify(t.args_summary)}` : ""}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * Tampilan percakapan: timeline (gelembung chat) + slot `staff` (tindakan staf) di kiri, konteks di kanan.
 * Nama region dipakai e2e: Timeline, Status percakapan, Pelanggan, Riwayat, Tiket, Approval.
 */
export function ConversationView({ d, staff }: { d: ConversationDetail; staff?: ReactNode }) {
  const st = status(CONVERSATION_STATUS, d.status);
  return (
    <div className="cv">
      {d.masked && (
        <div className="d-alert d-alert-info" role="note"><div>{tx("Data sensitif (kontak pelanggan, email/nomor dalam pesan) disamarkan untuk peran Anda.")}</div></div>
      )}
      <div className="d-split">
        <div className="d-stack u-content-start u-min-w-0">
          <section className="d-card" aria-label={tx("Timeline")}>
            <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Percakapan")}</h2><p>{d.timeline.length}{" "}{tx("pesan · dimulai")}{" "}{when(d.created_at)}</p></div></header>
            {/* role="log" (polite): pesan baru diumumkan pembaca layar; key = ganti percakapan tidak membacakan ulang seluruh riwayat. */}
            <ol className="cv-timeline" role="log" aria-label={tx("Pesan")} key={d.id}>
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
              <details className="d-details u-mt-4">
                <summary><Tr s="Pemanggilan AI tanpa balasan ({0})" a={[d.other_ai_runs.length]} /></summary>
                {d.other_ai_runs.map((r) => <RunMeta key={r.run_id} r={r} />)}
              </details>
            )}
          </section>
          {staff}
        </div>

        <div className="d-stack u-content-start u-min-w-0">
          <section className="d-card" aria-label={tx("Status percakapan")}>
            <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Status")}</h2></div><Badge tone={st.tone}>{st.label}</Badge></header>
            <dl className="d-dl">
              <dt>{tx("Status")}</dt><dd>{st.label}</dd>
              <dt>{tx("Kanal")}</dt><dd>{d.channel === "web" ? tx("Chat website") : d.channel}</dd>
              <dt>{tx("Ditangani")}</dt><dd>{d.assignee_email ?? "—"}</dd>
              <dt>{tx("Persetujuan AI")}</dt><dd>{d.consent_version} · {when(d.consented_at)}</dd>
              {d.closed_at && <><dt>{tx("Ditutup")}</dt><dd>{when(d.closed_at)}</dd></>}
            </dl>
          </section>
          <section className="d-card" aria-label={tx("Pelanggan")}>
            <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Pelanggan")}</h2></div></header>
            {d.customer ? (
              <dl className="d-dl">
                <dt>{tx("Nama")}</dt><dd>{d.customer.name}</dd>
                <dt>{tx("Email")}</dt><dd>{d.customer.email ?? "—"}</dd>
                <dt>{tx("Telepon")}</dt><dd>{d.customer.phone ?? "—"}</dd>
                <dt>{tx("Akun")}</dt><dd>{d.customer.has_account ? tx("Terverifikasi") : tx("Tamu")}</dd>
              </dl>
            ) : <p className="d-muted">{tx("Pengunjung anonim (belum masuk akun).")}</p>}
          </section>
          <section className="d-card" aria-label={tx("Tiket")}>
            <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Tiket terkait")}</h2></div></header>
            {d.tickets.length === 0 ? <p className="d-muted">{tx("Belum ada.")}</p> : (
              <ul className="d-facts">{d.tickets.map((x) => <li key={x.id}><span><strong>{x.code}</strong> {x.subject} <Badge tone={status(TICKET_STATUS, x.status).tone} plain>{status(TICKET_STATUS, x.status).label}</Badge></span></li>)}</ul>
            )}
          </section>
          <section className="d-card" aria-label={tx("Riwayat")}>
            <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Riwayat penugasan")}</h2></div></header>
            <ol className="d-timeline">
              {[...d.history].reverse().map((e, i) => (
                <li key={i}>
                  <span>{EVENT[e.kind] ?? e.kind}{" "}{tx("— oleh")}{" "}{ACTOR[e.actor_type] ?? e.actor_type}{e.actor_email ? ` (${e.actor_email})` : ""}</span>
                  <span className="d-muted d-small">{when(e.created_at)}</span>
                </li>
              ))}
            </ol>
          </section>
          {d.approvals.length > 0 && (
            <section className="d-card" aria-label={tx("Persetujuan")}>
              <header className="d-card-head"><div className="d-card-head-text"><h2>{tx("Persetujuan")}</h2></div></header>
              <ul className="d-facts">
                {d.approvals.map((a) => <li key={a.id}><span>{a.summary} — <Badge tone={status(APPROVAL_STATUS, a.status).tone} plain>{status(APPROVAL_STATUS, a.status).label}</Badge> <span className="d-muted"><Tr s="(langkah {0})" a={[a.current_step + 1]} /></span></span></li>)}
              </ul>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
