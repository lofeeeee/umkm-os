---
status: Accepted
date: 2026-11-13
decision-makers: Product owner (pemilik repo)
---

# ADR-0008 — Custom domain di luar V1.0; toko hanya subdomain platform

## Context and Problem Statement

D-06 (`PRD.md §20 #6`, `PRD.md §15`, `PRD.md §4.2`): custom domain butuh verifikasi kepemilikan DNS, TLS per
domain, dan lifecycle anti-takeover (R-04). Pemilik produk memutuskan (2026-11-13): **"Cukup subdomain"**.

## Considered Options

* Subdomain platform saja di V1.0; custom domain fase lanjutan (rekomendasi engineering)
* Custom domain masuk MVP

## Decision Outcome

Chosen option: "Subdomain platform saja di V1.0".

| Aturan | Nilai |
|---|---|
| Alamat toko V1.0 | `{slug}.ryasai.my.id` (ADR-0001) |
| P7-4 custom domain | **di luar V1.0**; tidak dikerjakan |
| Batas paket | `custom_domain = false` di semua paket (data plan, bukan kode) |
| `tenant_domains` | tetap mendukung tipe `subdomain` saja; tipe lain menunggu ADR baru |

### Consequences

* Good: tanpa TLS on-demand/verifikasi DNS per tenant, permukaan serangan takeover tidak ada di V1.0.
* Bad: toko yang ingin domain sendiri harus menunggu fase lanjutan.

## More Information

Status di `OPEN_DECISIONS.md`: D-06 `DISETUJUI` 2026-11-13. Append-only.
