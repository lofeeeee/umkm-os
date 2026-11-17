"use client";

// Gambar mode "popup": menempel di pojok layar dan ikut saat halaman digulir; pembeli bisa menutupnya.
// Di Studio tampil di pojok bingkai pratinjau dan bisa dipilih (tanpa tombol tutup yang aktif).

import { useState } from "react";

import { t as tx } from "@/i18n";

import { FreeImage, type LayerBlock } from "./layers";

const CORNERS = ["bottom-right", "bottom-left", "top-right", "top-left"];

export function PopupImage({ b, assetUrl, editing, selected, onSelect }: {
  b: LayerBlock; assetUrl: (ref: string) => string | undefined; editing?: boolean; selected?: boolean; onSelect?: () => void;
}) {
  const [closed, setClosed] = useState(false);
  if (closed && !editing) return null;
  const corner = typeof b.data.corner === "string" && CORNERS.includes(b.data.corner) ? b.data.corner : "bottom-right";
  return (
    <div className={`sv2-popup${selected ? " sf-float-selected" : ""}`} data-corner={corner} data-block-id={editing ? b.id : undefined}
      style={{ ["--pop-w" as string]: `${Math.max(1, Math.min(12, b.w)) * 56}px` }}
      onPointerDownCapture={editing ? onSelect : undefined}>
      <FreeImage b={b} assetUrl={assetUrl} editing={editing} />
      {!editing && (
        <button type="button" className="sv2-popup-close" aria-label={tx("Tutup gambar")} onClick={() => setClosed(true)}>×</button>
      )}
    </div>
  );
}
