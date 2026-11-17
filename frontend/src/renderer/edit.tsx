"use client";

// Edit teks langsung di kanvas Studio (prototipe ADR-0017). Situs publik TIDAK memasang EditContext,
// sehingga <T> hanya merender teks biasa (React escaping) — perilaku publik tidak berubah.
// Di editor, teks menjadi contentEditable="plaintext-only": tidak ada HTML yang bisa masuk.

import { createContext, useContext, type KeyboardEvent } from "react";

import { text } from "./safe";
import type { Json } from "./types";
import { t as tx } from "@/i18n";

export interface EditApi {
  /** `path` relatif terhadap data section, mis. "title", "cta.label", "items.0.quote". */
  commit: (path: string, value: string) => void;
}
export const EditContext = createContext<EditApi | null>(null);

export function T({ path, v, multiline, placeholder }: { path: string; v: Json | undefined; multiline?: boolean; placeholder?: string }) {
  const edit = useContext(EditContext);
  const t = text(v);
  if (!edit) return <>{t}</>;
  const done = (el: HTMLElement) => {
    const next = el.innerText.replace(/ /g, " ").replace(multiline ? /\n{3,}/g : /\s*\n\s*/g, multiline ? "\n\n" : " ").trim();
    if (next !== t) edit.commit(path, next);
    else el.innerText = t;
  };
  return (
    <span
      key={t}
      className="s-edit"
      data-interactive
      data-placeholder={placeholder ?? tx("Ketik di sini…")}
      contentEditable="plaintext-only"
      suppressContentEditableWarning
      spellCheck
      role="textbox"
      aria-multiline={multiline || undefined}
      aria-label={tx("Ubah {0}", path)}
      onBlur={(e) => done(e.currentTarget)}
      onKeyDown={(e: KeyboardEvent<HTMLSpanElement>) => {
        e.stopPropagation();
        if (e.key === "Escape") { e.currentTarget.innerText = t; e.currentTarget.blur(); }
        if (e.key === "Enter" && !multiline) { e.preventDefault(); e.currentTarget.blur(); }
      }}
      style={multiline ? { whiteSpace: "pre-wrap", display: "block" } : undefined}
    >
      {t}
    </span>
  );
}

/** Mode edit aktif? Dipakai renderer untuk menggabung paragraf menjadi satu isian. */
export const useEditing = () => useContext(EditContext) !== null;
