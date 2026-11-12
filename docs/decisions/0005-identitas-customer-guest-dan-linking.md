---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0005 — Identitas customer: guest checkout + akun opsional, linking setelah verifikasi

## Context and Problem Statement

D-03 (`PRD.md §20 #3`, `PRD.md §4.6`): customer guest saja, wajib akun, atau guest + account linking. Pemilik produk memilih **guest checkout dengan account linking opsional setelah verifikasi** (2026-11-12).

## Considered Options

* Guest-only — `get_customer_orders` (P6) hanya lewat token order
* Registered-only — checkout wajib daftar dulu
* Guest + akun opsional, linking hanya setelah email terverifikasi

## Decision Outcome

Chosen option: "Guest + akun opsional, linking setelah verifikasi".

| Aturan | Nilai |
|---|---|
| Lingkup customer | **per tenant**. Customer toko A tidak terlihat oleh toko B walau email sama (RLS, AC-CRM-002). Customer bukan `users` platform (staf); tabel dan sesi terpisah |
| Guest checkout | tidak perlu akun. Checkout membuat record `customers` baru (nama, email, telepon) atau memakai customer milik akun yang sedang login. **Email guest tidak pernah dipakai untuk menempelkan order ke customer lain** — mengetik email orang lain tidak memberi akses apa pun |
| Akses order guest | `order access token` acak 256-bit dikembalikan sekali saat checkout (disimpan hash SHA-256); URL status order memuatnya. Token hanya membuka order itu (AC-ORD-009) |
| Akun customer | opsional, per tenant, email + password (Argon2id, kebijakan sama dengan staf). Login di subdomain toko; cookie host-only `__Host-umkm_cust` (berbeda dari cookie staf); CSRF sama dengan ADR-0015 |
| Verifikasi | email akun diverifikasi lewat token sekali pakai di outbox. **D-08 masih ditunda** → adapter log-only; verifikasi end-to-end lewat email nyata tetap `Blocked by decision` seperti AUTH-003 |
| Linking | hanya setelah `email_verified_at` terisi: customer guest milik tenant yang sama dengan email (case-insensitive) sama ditautkan ke akun → riwayat ordernya terlihat di akun. Akun belum terverifikasi tidak melihat order guest mana pun (AC-CRM-005). Linking tercatat di audit |
| Data CRM | tags + internal note hanya untuk staf (`customers.read`/`customers.manage`); tidak pernah dikirim ke endpoint publik (AC-CRM-004) |
| Retensi | permintaan hapus → anonimisasi profil + purge +30 hari (ADR-0012); order tetap ada dengan snapshot tanpa PII |

Permission baru (memperluas preset 2026-11-11, dicatat di product decision log): `customers.read` (Owner, Manager, Support Agent, Viewer), `customers.manage` (Owner, Manager).

### Consequences

* Good: checkout tanpa hambatan pendaftaran; riwayat order lintas checkout tersedia bagi yang mau berakun.
* Good: tidak ada pengambilalihan riwayat lewat email yang diketik bebas.
* Bad: customer guest dengan email sama menjadi beberapa record sampai ditautkan; pencarian staf menampilkan semuanya.
* Bad: selama D-08 ditunda, akun customer tidak bisa diverifikasi lewat email nyata → linking hanya teruji dengan adapter log-only.

## More Information

Status di `OPEN_DECISIONS.md`: D-03 `DISETUJUI` 2026-11-12. Append-only.
