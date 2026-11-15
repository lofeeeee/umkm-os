"use client";

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { DotsThree } from "@phosphor-icons/react/ssr";

export interface MenuItem { label: string; run: () => void; danger?: boolean }

/**
 * Menu titik-tiga per baris. Dipasang lewat portal ke `document.body` dengan posisi `fixed`
 * dihitung dari tombol, agar TIDAK terpotong oleh kontainer tabel (`.d-table-wrap` memakai
 * `overflow`, yang mengklip dropdown absolute). Menutup saat klik di luar, Escape, atau scroll.
 */
export function RowMenu({ items, label }: { items: MenuItem[]; label: string }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number; origin: string } | null>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const place = () => {
    const b = btnRef.current;
    if (!b) return;
    const r = b.getBoundingClientRect();
    const width = 220;
    const height = items.length * 34 + 8; // estimasi: tiap item ~34px + padding
    const gap = 4;
    const left = Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8));
    const openUp = r.bottom + height + gap > window.innerHeight && r.top - height - gap > 8;
    const top = openUp ? r.top - height - gap : r.bottom + gap;
    setPos({ top, left, origin: openUp ? "bottom right" : "top right" });
  };

  useEffect(() => {
    if (!open) return;
    place();
    const close = () => setOpen(false);
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!btnRef.current?.contains(t) && !menuRef.current?.contains(t)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    // Scroll/resize mengubah posisi tombol → tutup agar menu tak "mengambang" salah tempat.
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  if (items.length === 0) return null;
  return (
    <>
      <button
        ref={btnRef}
        type="button"
        className="d-icon-btn"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <DotsThree weight="bold" />
      </button>
      {open && pos && typeof document !== "undefined" && createPortal(
        <div
          ref={menuRef}
          className="d-pop"
          role="menu"
          style={{ position: "fixed", top: pos.top, left: pos.left, minWidth: 220, ["--origin" as string]: pos.origin }}
        >
          {items.map((it) => (
            <button
              key={it.label}
              type="button"
              role="menuitem"
              className="d-menu-item"
              style={it.danger ? { color: "var(--danger)" } : undefined}
              onClick={() => { setOpen(false); it.run(); }}
            >
              {it.label}
            </button>
          ))}
        </div>,
        document.body,
      )}
    </>
  );
}
