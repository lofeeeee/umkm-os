---
status: Accepted
date: 2026-11-13
decision-makers: Product owner (pemilik repo)
supersedes: ADR-0007
---

# ADR-0017 — Studio website: kanvas grid 12 kolom + plugin resmi

## Context and Problem Statement

Pemilik produk (2026-11-13) meminta website UMKM "editable seperti Canva": variasi warna, grid, drag & drop,
dan "plugin is everything" — toko mencari plugin (galeri produk, chatbot, dll.) di dasbor, memasangnya, lalu
menarik dan mengubah ukurannya di halaman. ADR-0007 memilih *section bertumpuk* dan secara eksplisit menolak
kanvas bebas.

Keputusan pemilik produk pada sesi yang sama: (1) **grid snap 12 kolom**, bukan posisi piksel bebas;
(2) **plugin resmi saja** (tanpa kode pihak ketiga); (3) **prototipe dulu** sebelum skema & validator server diubah.

## Considered Options

* Tetap ADR-0007 + varian layout per section (paling murah, tanpa drag/resize).
* **Grid snap 12 kolom + katalog plugin resmi** (prototipe dibangun).
* Kanvas bebas x/y per breakpoint (ditolak: dua layout per halaman, mudah rusak di ponsel, validasi sulit).

## Decision Outcome (usulan)

"Grid snap 12 kolom + plugin resmi". Prototipe: `frontend/src/app/(app)/website/studio`, mesin tata letak
`frontend/src/studio/grid.ts` (diuji, termasuk 300 langkah acak tanpa tumpang tindih).

Kontrak `content_json` yang diusulkan (`schema_version = 2`, belum diimplementasikan di server):

| Field | Aturan usulan |
|---|---|
| `theme.primary` | `#RRGGBB` (bebas) |
| `theme.surface` | `light` \| `cream` \| `dark` |
| `theme.font` | preset whitelist (sama dengan v1) |
| `blocks[]` | 0–40 item |
| `blocks[].kind` | whitelist plugin resmi: 9 tipe section v1 (tanpa header/footer) + `chatbot` |
| `blocks[].x,w` | bilangan bulat, `0 ≤ x`, `w ≥ minW(kind)`, `x + w ≤ 12` |
| `blocks[].y,h` | bilangan bulat ≥ 0, `h ≥ minH(kind)`, tidak boleh tumpang tindih (diverifikasi server) |
| `blocks[].style` | `plain` \| `card` \| `tint` \| `dark` |
| `blocks[].layout` | enum per plugin (mis. produk: `grid`/`carousel`/`list`; chatbot: `inline`/`floating`) |
| `blocks[].data` | sama dengan `sections[].data` v1 per tipe (`deny_unknown_fields`, teks polos, URL whitelist) |
| `header`, `footer` | tetap section v1 di luar grid |
| `installed_plugins` | daftar kind yang dipasang toko (katalog), terpisah dari halaman |

Ponsel: blok ditumpuk berurutan (y lalu x) — tidak ada layout ponsel terpisah. Chatbot maksimal satu per halaman;
mode `floating` dirender sebagai widget chat yang ada (ADR-0006), bukan di grid.

### Consequences

* Good: penempatan & ukuran fleksibel; plugin = unit tunggal untuk katalog, editor, dan renderer; tetap tanpa
  HTML/JS bebas (whitelist) sehingga CSP & validasi server ADR-0007/0014 tetap berlaku.
* Bad: perlu validator Rust baru (overlap/bounds), migrasi v1→v2 (konversi otomatis: tiap section = baris penuh),
  renderer publik dua versi selama transisi, dan aturan tinggi blok (konten bisa melebihi `h`).

## Prototipe (2026-11-13) juga menguji

* Galeri 6 template (`frontend/src/studio/templates.ts`) yang langsung diisi data toko.
* Edit teks langsung di kanvas lewat `EditContext` (`frontend/src/renderer/edit.tsx`): `contentEditable="plaintext-only"`,
  batas panjang dari skema field; situs publik tidak memasang konteks sehingga renderer publik tetap teks biasa.
* Seret foto dari komputer ke plugin (unggah ke aset toko yang sama, ADR-0009).
* Undo/Redo dengan penggabungan langkah (`frontend/src/studio/history.ts`) + simpan otomatis.

## Open points sebelum Accepted

1. Disetujui pemilik produk setelah mencoba prototipe.
2. Batas plugin per paket (Starter vs Pro)? — menyentuh ADR-0016.
3. Tinggi blok — usulan dari prototipe: `auto_height: true` (bawaan) = tinggi mengikuti isi di publik; `false` = tinggi `h` tetap. Editor menandai isi yang terpotong.
4. Rollback lintas versi skema (v2 → versi v1 lama).

Append-only: ubah status lewat ADR atau catatan baru.

## Keputusan 2026-11-13 — Accepted

Pemilik produk menyetujui ("Setujui, mulai Fase 1") setelah mencoba prototipe dan membaca
`docs/ux/riset-studio-website-builder.md`. Konsekuensi yang ditetapkan bersama keputusan ini:

* `schema_version = 2` divalidasi server (Rust) berdampingan dengan v1; dokumen v1 tetap sah dan tetap
  dirender publik. Website v1 dikonversi otomatis menjadi v2 saat pertama dibuka di Studio (tanpa migrasi DB).
* Jalur draf → terbit → versi → rollback memakai endpoint yang ada (ADR-0014 butir 3–5) tanpa perubahan tabel.
* Open point 3 diputus: tinggi blok mengikuti isi di publik (`auto_height`, bawaan `true`).
* Open point 2 diputus lewat ADR-0019 (inti gratis, sebagian plugin Pro).
* `<head>` per toko (SEO/Open Graph) lewat ADR-0018.
* Pekerjaan dilacak sebagai Phase 9 di `docs/BUILD_PLAN.md`.
