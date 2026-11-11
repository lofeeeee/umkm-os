---
status: Accepted
date: 2026-11-11
decision-makers: Product owner (pemilik repo)
---

# ADR-0001 — Root domain `ryasai.my.id` dan subdomain per tenant

## Context and Problem Statement

D-01 (`PRD.md §20`): brand, root domain, dan format subdomain menentukan sertifikat TLS, Caddy, resolver tenant (`PRD.md §4.2`, `PRD.md §9.7`), dan nilai `tenant_domains.hostname`.

## Decision Outcome

| Host | Fungsi (`PRD.md §9.7`) |
|---|---|
| `ryasai.my.id` | Marketing |
| `app.ryasai.my.id` | Dashboard |
| `api.ryasai.my.id` | Rust API |
| `{slug}.ryasai.my.id` | Website tenant |

**Slug tenant:** huruf kecil `a-z`, angka, `-`; 3–63 karakter; tidak diawali/diakhiri `-`; unik global.

**Subdomain cadangan** (tidak pernah boleh menjadi slug tenant, mencegah penyamaran/takeover):
`app api www admin platform mail smtp imap pop ftp static cdn assets status docs help support blog billing auth login dashboard metrics grafana prometheus ns ns1 ns2 dev staging test` + subdomain apa pun yang **sudah dipakai** layanan lain di `ryasai.my.id`.

### Consequences

* Good: Caddy, config, dan resolver dapat memakai domain nyata; production tidak lagi ditolak oleh guard D-01.
* Bad / terbuka:
  * `ryasai.my.id` mungkin juga dipakai proyek lain (ryasai). Daftar subdomain yang sudah terpakai harus dimasukkan ke daftar cadangan sebelum registrasi tenant dibuka.
  * Sertifikat wildcard `*.ryasai.my.id` membutuhkan DNS-01 challenge → **provider DNS belum diketahui**. TLS produksi tetap nonaktif sampai provider DNS + kredensial API-nya tersedia (item ops baru, bukan keputusan PRD).
  * Staging memakai subdomain terpisah, ditetapkan saat staging dibuat.

## More Information

Status di `OPEN_DECISIONS.md`: D-01 `DISETUJUI` 2026-11-11.
