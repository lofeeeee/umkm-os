"use client";

// Pelapis interaktif blok di kanvas Studio (mode `studio-edit`).
//
// Saat mode edit, setiap blok dibungkus elemen ini agar bisa dipilih, diberi label, dan ditandai terpilih.
// Di mode `customer` pembungkus ini TIDAK dipasang sama sekali, sehingga pohon DOM situs terbit identik
// dengan yang dirender Studio (lihat tes paritas di `StudioSite.test.tsx`).
//
// Klik di dalam isi blok tetap sampai ke isi (mis. mengetik di `contentEditable`), karena pemilihan hanya
// terjadi bila target bukan elemen `[data-interactive]`.

import type { ReactNode } from "react";

export interface EditableBlockWrapperProps {
  blockId: string;
  /** Nama plugin yang ditampilkan di label, mis. "Banner". */
  label: string;
  selected: boolean;
  locked?: boolean;
  onSelect: () => void;
  children: ReactNode;
}

export function EditableBlockWrapper({ blockId, label, selected, locked, onSelect, children }: EditableBlockWrapperProps) {
  return (
    <div
      className={`sf-block${selected ? " sf-block-selected" : ""}${locked ? " sf-block-locked" : ""}`}
      data-block-id={blockId}
      data-block-label={label}
      onPointerDown={(e) => {
        // Kontrol di dalam blok (input, tautan, tombol, teks yang bisa diketik) tidak boleh memicu pemilihan.
        if ((e.target as HTMLElement).closest("[data-interactive], button, label, input, textarea, a, [contenteditable]")) return;
        e.stopPropagation();
        onSelect();
      }}
    >
      <span className="sf-block-tag" aria-hidden="true">{label}</span>
      {locked && <span className="sf-block-lock" aria-hidden="true" title="Blok terkunci" />}
      {children}
    </div>
  );
}
