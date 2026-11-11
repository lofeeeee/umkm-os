# Peta Alur dan Requirement — UMKM Business OS

Dokumen ini adalah hasil pembacaan menyeluruh `PRD.md` (1837 baris, SHA-256 `95d7d036dcf83e1d9e73c004cdbb153c159a874c29029186997e6d3cfb8266c4`). Fungsinya: menjadi ringkasan yang **jelas alurnya** sebelum `BUILD_PLAN.md` dipakai untuk eksekusi. Bukan pengganti PRD; bila ada beda, `PRD.md` yang menang.

---

## 1. Empat permukaan produk

| Permukaan | Aktor | Fungsi | Catatan kunci |
|---|---|---|---|
| Public platform | Calon tenant | Marketing, registrasi SaaS | root domain belum final (D-01) |
| Tenant website | Customer / pengunjung | Landing page, katalog, cart, chat widget | hostname dari domain registry; bentuk frontend contingent ADR-0014 |
| Tenant dashboard | Owner, Manager, Admin, Support Agent, Catalog Manager, Viewer | Kelola bisnis, produk, order, tim, tiket, chatbot | semua lewat `/api/v1` |
| Platform administration | Super Admin, Support, Finance | Kelola tenant, paket, domain, usage | platform role terpisah dari tenant role |

**Prinsip yang mengikat (`PRD.md §1.2`):** multi-tenant sejak awal; mobile-first; security & tenant isolation sebagai requirement inti; AI tidak punya otoritas transaksi tanpa validasi backend; modular monolith; adapter eksternal; dapat berkembang tanpa mengganti domain logic.

---

## 2. Alur utama end-to-end

### 2.1 Tenant: daftar sampai website terbit

```text
Register (email + password, Argon2id, email case-insensitive unik)
  -> Email verification (`PRD.md §4.1`; tenant belum publish sebelum ini)
  -> Buat tenant + membership Owner + business profile + konfigurasi default
  -> Generate subdomain unik (slug divalidasi, reserved slug ditolak)
  -> Pilih template website
  -> Edit draft page (JSON schema tervalidasi; bentuk editor menunggu D-05)
  -> Preview mobile/desktop (tidak mengubah published version)
  -> Publish => immutable page version
  -> Tenant website melayani publik melalui hostname terdaftar
  -> Rollback => pilih versi sebelumnya
```

Tenant context berasal dari hostname/session/membership terverifikasi, bukan `tenant_id` dari browser (`PRD.md §4.2`, `PRD.md §5.7`).

### 2.2 Customer: browsing sampai order

```text
Browse katalog publik
  -> Tambah ke cart / ubah quantity
  -> Isi data customer (bentuk identitas menunggu D-03)
  -> Pilih pickup/delivery jika tersedia
  -> Konfirmasi order dengan Idempotency-Key
  -> BEGIN
       validate customer + cart
       lock inventory rows
       SERVER recalculate: harga, promo, ongkir, total
       validate stock
       create order + immutable order_item snapshots
       create inventory_reservations (dengan expiry)
       create order_event
       create transactional outbox event
     COMMIT
  -> Setelah COMMIT: panggil payment provider (tidak menahan row lock saat HTTP eksternal)
  -> Webhook: verify signature -> dedup external_event_id -> update status idempotent
  -> Approval bila policy aktif: reason/actor/timestamp/separation-of-duties
  -> Notifikasi status ke customer
  -> Order selesai / refund dengan audit trail
```

Urutan di atas identik dengan `PRD.md §11.1`: recalculate **di dalam `BEGIN…COMMIT` setelah lock inventory rows**, bukan sebelum transaksi. Status order ada 13 nilai di `PRD.md §4.7`; transisi eksplisit dan divalidasi.

### 2.3 Customer service: chat → AI → tiket → manusia

```text
Customer message (channel menunggu D-04)
  -> Conversation Resolver (conversation/session/message UUID terpisah)
  -> Tenant AI Configuration
  -> Input Validation / Guardrails
  -> Context Builder
  -> Tenant-scoped Retrieval (`PRD.md §8.2`)
  -> LLM Provider Router (OpenAI-compatible, capability registry)
  -> Response Validation (LLM output = untrusted input)
  -> Auto / Assisted / Hybrid / Human Only
  -> Tool call (validasi parameter + permission backend)
  -> request_human_support / handover
  -> Ticket (entity terpisah dari conversation)
  -> Assignment -> public reply / internal note -> SLA -> RESOLVED -> CLOSED / reopen
  -> Conversation Viewer + analytics
```

AI tidak mendapat SQL langsung, unrestricted HTTP, payment credential, user management, atau administrative API (`PRD.md §4.11`, `PRD.md §6.6`). Tool awal ada 10 di `PRD.md §4.11`.

### 2.4 Approval, event, dan platform

