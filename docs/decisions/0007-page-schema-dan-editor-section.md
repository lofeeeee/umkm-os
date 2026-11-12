---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0007 — Page schema dan editor berbasis section (stacked blocks)

## Context and Problem Statement

D-05 / R-03 (`PRD.md §20 #5`, `PRD.md §4.3`): editor website section-based atau canvas drag-and-drop penuh. Kedua pembacaan mengubah `content_json`, renderer, dan rollback.

## Considered Options

* Section-based: daftar section bertumpuk vertikal (stacked blocks), urutan + enable/disable + data per tipe
* Canvas penuh: posisi bebas per elemen (x/y/size), layout per breakpoint

## Decision Outcome

Chosen option: "Section-based (stacked blocks)", karena sesuai contoh schema `PRD.md §4.3`, renderer dapat dibatasi ke whitelist komponen, dan responsif tanpa layout per breakpoint.

Kontrak `content_json` (`schema_version = 1`):

| Field | Aturan |
|---|---|
| `schema_version` | wajib `1`; versi lain ditolak |
| `template` | key template yang ada di `website_templates` |
| `theme.primary` | `#RRGGBB` |
| `theme.font` | salah satu preset typography (whitelist, bukan nama font bebas) |
| `sections[]` | 0–30 item; urutan array = urutan tampil |
| `sections[].id` | `[a-z0-9-]{1,40}`, unik dalam halaman |
| `sections[].type` | salah satu dari 11 komponen awal `PRD.md §4.3`: `header`, `hero`, `about`, `product_grid`, `promotion`, `gallery`, `testimonials`, `business_hours`, `location`, `contact`, `footer` |
| `sections[].enabled` | boolean; section nonaktif tidak dirender di publik |
| `sections[].data` | objek dengan field tertutup per tipe (`deny_unknown_fields`); teks = plain text dengan batas panjang; URL hanya `https://`, `http://`, `mailto:`, `tel:`, path relatif `/`, atau `asset:<uuid>` milik tenant |

Larangan (`PRD.md §4.3`): tidak ada field HTML/JS/CSS bebas. Renderer menampilkan teks sebagai teks (React escaping), tanpa `dangerouslySetInnerHTML`. Validasi dilakukan di server (Rust) saat simpan draft **dan** saat publish; frontend hanya membantu.

### Consequences

* Good: validasi tertutup mudah di-fuzz; rollback = pindah pointer ke versi immutable; tampilan mobile otomatis.
* Bad: tidak ada penempatan bebas; tipe section baru butuh perubahan kode + `schema_version` bila tidak kompatibel.

## More Information

Status di `OPEN_DECISIONS.md`: D-05 `DISETUJUI` 2026-11-12. Append-only.
