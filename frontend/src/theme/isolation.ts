// Isolasi tema: halaman publik situs toko tidak boleh memakai token DASBOR (--ink, --muted,
// --card, --bg, --brand) untuk warna teks/latar.
//
// Dua sistem token ada berdampingan:
//  - Token DASBOR (--ink, --muted, --bg, …) di `:root` → mengikuti tema APLIKASI (OS).
//  - Token SITUS (--s-ink, --s-muted, --s-bg, …) di `.site` → mengikuti brand kit TOKO.
//
// Halaman publik (`/`, `/produk/*`, `/keranjang`, `/pesanan`, `/akun`, `/kebijakan-privasi`,
// `/syarat-transaksi`) dirender di dalam bingkai `.site` yang latarnya mengikuti tema toko
// (mis. putih). Kalau teksnya ikut token dasbor, di mode gelap OS teksnya jadi #d4d4d8 di
// atas putih → kontras 1.48:1 → tidak terbaca.
//
// Aturan yang diuji: untuk setiap kelas yang dipakai komponen halaman publik, aturan CSS-nya
// hanya boleh memakai token situs (--s-*, --shop-*, --chat-*, --pg-*) atau token netral
// (durasi, spasi, radius, z-index, …). Token dasbor hanya boleh muncul sebagai nilai
// cadangan (fallback) di dalam var().

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(__dirname, "..", "..");

/** Token milik situs / komponen — aman di halaman publik. */
const SITE_TOKEN = /^(s-|shop-|chat-|st-|nm-|pg-|font-)/;

/** Token netral (bukan warna) — aman. */
const NEUTRAL_TOKEN =
  /^(z-|dur-|fs-|sp-|radius|ring|shadow|ease|accent|press|mrow|border|bg-surface|text$|primary|primary-light|ds-|sw-|brand-|safe|disable)/;

/** Token dasbor yang WAJIB dilarang bocor ke halaman publik. */
const DASH_FORBIDDEN = /^(ink|ink-2|muted|subtle|bg|bg-subtle|card|overlay|line|line-soft|line-strong|surface-hover|surface-active|brand|brand-hover|brand-ink|brand-solid|brand-soft|brand-ink-soft|danger|danger-bg|danger-line|ok|ok-bg|ok-line|warn|warn-bg|warn-line|info|info-bg|info-line|neutral-badge-bg|neutral-badge-ink)$/;

/** Token dasbor yang boleh muncul sebagai cadangan (fallback) di dalam var(). */
const DASH_FALLBACK_OK = new Set([
  "ink",
  "ink-2",
  "muted",
  "bg",
  "card",
  "line",
  "brand",
  "brand-ink-soft",
]);

/** Berkas CSS yang memuat gaya halaman publik. */
export const CSS_FILES = [
  "src/app/globals.css",
  "src/renderer/site.css",
  "src/shop/shop.css",
  "src/chat/chat.css",
];

/** Komponen yang dirender pada halaman publik (situs toko). */
export const PUBLIC_FILES = [
  "src/shop/views.tsx",
  "src/shop/frame.tsx",
  "src/app/site/page.tsx",
  "src/renderer/StudioSite.tsx",
  "src/renderer/Site.tsx",
  "src/renderer/PublicNav.tsx",
  "src/renderer/SiteFooter.tsx",
  "src/chat/widget.tsx",
  "src/legal/LegalDoc.tsx",
];

function read(rel: string): string | null {
  try {
    return readFileSync(join(ROOT, rel), "utf8");
  } catch {
    return null;
  }
}

/** Semua kelas yang dipakai file TSX halaman publik. */
export function publicClasses(): Set<string> {
  const out = new Set<string>();
  for (const f of PUBLIC_FILES) {
    const src = read(f);
    if (!src) continue;
    for (const m of src.matchAll(/className\s*=\s*(?:"([^"]*)"|'([^']*)'|\{`([^`]*)`\})/g)) {
      const raw = m[1] ?? m[2] ?? m[3] ?? "";
      // Template literal berisi interpolasi, mis. `shop-main${wide ? " wide" : ""}`.
      // Potong dulu di batas interpolasi, lalu ambil potongan statisnya saja.
      for (const part of raw.split(/\$\{[^}]*\}/)) {
        for (const c of part.split(/\s+/)) {
          const clean = c.trim();
          if (!clean || clean.includes("$") || clean.includes("{") || clean.includes("}")) continue;
          out.add(clean);
        }
      }
    }
  }
  return out;
}