```text
Perubahan sensitif -> approval_request -> approval_steps -> decide
  -> cek permission + approver assignment + entity state + tenant context
  -> rejection reason / timeout / delegation / approval_events
  -> baru entity status berubah
```

```text
Use case commit -> notification_outbox (transaksi sama)
  -> Rust Worker -> in-app / email / SSE / WebSocket
  -> idempotent + retry + backoff
```

Redis bukan source of truth untuk order, payment, inventory, ticket, approval, audit (`PRD.md §9.5`). `plans.limits_json` menyimpan limit; usage AI menghitung token dan/atau estimasi biaya (`PRD.md §4.15`).

---

## 3. Model data dan isolasi

- Baseline `PRD.md §5.1`: PostgreSQL shared database + shared schema + `tenant_id` + RLS.
- `PRD.md §5.4`: 64 tabel konseptual = 10 identity/tenant + 8 business/website + 16 commerce/inventory + 8 ticketing/approval + 13 AI/conversation + 9 platform/infrastructure.
- Sebelum migration final, setiap tabel harus punya tipe data, nullability, constraint, index, retention, dan RLS policy.
- RLS `ENABLE+FORCE`; app role bukan superuser dan tidak `BYPASSRLS`; `set_config(..., true)` transaction-local (`PRD.md §5.7`).
- Isolasi mencakup 9 permukaan `PRD.md §6.3`: PostgreSQL/RLS, Redis/cache, object storage, background jobs, SSE/WebSocket, search/RAG, AI context/conversation, analytics, exports/downloads.

---

## 4. Arsitektur teknis

`PRD.md §19` memuat 17 keputusan baseline; `BUILD_PLAN.md` Lampiran B memetakan semuanya ke bagian rencana dan status ADR.

```text
Caddy -> Frontend assets (bentuk final contingent ADR-0014)
Caddy -> Rust Axum API -> PostgreSQL / Redis / S3-compatible / External LLM
                      -> transactional outbox -> Rust Worker -> Email / LLM
```

Backend wajib Rust (`PRD.md §7.1`). Lapisan: `HTTP Handler -> Application Use Case -> Domain Service -> Repository/Port -> Infrastructure Adapter` (`PRD.md §7.2`). API `/api/v1`, response envelope, cursor pagination, public vs admin policy berbeda (`PRD.md §7.4`). SSE + WebSocket dengan auth saat koneksi dan setiap subscription (`PRD.md §7.5`).

---

## 5. Keamanan = release gate (`PRD.md §6`)

| Area | Inti |
|---|---|
| Auth | Argon2id, rate limit, secure cookie/CSRF, session rotation/revoke, reset token hash one-time, MFA Super Admin |
| Authorization | Deny-by-default, granular RBAC, object-level, separation-of-duties, AI tool call lewat permission check yang sama |
| API | TLS, CORS allowlist, body/upload limit, strict schema, timeout/concurrency, rate limit, versioning, idempotency, pagination limit, parameterized query, security headers |
| SSRF custom LLM URL | 9 kontrol di `PRD.md §6.5`: scheme/hostname, HTTPS, tolak private/link-local/multicast/metadata, DNS semua IP, rebinding, redirect, timeout/size/concurrency, trusted internal registry |
| File upload | MIME dari isi, size/dimensi, random key, path traversal, tolak executable, strip metadata, private storage, signed URL short expiry, tenant-scoped |
| Audit | Append-only; actor, tenant, action, resource, timestamp, request ID, before/after summary; tanpa password/API key/full payment credential/message sensitif |
| Backup/DR | Daily full, WAL frequent, 7 daily + 4 weekly, RPO 15 mnt / RTO 4 jam **target**, encrypted, di luar VPS, restore drill bulanan |

---

## 6. Requirement register

| Kelompok | Jumlah | Sumber | Catatan |
|---|---:|---|---|
| `AUTH-001..012` | 12 | `PRD.md §4.1` | 001–010 P0, 011–012 P1 |
| `TEN-001..009` | 9 | `PRD.md §4.2` | 007/008 P1; 009 P0 |
| Fungsional tanpa ID | 13 sub-bab | `PRD.md §4.3`–`PRD.md §4.15` | diturunkan menjadi `FR-...` di FSD |
| `BR-001..020` | 20 | `PRD.md §11` | aturan lintas modul |
| `NFR-001..012` | 12 | `PRD.md §10` | target latency tidak termasuk provider eksternal |
| Acceptance | 6 kelompok | `PRD.md §13.3` | Website punya 5 bullet |
| Isolation tests | 9 butir | `PRD.md §13.2` | release-blocking P1 |
| DoD | 14 butir | `PRD.md §16` | HTTP 200 ≠ selesai |
| Open decisions | 12 | `PRD.md §20` | semua `MENUNGGU` |
| V1.0 checklist | 30 checkbox / 6 bagian | `PRD.md §18` | Lampiran A `BUILD_PLAN.md` |
| Architecture decisions | 17 baris | `PRD.md §19` | Lampiran B `BUILD_PLAN.md` |

