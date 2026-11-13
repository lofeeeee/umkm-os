"use client";

import { useEffect, useRef, useState } from "react";

import { DotsThree } from "@phosphor-icons/react/ssr";

export interface MenuItem { label: string; run: () => void; danger?: boolean }

/** Menu titik-tiga per baris untuk aksi sekunder. */
export function RowMenu({ items, label }: { items: MenuItem[]; label: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const off = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", off);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", off); document.removeEventListener("keydown", esc); };
  }, [open]);
  if (items.length === 0) return null;
  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button type="button" className="d-icon-btn" aria-label={label} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <DotsThree weight="bold" />
      </button>
      {open && (
        <div className="d-pop" role="menu" style={{ right: 0, top: "calc(100% + 4px)", minWidth: 160, ["--origin" as string]: "top right" }}>
          {items.map((it) => (
            <button key={it.label} type="button" role="menuitem" className="d-menu-item" style={it.danger ? { color: "var(--danger)" } : undefined}
              onClick={() => { setOpen(false); it.run(); }}>
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

