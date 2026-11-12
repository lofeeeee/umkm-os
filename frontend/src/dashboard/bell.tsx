"use client";

// Lonceng notifikasi (P4-4) + pembaruan realtime lewat SSE (P4-6). Sumber kebenaran tetap API;
// event SSE hanya memicu muat ulang.

import { useCallback, useEffect, useState } from "react";

import { API_ORIGIN, api } from "@/lib/api";

interface Item { id: string; title: string; body: string; link: string | null; read: boolean; created_at: string }

export function Bell({ tenantId }: { tenantId: string }) {
  const base = `/api/v1/tenants/${tenantId}/notifications`;
  const [items, setItems] = useState<Item[]>([]);
  const [unread, setUnread] = useState(0);
  const [open, setOpen] = useState(false);
  const load = useCallback(
    () => api<{ items: Item[]; unread: number }>("GET", `${base}?limit=20`).then((r) => { setItems(r.items); setUnread(r.unread); }, () => undefined),
    [base],
  );
  useEffect(() => {
    void load();
    let es: EventSource | null = null;
    try {
      es = new EventSource(`${API_ORIGIN}/api/v1/tenants/${tenantId}/events`, { withCredentials: true });
      es.addEventListener("notification.created", () => void load());
    } catch {
      es = null;
    }
    return () => es?.close();
  }, [load, tenantId]);
  return (
    <div className="d-bell">
      <button type="button" className="d-btn d-btn-sm" aria-expanded={open} onClick={() => setOpen(!open)}>
        Notifikasi{unread > 0 ? ` (${unread})` : ""}
      </button>
      {open && (
        <div className="d-bell-panel" role="dialog" aria-label="Notifikasi">
          {items.length === 0 && <p className="d-muted">Belum ada notifikasi.</p>}
          <ul>
            {items.map((n) => (
              <li key={n.id} className={n.read ? "" : "d-unread"}>
                <a href={n.link ?? "#"} onClick={() => void api("POST", `${base}/${n.id}/read`).then(load)}>
                  <strong>{n.title}</strong>
                  <span>{n.body}</span>
                </a>
              </li>
            ))}
          </ul>
          {unread > 0 && <button type="button" className="d-btn d-btn-sm" onClick={() => void api("POST", `${base}/read-all`).then(load)}>Tandai semua dibaca</button>}
        </div>
      )}
    </div>
  );
}
