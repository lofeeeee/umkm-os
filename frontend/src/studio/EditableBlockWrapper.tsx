"use client";

// Pelapis interaktif blok di kanvas Studio (mode `studio-edit`).
//
// Saat mode edit, setiap blok dibungkus elemen ini agar bisa dipilih, diseret, diubah ukurannya, dan
// diberi foto. Di mode `customer` pembungkus ini TIDAK dipasang sama sekali, sehingga pohon DOM situs
// terbit identik dengan yang dirender Studio (lihat tes paritas di `StudioSite.test.tsx`).
//
// - Klik = pilih. Tekan lalu geser (≥4 px) di mana saja pada blok = pindahkan, kecuali di atas teks yang
//   bisa diketik, tombol, atau isian — di sana klik tetap untuk mengetik/menekan.
// - Pegangan kanan = ubah lebar (kolom), pegangan bawah = ubah tinggi.

import { useState, type ReactNode } from "react";

import { DotsSixVertical, LockSimple } from "@phosphor-icons/react/ssr";

import { useBlockEditor } from "./editor-context";
import { t } from "@/i18n";

export interface EditableBlockWrapperProps {
  blockId: string;
  /** Nama plugin yang ditampilkan di label, mis. "Banner". */
  label: string;
  selected: boolean;
  locked?: boolean;
  onSelect: () => void;
  children: ReactNode;
}

const INTERACTIVE = "[data-interactive], button, label, input, textarea, select, a, [contenteditable]";

export function EditableBlockWrapper({ blockId, label, selected, locked, onSelect, children }: EditableBlockWrapperProps) {
  const ed = useBlockEditor();
  const [fileOver, setFileOver] = useState(false);
  const active = ed?.activeId === blockId;
  const takesFile = !!ed?.acceptsFile?.(blockId);
  const cls = [
    "sf-block",
    selected && "sf-block-selected",
    locked && "sf-block-locked",
    active && "sf-block-active",
    fileOver && "sf-block-file",
  ].filter(Boolean).join(" ");
  return (
    <div
      className={cls}
      data-block-id={blockId}
      data-block-label={label}
      onPointerDown={(e) => {
        if (e.button !== 0 || ed?.drawing) return;
        // Kontrol di dalam blok (input, tautan, tombol, teks yang bisa diketik) tidak boleh memicu pemilihan.
        if ((e.target as HTMLElement).closest(INTERACTIVE)) return;
        e.stopPropagation();
        onSelect();
        ed?.startGesture(blockId, "move", e);
      }}
      onDragOver={(e) => {
        if (!takesFile || !e.dataTransfer.types.includes("Files")) return;
        e.preventDefault();
        e.stopPropagation();
        if (!fileOver) setFileOver(true);
      }}
      onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setFileOver(false); }}
      onDrop={(e) => {
        const f = e.dataTransfer.files?.[0];
        setFileOver(false);
        if (!f || !takesFile) return;
        e.preventDefault();
        e.stopPropagation();
        ed?.onDropFile?.(blockId, f);
      }}
    >
      <span className="sf-block-bar" data-interactive>
        <span
          className="sf-block-tag"
          title={locked ? t("Posisi terkunci") : t("Seret untuk memindahkan")}
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            e.stopPropagation();
            onSelect();
            ed?.startGesture(blockId, "move", e);
          }}
        >
          {locked ? <LockSimple weight="fill" aria-hidden="true" /> : <DotsSixVertical weight="bold" aria-hidden="true" />}
          {label}
        </span>
        {selected && ed?.toolbar && <span className="sf-block-tools">{ed.toolbar(blockId)}</span>}
      </span>
      {locked && <span className="sf-block-lock" aria-hidden="true" title={t("Blok terkunci")} />}
      {children}
      {ed && !locked && (
        <>
          <span className="sf-handle sf-handle-x" data-interactive aria-hidden="true" title={t("Tarik untuk mengubah lebar")}
            onPointerDown={(e) => { if (e.button !== 0) return; e.stopPropagation(); onSelect(); ed.startGesture(blockId, "resize-x", e); }} />
          <span className="sf-handle sf-handle-y" data-interactive aria-hidden="true" title={t("Tarik untuk mengubah tinggi")}
            onPointerDown={(e) => { if (e.button !== 0) return; e.stopPropagation(); onSelect(); ed.startGesture(blockId, "resize-y", e); }} />
        </>
      )}
      {fileOver && <span className="sf-drop-hint" aria-hidden="true">{t("Lepas untuk memasang foto")}</span>}
    </div>
  );
}
