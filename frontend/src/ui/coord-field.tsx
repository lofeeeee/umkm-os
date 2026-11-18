"use client";

// Isian koordinat (latitude/bujur) untuk Profil bisnis.
//
// Dua hal yang perlu dijaga di sini:
//  1. **Tanda minus harus bisa diketik.** Angka negatif lazim (Indonesia di belahan bumi
//     selatan: latitude ≈ -6.2). Bila nilai disimpan sebagai `number` saja, mengetik "-"
//     saja menghasilkan `NaN` dan isian ikut menampilkan "NaN" — pengguna mengira minus
//     ditolak. Karena itu teks yang diketik disimpan apa adanya sampai menjadi angka.
//  2. **Rentang dijaga** (latitude -90..90, bujur -180..180) dengan pesan langsung,
//     sama dengan aturan di backend (`validate_coordinates`).
//
// Komponen ini **ter kendali** (controlled): teks isian berasal dari state formulir di
// halaman induk, sehingga tidak ada setState di dalam efek maupun akses ref saat render.

import { useId } from "react";

import { coordInRange, coordNumber } from "@/lib/coords";
import { t as tx } from "@/i18n";

export function CoordField({
  label,
  value,
  range,
  onChange,
  disabled,
  hint,
}: {
  label: string;
  /** Teks yang diketik pengguna (boleh "-" atau "-6."); `null` = kosong. */
  value: string | null;
  range: { min: number; max: number };
  /** Teks baru yang diketik pengguna (sudah dibersihkan). */
  onChange: (v: string | null) => void;
  disabled?: boolean;
  hint?: string;
}) {
  const id = useId();
  const errId = `${id}-err`;
  const num = coordNumber(value);
  const err = coordInRange(num, range)
    ? null
    : tx("{0} harus antara {1} dan {2}.", label, range.min, range.max);

  return (
    <label className="d-field" htmlFor={id}>
      <span id={`${id}-label`}>{label}</span>
      <input
        id={id}
        type="text"
        inputMode="decimal"
        autoComplete="off"
        value={value ?? ""}
        disabled={disabled}
        aria-invalid={err ? true : undefined}
        aria-describedby={err ? errId : hint ? `${id}-hint` : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {hint && !err && <span className="d-hint" id={`${id}-hint`}>{hint}</span>}
      {err && (
        <span className="d-field-error" id={errId} role="alert">
          {err}
        </span>
      )}
    </label>
  );
}
