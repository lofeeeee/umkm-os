---
status: Accepted
date: 2026-11-11
decision-makers: Product owner (pemilik repo)
---

# ADR-0015 — Sesi dashboard via cookie HttpOnly host-only + CSRF

## Context and Problem Statement

Dashboard `app.ryasai.my.id` memanggil API `api.ryasai.my.id` (ADR-0001). Situs tenant berada di `*.ryasai.my.id`, satu *site* dengan dashboard. `PRD.md §6.1` mensyaratkan cookie Secure/HttpOnly/SameSite + CSRF bila memakai cookie session.

## Considered Options

* Cookie HttpOnly host-only + CSRF
* Bearer token di header `Authorization`
* Keduanya

## Decision Outcome

Chosen option: "Cookie HttpOnly host-only + CSRF".

| Atribut | Nilai | Alasan |
|---|---|---|
| Nama | `__Host-umkm_session` | Prefix `__Host-` memaksa `Secure`, `Path=/`, **tanpa `Domain`** → hanya `api.ryasai.my.id`; situs tenant tidak pernah menerima/menimpa cookie |
| `HttpOnly` | ya | Tidak terbaca JavaScript (mitigasi pencurian via XSS) |
| `Secure` | ya | HTTPS saja |
| `SameSite` | `Lax` | Dashboard dan API satu site; tidak terkirim pada sub-request lintas site |
| Isi | token opak 256-bit; server menyimpan SHA-256 (P1-2) | |
| Masa berlaku | 14 hari (`AuthPolicy`), rotasi saat login/perubahan hak | `PRD.md §6.1` session rotation |

CSRF: semua request yang mengubah state dengan cookie wajib membawa header `X-CSRF-Token` yang cocok dengan token CSRF terikat sesi (synchronizer token), plus pemeriksaan `Origin` terhadap allowlist (`https://app.ryasai.my.id`). Karena SameSite tidak melindungi dari subdomain lain di site yang sama (`*.ryasai.my.id`), pemeriksaan CSRF tetap wajib.

CORS: hanya origin allowlist, `credentials: true`; wildcard origin dilarang.

Lingkungan lokal (HTTP, tanpa TLS): nama cookie tanpa prefix `__Host-` dan tanpa `Secure` hanya bila `environment = development`.

### Consequences

* Good: token sesi tidak dapat dibaca skrip; situs tenant tidak dapat menyentuh cookie API.
* Bad: perlu CSRF + CORS; klien non-browser (mobile/integrasi) butuh mekanisme lain → ADR baru bila diperlukan.

## More Information

Terkait ADR-0014 (kontrak frontend, masih MENUNGGU): keputusan ini hanya mengunci transport sesi API.
