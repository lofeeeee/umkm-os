// Regression test: halaman publik situs toko tidak boleh "mewarisi" tema dasbor.
//
// Bug yang dicegah (UAT 2026-11): `/syarat-transaksi` memakai `.legal-doc p { color: var(--ink-2) }`.
// Di mode gelap OS, --ink-2 = #d4d4d8; latarnya putih (tema toko) → kontras 1.48:1 →
// teks praktis tak terlihat. Karena itu aturan CSS untuk kelas halaman publik hanya boleh
// memakai token SITUS (--s-*, --shop-*, --chat-*), atau token dasbor HANYA sebagai fallback.

import { describe, expect, it } from "vitest";

import { audit, publicClasses } from "./isolation";

describe("isolasi tema halaman publik", () => {
  it("kelas yang dipakai halaman publik terdeteksi", () => {
    const used = publicClasses();
    // Penjaga: kalau daftar file publik berubah, kelas ini harus ikut terdeteksi.
    expect(used.has("legal-doc")).toBe(true);
    expect(used.has("shop-main")).toBe(true);
  });

  it("tidak ada kelas halaman publik yang memakai token dasbor secara langsung", () => {
    const leaks = audit();
    const msg = leaks.map((l) => `${l.file}:${l.line}  .${l.cls}  →  ${l.token}\n    ${l.snippet}`).join("\n");
    expect(
      leaks,
      `Halaman publik memakai token dasbor (--ink/--muted/--card/--bg). Di mode gelap OS teksnya\n` +
        `menjadi terang di atas latar toko (putih) → tidak terbaca. Pakai token situs (--s-*)\n` +
        `dengan fallback token dasbor.\n\n${msg}`,
    ).toEqual([]);
  });

  it("halaman legal memakai token situs lebih dulu (tanda kalimat bukti)", () => {
    const leaks = audit().filter((l) => l.cls.includes("legal-doc") || l.cls.includes("legal-meta"));
    expect(leaks.map((l) => `${l.file}:${l.line} ${l.token}`)).toEqual([]);
  });
});