**NFR yang sering salah dibaca:** 99.5% adalah objective bukan jaminan (`PRD.md §10`); RPO/RTO harus divalidasi restore drill, bukan otomatis tercapai. Dokumen ini tidak menambahkan klaim standar eksternal yang tidak ada di `PRD.md`.

---

## 7. Urutan dependency (`PRD.md §15`)

```text
Phase 0 Infra -> Phase 1 SaaS [Gate: 9 tes isolasi riil]
 -> Phase 2 Website -> Phase 3 Commerce -> Phase 4 Operations
 -> Phase 5 AI -> Phase 6 Customer service -> Phase 7 Commercialization
 -> Phase 8 Production hardening -> Go-live
```

`PRD.md §17.1` memerintahkan: fondasi tenant/security dulu, core journey end-to-end, jangan implementasikan advanced fitur paralel, pisahkan P0/P1/P2, validasi pilot tenant.

---

## 8. Lima contract question / ambiguitas yang naik ke pemilik produk

| ID | Temuan | Mengapa bukan keputusan engineering | Tindakan |
|---|---|---|---|
| R-01 | `PRD.md §9.2` merekomendasikan static export **where feasible**, sedangkan `PRD.md §4.3` butuh draft/preview/publish/rollback dan `PRD.md §9.7` memakai wildcard hostname. Ini **contract gap** Host/domain-registry/Caddy/API, bukan incompatibilitas statis-vs-wildcard. | Mengubah frontend memengaruhi resource budget 8 GB, TTFB, dan domain routing. | ADR-0014: tetapkan kontrak routing + published-version selector; jangan rekayasa endpoint baru sebelum `DISETUJUI`. |
| R-02 | `PRD.md §4.1` AUTH-003/005/009/010 membutuhkan email provider, tetapi itu masih `PRD.md §20 #8`. | D-08 memblokir uji end-to-end verification/invitation/reset; outbox tetap provider-neutral (`PRD.md §4.13`, `PRD.md §7.2`). | D-08 sebelum P1; jangan mengklaim kontrak vendor berubah. |
| R-03 | `PRD.md §4.3` memakai "section/component" dan `sections[]`, tetapi `PRD.md §20 #5` masih menanyakan section-based vs canvas penuh. | Dua pembacaan sah mengubah `content_json`, renderer, rollback. | D-05 `MENUNGGU`; P2-3 contingent. |
| R-04 | `PRD.md §15` Phase 7 memuat "Custom domains", `PRD.md §20 #6` menanyakan masuk MVP atau lanjutan, `PRD.md §18` tidak mencantumkannya. | Perlu definisi apakah MVP = V1.0/Phase 0–8 atau subset; P7-4 bisa keluar dari V1.0. | D-06 `MENUNGGU`; P7-4 conditional/out-of-V1.0. |
| R-05 | `PRD.md §20 #12` legal/compliance bersifat lintas fase: privacy/terms registrasi (P1), privacy/terms transaksi (P3), consent AI (P5), legal billing (P7), sign-off go-live. | Menyamakan semuanya dengan "go-live" membuat P1/P3/P5/P7 mengabaikan keputusan sebelum fitur terkait. | Pecah D-12 menjadi gate per fase; status tetap `MENUNGGU`. |

Kelimanya **tidak dipecahkan diam-diam**; lihat `OPEN_DECISIONS.md`.

---

## 9. Ringkasan satu paragraf

Satu user mendaftar, memverifikasi email, lalu sistem membuat tenant + Owner membership + business profile + subdomain unik yang hanya dikenali lewat hostname/session terverifikasi. Di dalam transaction-local `app.tenant_id` + RLS, tenant menyusun website dari JSON schema (bentuk editor menunggu D-05), menerbitkan versi immutable, dan melayani customer. Customer browsing katalog lalu checkout dengan idempotency key: di dalam satu transaksi server memvalidasi cart, mengunci stok, menghitung ulang harga, menulis snapshot order, membuat reservasi stok dan outbox event, lalu commit; payment/webhook diproses setelah commit secara idempotent; approval memvalidasi permission/assignment/state/tenant sebelum status berubah. Pertanyaan customer masuk conversation tenant-scoped, melewati guardrails → RAG tenant-scoped → provider adapter → response validation → mode Auto/Assisted/Hybrid/Human → tool call yang diotorisasi backend → handover ke tiket yang lifecycle-nya terpisah. Semua perubahan sensitif masuk audit append-only, semua notifikasi lewat outbox, semua cache/job/storage/SSE/analytics/exports diberi namespace tenant, dan seluruhnya berjalan di Rust modular monolith pada VPS 8 GB, di bawah 12 keputusan produk yang belum final.