/**
 * Nama token CSS yang dipakai LANGSUNG (bukan sebagai fallback) di dalam badan aturan.
 *
 * `var(--a)` → --a dipakai langsung.
 * `var(--a, var(--b))` → --a dipakai langsung; --b ada di posisi fallback (aman, hanya dipakai
 *   bila --a tidak terpasang).
 *
 * Catatan: `matchAll(/var\(--x/)` saja TIDAK cukup — var() bersarang di dalam fallback akan
 * terhitung sebagai match terpisah dan salah flagged. Karena itu dipindai manual: setiap
 * ekspresi var() dipindai utuh, dan posisi fallback dilewati tanpa pernah dihitung.
 */
function directTokens(body: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < body.length) {
    const idx = body.indexOf("var(", i);
    if (idx === -1) break;
    let j = idx + 4;
    while (j < body.length && /\s/.test(body[j]!)) j++;
    if (body[j] !== "-") {
      i = idx + 4;
      continue;
    }
    j++;
    let name = "";
    while (j < body.length && /[a-zA-Z0-9_-]/.test(body[j]!)) {
      name += body[j];
      j++;
    }
    // Buang tanda hubung pertama (nama token CSS: `--s-ink` → `s-ink`).
    if (name.startsWith("-")) name = name.slice(1);    // Lewati seluruh ekspresi var() ini (termasuk var() di dalam fallback).
    let depth = 1;
    while (j < body.length) {
      const c = body[j];
      if (c === "(") depth++;
      else if (c === ")") {
        depth--;
        if (depth === 0) {
          j++;
          break;
        }
      }
      j++;
    }
    if (name) out.push(name);
    i = j;
  }
  return out;
}

export interface Leak {
  cls: string;
  file: string;
  line: number;
  token: string;
  snippet: string;
}

/** Diekspor untuk keperluan self-test. */
export const directTokensForTest = directTokens;

/**
 * Aturan CSS yang memakai token dasbor untuk kelas yang dipakai halaman publik.
 *
 * Token dasbor hanya dianggap bocor bila dipakai LANGSUNG (bukan sebagai fallback kedua
 * di dalam var()). Fallback sah karena: `var(--s-ink-2, var(--ink-2))` memakai --s-ink-2
 * bila tersedia (di dalam bingkai toko) dan --ink-2 hanya sebagai jaring pengaman untuk
 * pemakaian dasbor.
 */
export function audit(): Leak[] {
  const used = publicClasses();
  const hits: Leak[] = [];
  for (const f of CSS_FILES) {
    const src = read(f);
    if (!src) continue;
    src.split("\n").forEach((line, i) => {
      const brace = line.indexOf("{");
      if (brace === -1) return;
      const sel = line.slice(0, brace).trim();
      const body = line.slice(brace);
      if (sel.startsWith("@") || sel.startsWith(":root")) return;

      const bad = directTokens(body).filter(
        (t) => DASH_FORBIDDEN.test(t) && !SITE_TOKEN.test(t) && !NEUTRAL_TOKEN.test(t),
      );
      if (!bad.length) return;

      // Kelas yang dipakai halaman publik (pecah selector majemuk & berdaftar).
      const classes = sel
        .split(",")
        .flatMap((s) => s.trim().split(/\s+/))
        .map((s) => s.replace(/^\./, "").replace(/[>+~].*$/, ""))
        .filter((s) => used.has(s) && !s.startsWith(":") && !s.startsWith("["));
      if (!classes.length) return;

      // Aturan hanya bisa memengaruhi halaman publik bila SEMUA kelas pada rantainya
      // (termasuk kelas nenek moyang) muncul di komponen publik. Contoh:
      // `.d-auth-form .d-narrow > p` memakai `.d-auth-form` (khusus login/register),
      // jadi tidak mungkin menyentuh halaman publik meski `d-narrow` dipakai di sana.
      //
      // Selector elemen (p, h2, a, …) diabaikan — mereka generik dan boleh muncul di mana saja.
      const chainClasses = sel
        .split(",")
        .flatMap((s) => s.trim().split(/[\s>+~]+/))
        .filter((s) => s.startsWith("."))
        .map((s) => s.slice(1))
        .filter((s) => /^[a-zA-Z][\w-]*$/.test(s));
      if (chainClasses.some((c) => !used.has(c))) return;

      hits.push({
        cls: classes.join(" "),
        file: f.replace(/\\/g, "/"),
        line: i + 1,
        token: bad.map((t) => `--${t}`).join(", "),
        snippet: line.trim(),
      });
    });
  }
  return hits;
}
