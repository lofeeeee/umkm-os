"use client";

// Percakapan (P6-3): daftar + Conversation Viewer. Balasan staf & takeover menyusul (P6-4/P6-5).

import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { CONV_STATUS, ConversationView, when, type ConversationDetail, type ConversationSummary } from "@/inbox/conversation";
import { api, describe } from "@/lib/api";

export default function ConversationsPage() {
  return <WithTenant>{(ctx) => <Conversations ctx={ctx} />}</WithTenant>;
}

function Conversations({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/conversations`;
  const [items, setItems] = useState<ConversationSummary[]>([]);
  const [masked, setMasked] = useState(false);
  const [status, setStatus] = useState("");
  const [open, setOpen] = useState<ConversationDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: ConversationSummary[]; masked: boolean }>("GET", `${base}?limit=100${status ? `&status=${status}` : ""}`)
      .then((r) => { setItems(r.items); setMasked(r.masked); }, (e) => setErr(describe(e))),
    [base, status],
  );
  useEffect(() => { void load(); }, [load]);

  async function show(id: string) {
    setErr(null);
    try { setOpen(await api<ConversationDetail>("GET", `${base}/${id}`)); } catch (e) { setErr(describe(e)); }
  }

  if (!ctx.can("conversations.read")) return <Alert kind="error">Anda tidak punya izin melihat percakapan.</Alert>;
  if (open) {
    return (
      <>
        <button className="d-btn d-btn-sm" onClick={() => { setOpen(null); void load(); }}>← Kembali</button>
        <h1>Percakapan</h1>
        <Alert kind="error">{err}</Alert>
        <ConversationView d={open} />
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
              <button className="cv-open" onClick={() => void show(c.id)}>
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
