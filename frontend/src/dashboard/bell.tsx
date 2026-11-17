"use client";

// Lonceng notifikasi (P4-4) + pembaruan realtime lewat SSE (P4-6). Sumber kebenaran tetap API;
// event SSE hanya memicu muat ulang. Nama aksesibel tombol: "Notifikasi" / "Notifikasi (n)"
// (dipakai e2e ops.spec.ts via toHaveAccessibleName).

import { useCallback, useEffect, useRef, useState } from "react";

import { Bell, BellSimpleSlash } from "@phosphor-icons/react/ssr";

import { API_ORIGIN, api } from "@/lib/api";
import { relatif } from "@/lib/labels";
import { t } from "@/i18n";

interface Item { id: string; title: string; body: string; link: string | null; read: boolean; created_at: string }

export function BellMenu({ tenantId }: { tenantId: string }) {
  const base = `/api/v1/tenants/${tenantId}/notifications`;
  const [items, setItems] = useState<Item[]>([]);
  const [unread, setUnread] = useState(0);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const name = t("Notifikasi{0}", unread > 0 ? ` (${unread})` : "");
  return (
    <div className="d-bell" ref={ref}>
      <button type="button" className="d-bell-btn" aria-expanded={open} aria-haspopup="dialog" aria-label={name} title={name} onClick={() => setOpen(!open)}>
        <Bell aria-hidden="true" weight={unread > 0 ? "fill" : "bold"} />
        {unread > 0 && <span className="d-bell-dot" aria-hidden="true">{unread > 9 ? "9+" : unread}</span>}
      </button>
      {open && (
        <div className="d-pop d-bell-panel" role="dialog" aria-label={t("Notifikasi")}>
          <div className="d-bell-head">
            <strong>{t("Notifikasi")}</strong>
            {unread > 0 && (
              <button type="button" className="d-link d-small" onClick={() => void api("POST", `${base}/read-all`).then(load)}>{t("Tandai semua dibaca")}</button>
            )}
          </div>
          {items.length === 0 ? (
            <div className="d-empty">
              <span className="d-empty-icon" aria-hidden="true"><BellSimpleSlash weight="duotone" /></span>
              <p>{t("Belum ada notifikasi.")}</p>
            </div>
          ) : (
            <ul>
              {items.map((n) => (
                <li key={n.id} className={n.read ? "" : "d-unread"}>
                  <a href={n.link ?? "#"} onClick={() => void api("POST", `${base}/${n.id}/read`).then(load)}>
                    <strong>{n.title}</strong>
                    <span>{n.body}</span>
                    <span className="d-small u-subtle">{relatif(n.created_at)}</span>
                  </a>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
