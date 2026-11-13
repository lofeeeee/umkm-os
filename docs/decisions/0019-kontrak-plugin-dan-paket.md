---
status: Accepted
date: 2026-11-13
decision-makers: Product owner (pemilik repo)
---

# ADR-0019 — Kontrak plugin Studio dan pembatasan per paket

## Context and Problem Statement

ADR-0017 menjadikan "plugin is everything" dengan plugin resmi saja. Riset (G-ECO-01..04) menunjukkan plugin butuh
kontrak tertulis — versi, skema data, penempatan, batas per halaman, dan kaitan paket — agar validator server,
editor, dan renderer publik konsisten, dan agar turun paket tidak merusak situs. Shopify app blocks/app embeds
dijadikan rujukan disiplin (target, `enabled_on`, `available_if`, batas jumlah).

## Considered Options

* Inti gratis, sebagian plugin/fitur Pro (usulan engineering)
* Semua plugin untuk semua paket
* Kolom paket disiapkan, nilai ditentukan nanti

## Decision Outcome

Chosen option: "Inti gratis, sebagian Pro" (pemilik produk, 2026-11-13).

Manifest per plugin (sumber kebenaran ganda: `crates/domain/src/website/schema_v2.rs` dan
`frontend/src/studio/plugins.ts`, dijaga tes kontrak):

| Field | Arti |
|---|---|
| `kind` | kunci plugin (whitelist) |
| `version` | versi skema data plugin (bilangan bulat, mulai 1) |
| `placement` | `grid` (kotak di kanvas) atau `embed` (di luar grid: gelembung/bilah) — chatbot & WhatsApp bisa keduanya lewat `layout` |
| `max_per_page` | batas jumlah per halaman (chatbot 1, WhatsApp 1, lainnya 40 total blok) |
| `plan` | `starter` (semua paket) atau `pro` |
| `data` | field tertutup per plugin (`deny_unknown_fields`, teks polos, URL whitelist, aset `asset:<uuid>`) |

Aturan paket:

* Fase 1 Studio: **semua plugin yang ada = `starter`** (Banner, Galeri produk, Chatbot, Promo, Tentang, Teks,
  Galeri foto, Testimoni, Jam buka, Lokasi, Kontak, WhatsApp). Kandidat `pro` berikutnya: formulir, jadwal tampil,
  analitik lanjut, halaman ke-4 dst., domain sendiri.
* Menyimpan draf selalu boleh. **Saat turun paket, blok `pro` tidak dihapus**, hanya tidak dirender publik dan
  ditandai di editor ("Butuh paket Pro"); naik paket lagi → tampil kembali.
* Penegakan di server (renderer publik/endpoint publik) — bukan hanya UI.

### Consequences

* Good: penambahan plugin = perubahan manifest + validator + renderer + tes, terukur.
* Bad: manifest ada di dua bahasa (Rust & TS); tes kontrak wajib menjaga keduanya sinkron.

## More Information

Append-only.
