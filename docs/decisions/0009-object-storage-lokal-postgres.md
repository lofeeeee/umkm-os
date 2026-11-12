---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0009 — Object storage: adapter lokal di PostgreSQL (sementara) + signed URL

## Context and Problem Statement

D-07 (`PRD.md §20 #7`, `PRD.md §6.3`, `PRD.md §6.8`): provider S3-compatible dan biayanya. P2-6 butuh upload logo/cover/gambar section. Pemilik produk memutuskan **"local database for now"**: belum memakai provider eksternal.

## Considered Options

* Provider S3-compatible eksternal (rekomendasi awal `OPEN_DECISIONS.md`)
* MinIO di VPS yang sama
* Disk lokal (volume Docker)
* Tabel PostgreSQL (`bytea`) di belakang port storage provider-neutral

## Decision Outcome

Chosen option: "Tabel PostgreSQL di belakang port `ObjectStorage`", karena sesuai keputusan "local database", isolasi tenant ikut RLS yang sudah terbukti di Gate P1, dan file ikut backup terenkripsi P0-10 tanpa layanan tambahan.

| Aturan | Nilai |
|---|---|
| Port | trait `ObjectStorage` (put/get/delete) di `crates/infrastructure/src/storage.rs`; adapter S3 menyusul tanpa mengubah pemanggil |
| Tabel | `stored_objects(tenant_id, id, content_type, byte_size, sha256, bytes, created_at)`, RLS `ENABLE+FORCE` |
| Key | UUID acak; tidak pernah memuat nama file/path asli (`PRD.md §6.8`) |
| Tipe diizinkan | `image/png`, `image/jpeg`, `image/webp`, dideteksi dari magic bytes (bukan header/ekstensi klien); SVG ditolak (bisa memuat skrip) |
| Ukuran | maks 2 MB per file (sama dengan `request_body max_size` Caddy) |
| Kuota | maks 200 objek per tenant sampai D-10 menetapkan limit plan |
| Signed URL | `/files/{object_id}?t={tenant_id}&exp={unix}&sig={hmac}`; HMAC-SHA256 dengan secret server; expiry ≤ 1 jam; tenant dalam signature dicocokkan dengan pemilik objek → URL tenant A tidak dapat dipakai membaca objek tenant B |
| Header saat disajikan | `Content-Type` dari deteksi server, `X-Content-Type-Options: nosniff`, `Content-Disposition: inline`, `Cache-Control: private, max-age` ≤ sisa expiry |

### Consequences

* Good: tanpa biaya/provider baru; RLS + backup + restore drill yang sudah ada mencakup file.
* Bad: database membesar (budget PostgreSQL 2 GB `PRD.md §9.3` harus dipantau); backup harian ikut membesar; tidak ada CDN.
* Tetap terbuka (D-07 bagian provider): salinan backup di luar VPS (`BACKUP_OFFSITE_CMD`) dan RPO 15 menit masih menunggu provider eksternal. Migrasi ke S3 = adapter baru + job salin objek; key UUID tetap.

## More Information

Status di `OPEN_DECISIONS.md`: D-07 `DISETUJUI` sementara 2026-11-12 (adapter lokal); provider eksternal ditunda. Append-only.
