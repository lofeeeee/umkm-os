"use client";

// Galeri template Studio + miniatur tata letak.

import { useMemo } from "react";

import { ChatCircleDots } from "@phosphor-icons/react/ssr";

import type { Business } from "@/renderer/types";
import { COLS, DESIGN_WIDTH, GAP, ROW } from "@/studio/grid";
import { TEMPLATES } from "@/studio/templates";
import { t as tx } from "@/i18n";
import { type StudioCell, isFloating, isEmbed } from "@/studio/editor-model";

export function TemplateGallery({ first, business, renderCell, onPick, onCurrent, surfaceOf }: {
  first?: boolean;
  business: Business;
  renderCell: (c: StudioCell, editable?: boolean, th?: { primary: string; font: string }) => React.ReactNode;
  onPick: (key: string) => void;
  onCurrent: () => void;
  surfaceOf: (key: string) => { primary: string; surface: string; font: string; cells: StudioCell[] };
}) {
  const previews = useMemo(() => Object.fromEntries(TEMPLATES.map((t) => [t.key, surfaceOf(t.key)])), [surfaceOf]);
  return (
    <div className={first ? "st-gallery st-gallery-first" : "st-gallery"}>
      {first && (
        <div className="st-gallery-head">
          <h1>{tx("Pilih gaya awal untuk")}{" "}{business.profile.name}</h1>
          <p>{tx("Semua templat sudah diisi nama, produk, dan jam buka toko Anda. Teks, foto, warna, dan posisi bisa diubah setelahnya.")}</p>
        </div>
      )}
      <div className="st-tpls">
        {TEMPLATES.map((t) => {
          const p = previews[t.key]!;
          return (
            <div key={t.key} className="st-tpl" onClick={() => onPick(t.key)}>
              <span className="st-tpl-thumb" data-surface={p.surface} style={{ ["--s-primary" as string]: p.primary }}>
                <MiniLayout cells={p.cells.filter((c) => !isEmbed(c))} render={(c) => renderCell(c, false, { primary: p.primary, font: p.font })} theme={{ primary: p.primary, font: p.font }} />
                {p.cells.some(isFloating) && <span className="st-tpl-float" style={{ background: p.primary }} aria-hidden="true"><ChatCircleDots weight="fill" /></span>}
              </span>
              <span className="st-tpl-meta">
                <span className="d-row d-row-between u-nowrap">
                  <strong>{t.name}</strong>
                  <button type="button" className="d-btn d-btn-sm d-btn-primary" onClick={(e) => { e.stopPropagation(); onPick(t.key); }}>{tx("Pakai")}{" "}{t.name}</button>
                </span>
                <span className="st-tpl-cat">{t.category}{t.pack ? ` · ${tx("Paket {0} halaman", 1 + (t.pack({ name: business.profile.name, description: null }).length))}` : ""}</span>
                <span>{t.desc}</span>
              </span>
            </div>
          );
        })}
      </div>
      <div className="d-row u-justify-center u-mt-4">
        <button type="button" className="d-btn d-btn-ghost" onClick={onCurrent}>{tx("Atau mulai dari susunan website saat ini")}</button>
      </div>
    </div>
  );
}

/** Miniatur tata letak (tanpa interaksi) di lebar desain lalu diperkecil. */
export function MiniLayout({ cells, render, theme }: { cells: StudioCell[]; render: (c: StudioCell) => React.ReactNode; theme: { primary: string; font: string } }) {
  const colW = (DESIGN_WIDTH - GAP * (COLS - 1)) / COLS;
  return (
    <span className="st-mini" aria-hidden="true" style={{ ["--s-primary" as string]: theme.primary }}>
      {cells.map((c) => (
        <span key={c.id} className="st-mini-cell" style={{ transform: `translate(${c.x * (colW + GAP)}px, ${c.y * (ROW + GAP)}px)`, width: c.w * colW + (c.w - 1) * GAP, height: c.h * ROW + (c.h - 1) * GAP }}>
          {render(c)}
        </span>
      ))}
    </span>
  );
}

/* ============================================================ Inspektur */
