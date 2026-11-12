---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0014 — Kontrak frontend: static export di Caddy, data tenant dari API berdasarkan Host

## Context and Problem Statement

R-01 (`PRD.md §9.2`, `PRD.md §4.3`, `PRD.md §9.7`): static export "where feasible" harus berdampingan dengan draft/preview/publish/rollback dan wildcard hostname tenant. Kontrak Host → domain registry → Caddy → API dan selector versi terbit belum ada.

## Considered Options

* Static export (Next.js) dilayani Caddy + data dari API berdasarkan Host (usulan engineering)
* Next.js SSR (Node server di produksi)
* HTML per tenant di-generate saat publish

## Decision Outcome

Chosen option: "Static export dilayani Caddy + data dari API berdasarkan Host". Tidak ada Node server di produksi (`PRD.md §9.2`); budget 8 GB tidak berubah.

| Host (ADR-0001) | Isi | Data |
|---|---|---|
| `app.ryasai.my.id` | build statis dashboard (`frontend/`, Next.js `output: 'export'`) | `https://api.ryasai.my.id` dengan cookie + CSRF (ADR-0015) |
| `{slug}.ryasai.my.id` | build statis renderer situs tenant (satu shell, fallback SPA `try_files {path} /index.html`) | same-origin `/_api/public/*` → Caddy proxy ke API **dengan Host asli dipertahankan** |
| `ryasai.my.id` | marketing (statis) | — |

Aturan kontrak:

1. **Tenant ditentukan hanya oleh Host** lewat resolver P1-4 (`tenant_domains`, status aktif). Browser tidak pernah mengirim `tenant_id`. Domain suspended/disabled/tak dikenal → 404 (TEN-009).
2. Di host tenant, Caddy hanya meneruskan `/_api/public/*`. Endpoint publik hanya-baca, tanpa cookie (cookie sesi `__Host-` terikat `api.` dan tidak pernah terkirim ke host tenant).
3. **Selector versi terbit:** `tenant_websites.published_version_id`. Publik hanya membaca versi yang ditunjuk pointer itu; draft tidak punya jalur publik apa pun.
4. **Preview** hanya di dashboard (`app.`), memakai komponen renderer yang sama dengan data draft dari endpoint terautentikasi. Preview tidak mengubah published version.
5. **Publish** = buat baris `website_page_versions` immutable + pindahkan pointer, dalam satu transaksi. **Rollback** = pindahkan pointer ke versi lama milik tenant yang sama.
6. Header host tenant: `Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https://api.ryasai.my.id data:; frame-ancestors 'none'`.
7. Respons publik membawa `Cache-Control: public, max-age=60`; publish/rollback terlihat ≤ 60 detik.

### Consequences

* Good: frontend statis murah; satu build melayani semua tenant; isolasi tetap di Rust/RLS.
* Bad: SEO terbatas (konten dirender di klien); halaman pertama butuh satu request API. Bila SEO server-side menjadi wajib, buat ADR baru dan hitung ulang budget.

## More Information

Status di `OPEN_DECISIONS.md`: ADR-0014 `DISETUJUI` 2026-11-12. Terkait ADR-0001, ADR-0007, ADR-0015. Append-only.

Klarifikasi implementasi (P2-1, 2026-11-12, tidak mengubah keputusan): versi bersifat per halaman (`PRD.md §5.4` `website_page_versions.page_id`), sehingga pointer butir 3 disimpan di `website_pages.published_version_id` (composite FK ke versi halaman + tenant yang sama).
