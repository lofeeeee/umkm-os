"use client";

// Pesan validasi bawaan browser mengikuti bahasa browser (sering Inggris: "Please fill out this
// field."). Komponen ini memasang satu listener `invalid` (fase capture, seluruh dokumen) dan
// mengganti pesannya dengan kalimat Indonesia; pesan dibersihkan lagi saat isian diubah.
// UAT 15 Nov: login & register masih menampilkan gelembung validasi berbahasa Inggris.

import { useEffect } from "react";
import { t as tx } from "@/i18n";

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

export function validityMessage(el: Field): string {
  const v = el.validity;
  if (v.valueMissing) {
    if (el instanceof HTMLInputElement && el.type === "checkbox") return tx("Centang kotak ini untuk melanjutkan.");
    if (el instanceof HTMLSelectElement) return tx("Pilih salah satu.");
    return tx("Wajib diisi.");
  }
  if (v.typeMismatch && el instanceof HTMLInputElement) {
    if (el.type === "email") return tx("Masukkan alamat email yang valid, mis. nama@contoh.id.");
    if (el.type === "url") return tx("Masukkan tautan yang valid, diawali https://.");
  }
  if (v.tooShort && "minLength" in el) return tx("Minimal {0} karakter (sekarang {1}).", el.minLength, el.value.length);
  if (v.tooLong && "maxLength" in el) return tx("Maksimal {0} karakter.", el.maxLength);
  if (v.rangeUnderflow && el instanceof HTMLInputElement) return tx("Minimal {0}.", el.min);
  if (v.rangeOverflow && el instanceof HTMLInputElement) return tx("Maksimal {0}.", el.max);
  if (v.stepMismatch) return tx("Nilai tidak sesuai kelipatan yang diizinkan.");
  if (v.badInput) return tx("Isian tidak valid.");
  if (v.patternMismatch) return el.title || tx("Format tidak sesuai.");
  return "";
}

const isField = (t: EventTarget | null): t is Field =>
  t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;

export function ValidityLocalizer() {
  useEffect(() => {
    const onInvalid = (e: Event) => {
      if (!isField(e.target)) return;
      const msg = validityMessage(e.target);
      if (msg) e.target.setCustomValidity(msg);
    };
    const onEdit = (e: Event) => {
      if (isField(e.target)) e.target.setCustomValidity("");
    };
    // Sebelum browser memvalidasi (klik tombol submit / Enter), hapus semua pesan kustom di form itu
    // supaya validitas selalu dihitung ulang — nilai yang diubah lewat state React tanpa event input
    // tidak boleh tertahan oleh pesan lama.
    const reset = (form: HTMLFormElement | null | undefined) => {
      if (!form) return;
      for (const el of Array.from(form.elements)) if (isField(el)) el.setCustomValidity("");
    };
    const onClick = (e: Event) => {
      const t = e.target instanceof Element ? e.target.closest("button, input[type=submit], input[type=image]") : null;
      if (t instanceof HTMLButtonElement || t instanceof HTMLInputElement) reset(t.form);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && e.target instanceof HTMLInputElement) reset(e.target.form);
    };
    document.addEventListener("invalid", onInvalid, true);
    document.addEventListener("input", onEdit, true);
    document.addEventListener("change", onEdit, true);
    document.addEventListener("click", onClick, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("invalid", onInvalid, true);
      document.removeEventListener("input", onEdit, true);
      document.removeEventListener("change", onEdit, true);
      document.removeEventListener("click", onClick, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, []);
  return null;
}
