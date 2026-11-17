"use client";

// Dialog berbasis <dialog> native: fokus terkunci, Escape menutup, backdrop dari browser.
// - `confirm()` = konfirmasi berbasis Promise untuk aksi berisiko (hapus, arsipkan, tolak).
// - <Sheet> = panel samping untuk form panjang (editor produk, detail pesanan) tanpa
//   meninggalkan daftar — konteks tetap terlihat di belakang.

import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";

import { X } from "@phosphor-icons/react/ssr";
import { t } from "@/i18n";

/* ---------------- confirm() ---------------- */
interface ConfirmOpts {
  title: string;
  body?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}
interface Pending extends ConfirmOpts { resolve: (ok: boolean) => void }

let pending: Pending | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export function confirm(opts: ConfirmOpts): Promise<boolean> {
  pending?.resolve(false);
  return new Promise((resolve) => {
    pending = { ...opts, resolve };
    emit();
  });
}

export function ConfirmHost() {
  const p = useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    () => pending,
    () => null,
  );
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (p && !d.open) d.showModal();
    if (!p && d.open) d.close();
  }, [p]);
  const done = (ok: boolean) => {
    const cur = pending;
    pending = null;
    emit();
    cur?.resolve(ok);
  };
  return (
    <dialog
      ref={ref}
      className="d-dialog"
      aria-labelledby="d-confirm-title"
      onCancel={(e) => { e.preventDefault(); done(false); }}
      onClick={(e) => { if (e.target === e.currentTarget) done(false); }}
    >
      {p && (
        <form method="dialog" onSubmit={(e) => { e.preventDefault(); done(true); }}>
          <div className="d-dialog-body">
            <h2 id="d-confirm-title">{p.title}</h2>
            {p.body && <p>{p.body}</p>}
          </div>
          <div className="d-dialog-foot">
            <button type="button" className="d-btn" onClick={() => done(false)}>{p.cancelLabel ?? t("Batal")}</button>
            <button type="submit" className={`d-btn ${p.danger ? "d-btn-danger-solid" : "d-btn-primary"}`} autoFocus>
              {p.confirmLabel ?? t("Lanjutkan")}
            </button>
          </div>
        </form>
      )}
    </dialog>
  );
}

/* ---------------- <Sheet> ---------------- */
export function Sheet({
  open, onClose, title, description, children, footer, wide, labelledBy,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  labelledBy?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [id] = useState(() => labelledBy ?? `sheet-${Math.random().toString(36).slice(2, 8)}`);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  // Kunci scroll halaman di belakang selama sheet terbuka.
  useEffect(() => {
    if (!open) return;
    const prev = document.documentElement.style.overflow;
    document.documentElement.style.overflow = "hidden";
    return () => { document.documentElement.style.overflow = prev; };
  }, [open]);
  return (
    <dialog
      ref={ref}
      className={`d-sheet${wide ? " d-sheet-wide" : ""}`}
      aria-labelledby={id}
      onCancel={(e) => { e.preventDefault(); onClose(); }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      {open && (
        <>
          <header className="d-sheet-head">
            <div style={{ flex: 1, minWidth: 0 }}>
              <h2 id={id}>{title}</h2>
              {description && <p>{description}</p>}
            </div>
            <button type="button" className="d-icon-btn" aria-label={t("Tutup panel")} onClick={onClose}>
              <X weight="bold" />
            </button>
          </header>
          <div className="d-sheet-body">{children}</div>
          {footer && <footer className="d-sheet-foot">{footer}</footer>}
        </>
      )}
    </dialog>
  );
}
