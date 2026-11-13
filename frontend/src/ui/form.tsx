"use client";

// Error validasi per-isian dari API: tandai input (aria-invalid), tampilkan pesan di bawahnya,
// dan pindahkan fokus ke isian pertama yang salah. Satu pola untuk semua form dasbor.

import { useCallback, useState, type ReactNode, type RefObject } from "react";

import { describe, fieldErrors, fieldLabel } from "@/lib/api";

const errId = (path: string) => `err-${path.replace(/\W+/g, "-")}`;

export function useFieldErrors(formRef: RefObject<HTMLFormElement | null>, labelOf: (path: string) => string = fieldLabel) {
  const [map, setMap] = useState<Map<string, string>>(new Map());
  const [general, setGeneral] = useState<string | null>(null);

  /** Isi dari error API; kembalikan true bila ada error per-isian. */
  const fromError = useCallback((e: unknown) => {
    const fe = fieldErrors(e);
    setMap(fe);
    setGeneral(fe.size > 0 ? null : describe(e));
    if (fe.size > 0) {
      const first = fe.keys().next().value;
      requestAnimationFrame(() => formRef.current?.querySelector<HTMLElement>(`[data-field="${first}"]`)?.focus());
    }
    return fe.size > 0;
  }, [formRef]);
  const clear = useCallback(() => { setMap(new Map()); setGeneral(null); }, []);

  const props = (path: string) => ({
    "data-field": path,
    "aria-invalid": map.has(path) ? (true as const) : undefined,
    "aria-describedby": map.has(path) ? errId(path) : undefined,
  });
  const error = (path: string): ReactNode => {
    const msg = map.get(path);
    return msg ? <span className="d-field-error" id={errId(path)} role="alert">{labelOf(path)}: {msg}</span> : null;
  };
  return { fromError, clear, props, error, count: map.size, general };
}
