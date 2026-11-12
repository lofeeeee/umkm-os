# Rencana Pembangunan — UMKM Business OS v1.0

**Status:** Rencana untuk disetujui. Bukan izin untuk mulai implementasi fitur yang masih diblokir keputusan.
**Sumber otoritatif:** `PRD.md` v1.0, SHA-256 `95d7d036dcf83e1d9e73c004cdbb153c159a874c29029186997e6d3cfb8266c4`.
**Dokumen pendamping:** `PRD_FLOW_AND_REQUIREMENTS.md`, `OPEN_DECISIONS.md`, `RTM.md`, `SKILL_INSTALL_REPORT.md`, `docs/evidence/SKILL_SOURCE_MANIFEST.json`.
**Status vocabulary kanonik:** `MENUNGGU` / `DISETUJUI` / `DITOLAK` untuk keputusan; `Planned` / `Blocked by decision` / `Verified` untuk baris requirement RTM. Lifecycle MADR (`Proposed/Accepted/Deprecated/Superseded`) hanya dipakai di dalam template ADR.

---

## 0. Cara membaca rencana ini

| Aturan | Artinya |
|---|---|
| **PRD menang** | Bila rencana dan `PRD.md` beda, `PRD.md` yang berlaku. Rencana hanya memecah urutan kerja, bukan mengubah requirement. |
| **Tidak ada asumsi diam-diam** | 12 keputusan `PRD.md §20` berstatus `MENUNGGU`. Tidak ada perilaku yang diimplementasikan berdasarkan rekomendasi sebelum statusnya `DISETUJUI`. |
| **Gate, bukan kalender** | Urutan ditulis sebagai dependency gate. Tidak ada janji tanggal, minggu, atau hari. |
| **Setiap task = satu PR kecil** | Setiap task menyebut path + kriteria uji; perintah dan `docs/evidence/*` dicatat saat task dijalankan. DoD `PRD.md §16` (14 butir) berlaku per task. |
| **Backend Rust wajib** | Seluruh business logic backend Rust (`PRD.md §19`). Frontend adalah rekomendasi `PRD.md §9.2` yang contingent pada ADR-0014. |
| **Sitasi** | Setiap rujukan section ditulis `NAMA_DOKUMEN §N.M`. Tidak ada sitasi telanjang. |

**Phase 0 tidak dapat dimulai penuh sebelum ADR-0002 + ADR-0013 `DISETUJUI`.** P0-0 repo skeleton netral dapat berjalan lebih dulu; semua task yang menyentuh `crates/*`, `apps/*`, atau bentuk workspace menunggu ADR-0013. P0-6 memakai `<ROOT_DOMAIN>` placeholder sampai D-01 `DISETUJUI` untuk TLS produksi.

---

## 1. Ringkasan eksekutif

`PRD.md §17.1` mengakui scope complexity sebagai risiko tinggi. Rencana memprioritaskan fondasi tenant/isolation/security, lalu satu core journey, baru perluasan operasional/AI/komersial.

Tiga usulan ADR yang **belum `DISETUJUI`**:

1. **ADR-0002** — kapabilitas lintas-cutting (audit, outbox/notifikasi, upload, OpenAPI, observability) dibangun lebih awal karena `PRD.md §16` butir 7/9/11/13/14 berlaku untuk setiap fitur.
2. **ADR-0013** — workspace memakai bentuk `apps/` + `crates/` yang disetujui; jangan membuat crate terlalu kecil tanpa boundary nyata (`PRD.md §7.3`).
3. **ADR-0014** — kontrak frontend: `PRD.md §9.2` merekomendasikan static export "where feasible"; bentuk final (static shell, SSR, Host/domain-registry/Caddy/API contract) harus disetujui sebelum task UI.

---

## 2. Prinsip eksekusi

| # | Prinsip | Turunan konkret |
|---|---|---|
| 1 | Tenant context adalah primitive | Dibangun dari hostname/session/membership terverifikasi (`PRD.md §4.2`, `PRD.md §5.7`); handler tidak menerima `tenant_id` dari body/query sebagai bukti. |
| 2 | Dua lapis isolasi | RLS `ENABLE+FORCE` **dan** repository tenant-scoped; app DB role tanpa `BYPASSRLS` (`PRD.md §5.7`). |
| 3 | Transaksi dulu, HTTP eksternal kemudian | Urutan `PRD.md §11.1`; payment/email/LLM setelah commit; outbox dalam transaksi (`PRD.md §4.13`). |
| 4 | Permission > role | Role = preset; permission = sumber otorisasi (`PRD.md §3.3`). |
| 5 | AI output = untrusted input | Tool allowlist, validasi backend, approval untuk restricted write (`PRD.md §4.11`, `PRD.md §6.6`). |
| 6 | Immutable facts | `order_items`, `website_page_versions`, `audit_logs`, `inventory_movements`, `approval_events`. |
| 7 | Adapter untuk eksternal | Payment, email, LLM, embedding, storage, messaging; domain service tidak memanggil vendor langsung (`PRD.md §7.2`). |
| 8 | Fail-closed CI | Agregasi `if: always()` + cek eksplisit `needs.*.result` (`PRD.md §14.2`). |
| 9 | Bukti, bukan narasi | Setiap klaim lulus menyertakan perintah + exit code di `docs/evidence/*`. |
| 10 | Register, bukan RTM final | `RTM.md` saat ini adalah register satu arah draft. RTM dua arah `FR/AC/TC/API/Evidence` dibuat pada artefak #6 setelah Gate B/P1 dan sebelum implementasi fitur P2+. |

---

## 3. Arsitektur target (contingent)

### 3.1 Runtime baseline (`PRD.md §9.1`, `PRD.md §19`)

```text
Internet -> Firewall -> Caddy
                      |-> Frontend assets (bentuk final contingent ADR-0014)
                      `-> Rust Axum API -> PostgreSQL (RLS + pgvector)
                                       -> Redis
                                       -> S3-compatible storage
                                       -> External LLM/embedding providers
                      PostgreSQL notification_outbox -> Rust Worker -> Email / LLM / jobs
```

Database dan Redis tidak diekspos publik (`PRD.md §9.6`). Secrets di-inject runtime (`PRD.md §6.7`, `PRD.md §14.3`).

### 3.2 Workspace — contingent ADR-0013

```text
umkm-business-os/
├── apps/api/
├── apps/worker/
├── crates/<boundary-yang-disetujui-ADR-0013>/
├── migrations/
├── config/
├── frontend/            # bentuk final contingent ADR-0014
├── docs/
├── tests/
├── .github/workflows/
├── docker-compose.yml
├── Cargo.toml
├── Cargo.lock           # WAJIB di-commit (`PRD.md §7.1`)
└── README.md
```

`PRD.md §7.3` memuat **20 direktori crate + 2 app**. Rencana memakai 15 modul feature yang disebut di sana setelah 5 crate inti dipisah lebih dulu; pemisahan crate final menunggu ADR-0013.

### 3.3 Lapisan (`PRD.md §7.2`)

```text
HTTP Handler -> Application Use Case -> Domain Service -> Repository/Port -> Infrastructure Adapter
```

`domain` tidak meng-import `axum`/`sqlx`; dijaga CI boundary check.

### 3.4 Database

- Semua tabel `PRD.md §5.4` mendapat tipe data, nullability, constraint, index, retention, RLS policy sebelum migration final.
- Composite FK `(tenant_id,id)`, `NUMERIC` uang, `TIMESTAMPTZ`, UUID PK (`PRD.md §5.6`).
- `pgvector` retrieval selalu `WHERE tenant_id = $1` (`PRD.md §8.2`).
- `notification_outbox`, `idempotency_keys`, `webhook_events`, `background_jobs`, `audit_logs` = tabel platform/infra.

### 3.5 Frontend — contract gap R-01 / ADR-0014

`PRD.md §9.2` merekomendasikan Next.js static export **where feasible**; `PRD.md §4.3` menuntut draft/preview/publish/rollback; `PRD.md §9.7` memakai wildcard tenant hostname. Ini **bukan incompatibilitas statis-vs-wildcard**, melainkan contract gap yang harus diselesaikan ADR-0014: routing `Host` → domain registry, Caddy, API tenant-resolution, published-version selector, dan bila perlu SSR dengan budget 8 GB dihitung ulang. **Tidak ada endpoint publik baru yang direkayasa** sampai ADR tersebut `DISETUJUI`.

### 3.6 API (`PRD.md §7.4`)

Prefix `/api/v1`; envelope sukses/gagal; `meta.request_id`; cursor pagination; router public customer dan tenant admin punya authorization policy berbeda; OpenAPI via `utoipa`.

---

## 4. Peta modul → requirement → fase

| Modul | `PRD.md` | Fase | Catatan |
|---|---|---|---|
| `shared`, `auth`, `tenant`, `rbac` | `PRD.md §3`, `PRD.md §4.1`, `PRD.md §4.2`, `PRD.md §5.7` | 0–1 | RLS, resolver, RBAC |
| `business`, `website` | `PRD.md §4.3`, `PRD.md §4.4` | 2 | JSON schema, immutable version |
| `catalog`, `inventory`, `crm`, `commerce` | `PRD.md §4.5`–`PRD.md §4.7`, `PRD.md §11.1` | 3 | ledger, reservation, snapshot |
| `approvals`, `ticketing`, `notifications`, `audit` | `PRD.md §4.8`, `PRD.md §4.9`, `PRD.md §4.13`, `PRD.md §6.9` | 4 (capability dasar 0–1) | engine reusable, outbox |
| `ai_orchestrator`, `rag`, `provider_adapter` | `PRD.md §4.10`–`PRD.md §4.12`, `PRD.md §8` | 5 | SSRF, tools, RAG |
| `conversation`, `inbox`, `handover` | `PRD.md §4.12`, `PRD.md §7.5` | 6 | SSE + WebSocket |
| `billing`, `analytics` | `PRD.md §4.14`, `PRD.md §4.15`, `PRD.md §12` | 7 | `limits_json`, metering |
| hardening | `PRD.md §6`, `PRD.md §10`, `PRD.md §13`, `PRD.md §14`, `PRD.md §18` | 8 | ASVS, load, restore |

---

## 5. Rencana bertahap

### Phase 0 — Infrastructure foundation

**Gate awal:** ADR-0002 dan ADR-0013 `DISETUJUI` sebelum P0-1; D-01 `DISETUJUI` sebelum TLS produksi P0-6. P0-0 adalah skeleton netral yang tidak mengunci bentuk crate.

| Task | File | Uji / bukti |
|---|---|---|
| P0-0 Repo skeleton netral: README, `.gitignore`, config loader, CI placeholder, docs dirs | `README.md`, `.gitignore`, `config/`, `.github/workflows/.gitkeep`, `docs/` | tree check + `docs/evidence/P0-0.txt` |
| P0-1 Init Rust workspace setelah ADR-0013 | `Cargo.toml`, `Cargo.lock`, `apps/*/src/main.rs`, `crates/*/src/lib.rs` | `cargo check --workspace && cargo test --workspace`, exit 0； bukti: `docs/evidence/<task>.txt`|
| P0-2 Bootstrap config 3 environment tanpa secret | `config/*.toml`, `crates/shared/src/config.rs` | config test + secret grep； bukti: `docs/evidence/<task>.txt`|
| P0-3 Axum skeleton `/healthz`, `/readyz`, request ID, tracing, error envelope | `apps/api/src/bootstrap.rs`, `apps/api/src/router.rs`, `crates/shared/src/http.rs` | `tests/integration/health.rs`； bukti: `docs/evidence/<task>.txt`|
| P0-4 SQLx migration harness + `set_config(..., true)` | `migrations/0001_extensions.sql`, `crates/infrastructure/src/db.rs` | `sqlx migrate run` + unit helper； bukti: `docs/evidence/<task>.txt`|
| P0-5 Redis namespacing helper | `crates/infrastructure/src/redis.rs` | unit key builder + collision test； bukti: `docs/evidence/<task>.txt`|
| P0-6 Docker Compose dengan `<ROOT_DOMAIN>` placeholder | `docker-compose.yml`, `Dockerfile*`, `ops/caddy/Caddyfile` | `docker compose config -q`; TLS produksi menunggu D-01； bukti: `docs/evidence/<task>.txt`|
| P0-7 CI fail-closed | `.github/workflows/ci.yml` | negative control: required check sengaja gagal → gate merah → pulih → hijau； bukti: `docs/evidence/<task>.txt`|
| P0-8 16 metrics `PRD.md §9.8` + structured log | `crates/shared/src/metrics.rs`, `ops/monitoring/` | `/metrics` memuat metric wajib； bukti: `docs/evidence/<task>.txt`|
| P0-9 OpenAPI generator + contract scaffold | `docs/api/openapi.yaml`, `crates/*/src/http.rs` | schema diff kosong； bukti: `docs/evidence/<task>.txt`|
| P0-10 Backup automation + restore script | `ops/backup/*`, `docs/governance/runbook-restore.md` | restore ke DB kosong; RPO/RTO belum diklaim； bukti: `docs/evidence/<task>.txt`|

**Gate P0:** ADR-0013 + D-01 (TLS produksi) + seluruh task + satu perintah `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && docker compose config -q && sqlx migrate run`, exit 0.

### Phase 1 — SaaS foundation 🔒 **GATE ISOLASI**

**Blokir:** D-01, D-08, D-11; D-12 privacy/terms untuk registrasi sebelum task auth/email. Lihat R-05.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P1-1 Skema identity 10 tabel `PRD.md §5.4` | AUTH/TEN/BR | `migrations/0002_identity.sql` | up/down + constraint； bukti: `docs/evidence/<task>.txt`|
| P1-2 Argon2id, token hash, rate limit, session revoke | `PRD.md §4.1`, `PRD.md §6.1` | `crates/domain/src/auth.rs`, `crates/infrastructure/src/auth_repo.rs` | `tests/security/auth.rs`； bukti: `docs/evidence/<task>.txt`|
| P1-3 Register → tenant + Owner + profile + subdomain | `PRD.md §4.2` | `crates/application/src/onboarding.rs` | acceptance `PRD.md §13.3` Registration； bukti: `docs/evidence/<task>.txt`|
| P1-4 Tenant resolver dari `Host` | `PRD.md §4.2`, `PRD.md §9.7` | `apps/api/src/middleware/tenant.rs` | spoof + suspended domain； bukti: `docs/evidence/<task>.txt`|
| P1-5 RBAC deny-by-default + object-level | `PRD.md §3`, `PRD.md §6.2` | `crates/application/src/authz.rs` | 6 role × 33 permission key； bukti: `docs/evidence/<task>.txt`|
| P1-6 Email verification/invitation/reset via outbox provider-neutral | `PRD.md §4.13` | `apps/worker/src/jobs/email.rs`, `migrations/0003_outbox.sql` | D-08 hanya menentukan adapter; outbox tetap idempotent； bukti: `docs/evidence/<task>.txt`|
| P1-7 RLS `ENABLE+FORCE` | `PRD.md §5.7` | `migrations/0004_rls.sql`, `crates/infrastructure/src/db.rs` | app role tanpa `BYPASSRLS`； bukti: `docs/evidence/<task>.txt`|
| P1-8 Audit append-only + redaction | `PRD.md §6.9` | `crates/domain/src/audit.rs`, `migrations/0005_audit.sql` | tidak ada secret di log； bukti: `docs/evidence/<task>.txt`|
| P1-9 Retention + purge + deletion scaffold | `PRD.md §5.2`, `PRD.md §4.6` | `migrations/0006_retention.sql`, `apps/worker/src/jobs/purge.rs` | D-11 menentukan angka； bukti: `docs/evidence/<task>.txt`|
| P1-10 **Jalur minimum worker + realtime nyata** untuk 9 tes `PRD.md §13.2` | `PRD.md §13.2` | `apps/worker/src/jobs/tenant_guard_probe.rs`, `apps/api/src/realtime/tenant_guard.rs`, `tests/tenancy/*` | tes riil + negative control; stub tidak boleh lulus gate； bukti: `docs/evidence/<task>.txt`|

**Gate P1:** 9 tes isolasi riil lulus; connection reuse aman; app DB role tidak bypass RLS; `docs/evidence/phase-1-isolation.txt`. **Tidak ada Phase 2 sebelum gate ini.**

### Phase 2 — Website builder

**Blokir:** D-05 (R-03), D-07, D-01, ADR-0014 (R-01). Semua task UI contingent.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P2-1 Schema + tabel website | `PRD.md §4.3`, `PRD.md §5.4` | `migrations/0007_website.sql`, `crates/domain/src/website/schema.rs` | fuzz + arbitrary HTML/JS ditolak； bukti: `docs/evidence/<task>.txt`|
| P2-2 Template catalog + preview + versioning | `PRD.md §4.3` | `apps/api/src/website/templates.rs`, `frontend/src/templates/` | preview tidak mengubah published； bukti: `docs/evidence/<task>.txt`|
| P2-3 Contingent D-05: section-based editor | `PRD.md §4.3`, `PRD.md §20 #5` | `frontend/src/editor/sections/`, `crates/application/src/website/editor.rs` | jangan mulai sebelum D-05; cabang canvas penuh didesain bila dipilih； bukti: `docs/evidence/<task>.txt`|
| P2-4 Draft/Preview/Publish/Rollback | `PRD.md §13.3` | `crates/application/src/website/publish.rs`, `frontend/src/preview/` | acceptance 5 skenario； bukti: `docs/evidence/<task>.txt`|
| P2-5 Business profile/hours/address | `PRD.md §4.4` | `migrations/0008_business.sql`, `apps/api/src/business/profile.rs` | timezone + exception hours； bukti: `docs/evidence/<task>.txt`|
| P2-6 Object storage adapter | `PRD.md §6.8` | `crates/infrastructure/src/storage.rs`, `apps/api/src/upload.rs` | malicious upload + signed URL lintas tenant ditolak； bukti: `docs/evidence/<task>.txt`|
| P2-7 Public routing + responsive page | `PRD.md §4.3`, `PRD.md §9.2`, `PRD.md §9.7` | `ops/caddy/Caddyfile`, `frontend/src/tenant/` | 375/768/1024/1440; NFR-003/004 memakai env + dataset + threshold yang disetujui； bukti: `docs/evidence/<task>.txt`|

### Phase 3 — Commerce

**Blokir:** D-02, D-03, D-12 privacy/terms untuk transaksi publik (P3) — **semua diputus 2026-11-12** (ADR-0004 QRIS statis + verifikasi manual; ADR-0005 guest + linking setelah verifikasi; D-12 transaksi disetujui bersyarat). Cabang terpilih: P3-3 linking, P3-7 manual (tanpa webhook).

| Task | Requirement | File | Uji |
|---|---|---|---|
| P3-1 Catalog + inventory tables | `PRD.md §4.5` | `migrations/0009_catalog.sql`, `crates/domain/src/catalog/` | composite FK + SKU unik； bukti: `docs/evidence/<task>.txt`|
| P3-2 Inventory ledger + reservation | `PRD.md §4.5`, `PRD.md §11.1` | `crates/domain/src/inventory/ledger.rs`, `crates/infrastructure/src/inventory_repo.rs` | overselling fixture terpisah dari angka benchmark PRD； bukti: `docs/evidence/<task>.txt`|
| P3-3 **Contingent D-03** customer identity | `PRD.md §4.6`, `PRD.md §20 #3` | `migrations/0010_customers.sql`, `crates/application/src/crm/identity.rs` | cabang guest-only / registered-only / linking didokumentasikan sebelum implementasi； bukti: `docs/evidence/<task>.txt`|
| P3-4 Cart/order/payment/refund tables | `PRD.md §4.7` | `migrations/0011_commerce.sql`, `crates/domain/src/commerce/` | snapshot immutable； bukti: `docs/evidence/<task>.txt`|
| P3-5 Checkout transaction boundary | `PRD.md §11.1` | `crates/application/src/commerce/checkout.rs` | 4 acceptance Commerce; duplicate idempotency key → 1 order； bukti: `docs/evidence/<task>.txt`|
| P3-6 Order state machine 13 status | `PRD.md §4.7` | `crates/domain/src/commerce/state.rs` | transisi invalid ditolak； bukti: `docs/evidence/<task>.txt`|
| P3-7 **Contingent D-02** payment adapter | `PRD.md §18`, `PRD.md §20 #2` | `crates/infrastructure/src/payment/adapter.rs`, `apps/api/src/payments/webhook.rs` | manual/gateway cabang; webhook signature + dedup； bukti: `docs/evidence/<task>.txt`|
| P3-8 Order approval | `PRD.md §4.7`, `PRD.md §4.9` | `crates/application/src/approvals/order.rs` | separation-of-duties； bukti: `docs/evidence/<task>.txt`|

### Phase 4 — Operations

**Status:** Gate P4 lulus 2026-11-12 (`docs/evidence/phase-4-operations.txt`). MFA (AUTH-011/012) belum — menunggu keputusan enkripsi/dependency.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P4-1 Multi-admin + permission management | `PRD.md §3`, `PRD.md §4.9` | `apps/api/src/team/`, `crates/application/src/authz.rs` | permission matrix； bukti: `docs/evidence/<task>.txt`|
| P4-2 Reusable approval engine | `PRD.md §4.9` | `crates/domain/src/approvals/engine.rs`, `migrations/0012_approvals.sql` | 6 entity types; bypass ditolak； bukti: `docs/evidence/<task>.txt`|
| P4-3 Ticket lifecycle | `PRD.md §4.8` | `crates/domain/src/ticketing/`, `migrations/0013_tickets.sql` | acceptance Ticketing； bukti: `docs/evidence/<task>.txt`|
| P4-4 Notifications + outbox delivery | `PRD.md §4.13` | `apps/worker/src/jobs/notification.rs`, `migrations/0014_notifications.sql` | 9 event; worker crash; isolasi sudah lulus P1-10； bukti: `docs/evidence/<task>.txt`|
| P4-5 Audit log **10 event minimum** | `PRD.md §6.9` | `crates/domain/src/audit.rs`, `apps/api/src/audit/` | append-only + redaction； bukti: `docs/evidence/<task>.txt`|
| P4-6 SSE/WebSocket TenantGuard penuh | `PRD.md §7.5`, `PRD.md §6.3` | `apps/api/src/realtime/`, `crates/infrastructure/src/realtime.rs` | regresi isolasi dari P1-10； bukti: `docs/evidence/<task>.txt`|

### Phase 5 — AI platform

**Blokir:** D-09, D-11 untuk retention conversation/knowledge, D-12 consent AI.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P5-1 AI tables | `PRD.md §5.4` | `migrations/0015_ai.sql`, `crates/domain/src/ai/` | composite FK； bukti: `docs/evidence/<task>.txt`|
| P5-2 Envelope encryption + rotation | `PRD.md §4.10`, `PRD.md §6.7` | `crates/infrastructure/src/crypto/`, `apps/api/src/ai/providers.rs` | secret tidak terekspos； bukti: `docs/evidence/<task>.txt`|
| P5-3 SSRF guard 9 kontrol | `PRD.md §6.5` | `crates/infrastructure/src/http_client/ssrf.rs` | private IP, metadata, DNS rebinding, redirect； bukti: `docs/evidence/<task>.txt`|
| P5-4 OpenAI-compatible adapter + capability registry | `PRD.md §8.1` | `crates/infrastructure/src/ai/provider_adapter.rs` | mock capability matrix； bukti: `docs/evidence/<task>.txt`|
| P5-5 Orchestration + guardrails | `PRD.md §4.11`, `PRD.md §6.6` | `crates/application/src/ai/orchestrator.rs` | prompt injection; unauthorized tool ditolak； bukti: `docs/evidence/<task>.txt`|
| P5-6 Tool registry 10 tool | `PRD.md §4.11` | `crates/application/src/ai/tools.rs` | parameter validation + permission； bukti: `docs/evidence/<task>.txt`|
| P5-7 Tenant-scoped RAG | `PRD.md §8.2` | `crates/infrastructure/src/rag/`, `migrations/0016_pgvector.sql` | Tenant A tidak menerima chunk Tenant B； bukti: `docs/evidence/<task>.txt`|
| P5-8 Usage metering + fallback | `PRD.md §4.10`, `PRD.md §4.15` | `crates/application/src/ai/usage.rs`, `crates/infrastructure/src/ai/circuit_breaker.rs` | provider timeout → API utama tetap hidup； bukti: `docs/evidence/<task>.txt`|
| P5-9 Mode Auto/Assisted/Hybrid/Human | `PRD.md §4.10` | `crates/domain/src/ai/mode.rs` | matrix 4 mode； bukti: `docs/evidence/<task>.txt`|

### Phase 6 — Customer service

**Blokir:** D-04 messaging channel, D-03 untuk `get_customer_orders`.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P6-1 Conversation/session/message UUID | `PRD.md §4.12` | `migrations/0017_conversations.sql`, `crates/domain/src/conversation/` | uniqueness + tenant scope； bukti: `docs/evidence/<task>.txt`|
| P6-2 **Contingent D-04** chat channel + SSE/WS | `PRD.md §4.12`, `PRD.md §7.5` | `frontend/src/chat/`, `apps/api/src/realtime/chat.rs` | 20 chat session hanyalah skenario benchmark `PRD.md §13.4`, bukan kapasitas yang dijamin； bukti: `docs/evidence/<task>.txt`|
| P6-3 Conversation Viewer + masking | `PRD.md §4.12` | `frontend/src/inbox/conversation.tsx`, `apps/api/src/inbox/viewer.rs` | Viewer role tidak melihat field sensitif； bukti: `docs/evidence/<task>.txt`|
| P6-4 Handover + human reply + chat approval | `PRD.md §4.10`, `PRD.md §4.12` | `crates/application/src/ai/handover.rs`, `frontend/src/inbox/` | takeover menghentikan auto-response； bukti: `docs/evidence/<task>.txt`|
| P6-5 Admin inbox + ticket creation | `PRD.md §4.8`, `PRD.md §4.12` | `frontend/src/inbox/`, `crates/application/src/ticketing/from_conversation.rs` | ticket != conversation； bukti: `docs/evidence/<task>.txt`|
| P6-6 Conversation analytics | `PRD.md §4.12`, `PRD.md §4.14` | `apps/api/src/analytics/conversations.rs` | tenant-scoped aggregation； bukti: `docs/evidence/<task>.txt`|

### Phase 7 — Commercialization

**Blokir:** D-06 custom domain (R-04), D-10 pricing, D-12 legal billing. P7-4 **di luar V1.0** bila D-06 menolak custom domain masuk MVP.

| Task | Requirement | File | Uji |
|---|---|---|---|
| P7-1 Plan configuration | `PRD.md §4.15`, `PRD.md §12` | `migrations/0018_billing.sql`, `crates/domain/src/billing/plan.rs` | ubah `limits_json` tanpa rebuild； bukti: `docs/evidence/<task>.txt`|
| P7-2 Backend limit enforcement | `PRD.md §4.15`, `PRD.md §12` | `crates/application/src/billing/limits.rs` | limit breach ditolak； bukti: `docs/evidence/<task>.txt`|
| P7-3 Subscription + billing adapter contingent D-10 | `PRD.md §4.15`, `PRD.md §3.1` | `apps/api/src/billing/`, `crates/infrastructure/src/billing/adapter.rs` | plan downgrade； bukti: `docs/evidence/<task>.txt`|
| P7-4 **Conditional/out-of-V1.0** custom domain | `PRD.md §15`, `PRD.md §20 #6`, `PRD.md §4.2` | `apps/api/src/domains/`, `ops/caddy/custom_domains/`, `migrations/0019_domains.sql` | hanya bila D-06 `DISETUJUI` untuk MVP； bukti: `docs/evidence/<task>.txt`|
| P7-5 Analytics tenant + platform | `PRD.md §4.14` | `apps/api/src/analytics/`, `crates/application/src/analytics/` | platform aggregate hanya platform role； bukti: `docs/evidence/<task>.txt`|

### Phase 8 — Production hardening 🔒 **GO-LIVE GATE**

| Task | Requirement | File | Uji |
|---|---|---|---|
| P8-1 ASVS + threat model + pentest | `PRD.md §6`, `PRD.md §17` | `docs/security/`, `docs/evidence/P8-1.txt` | zero critical/high； bukti: `docs/evidence/<task>.txt`|
| P8-2 Load 5 skenario benchmark | `PRD.md §13.4` | `tests/load/`, `docs/evidence/P8-2.txt` | latency/error/CPU/RAM/DB pool/provider latency; bukan kapasitas yang dijamin； bukti: `docs/evidence/<task>.txt`|
| P8-3 DB optimization + NFR-005 | `PRD.md §10`, `PRD.md §9.4` | `docs/evidence/P8-3.txt` | env + dataset + threshold + denominator disetujui； bukti: `docs/evidence/<task>.txt`|
| P8-4 Backup restore drill | `PRD.md §6.10` | `ops/backup/`, `docs/evidence/P8-4.txt` | RPO/RTO target divalidasi； bukti: `docs/evidence/<task>.txt`|
| P8-5 Production hardening | `PRD.md §14.3` | `ops/`, `docs/governance/runbook-rollback.md` | checklist + rollback drill； bukti: `docs/evidence/<task>.txt`|
| P8-6 Runbooks + **30 butir V1.0 checklist** + DoD | `PRD.md §18`, `PRD.md §16` | `docs/governance/v1-release-checklist.md`, `docs/evidence/` | tabel 30 baris Lampiran A tercentang； bukti: `docs/evidence/<task>.txt`|
| P8-7 E2E + pilot tenant | `PRD.md §17.1` | `tests/e2e/`, `docs/evidence/P8-7.txt` | sign-off product； bukti: `docs/evidence/<task>.txt`|

**Gate Go-live:** seluruh 12 keputusan `DISETUJUI`/`DITOLAK` dengan konsekuensi tercatat; D-12 sign-off legal; gate P0–P7 lulus; **30/30** checklist `PRD.md §18`; bukti perintah tersimpan.

---

## 6. Ketergantungan keputusan

| Keputusan | Harus diputus sebelum | Mengapa |
|---|---|---|
| ADR-0002, ADR-0013 | P0-1 | bentuk workspace & urutan fondasi |
| D-01 | P0-6 TLS produksi + P1 | root domain/hostname |
| D-08, D-11, D-12 privacy/terms | P1 | email adapter, retention, registrasi |
| D-12 consent AI | P5 | consent chatbot/AI |
| D-12 legal billing | P7 | terms billing |
| D-12 sign-off | Go-live | legal akhir |
| D-05, D-07, ADR-0014 | P2 | editor, storage, frontend contract |
| D-02, D-03 | P3 | payment, customer identity |
| D-09 | P5 | AI key policy |
| D-04 | P6 | messaging channel |
| D-06, D-10 | P7 | custom domain, pricing |

Semua status awal `MENUNGGU`; lihat `OPEN_DECISIONS.md`.

---

## 7. Artefak engineering

| # | Artefak | Path | Isi minimum | Status |
|---|---|---|---|---|
| 1 | Product decision log + 14 ADR | `docs/decisions/` | MADR 4.0.0 append-only | `MENUNGGU` |
| 2 | Design doc + C4 + STRIDE + rollback | `docs/architecture/` | `PRD.md §7.2`, `PRD.md §6`, observability | Planned |
| 3 | FSD dengan `FR-...`/`AC-...` | `docs/requirements/02-FSD.md` | menurunkan `PRD.md §4.3`–`PRD.md §4.15` | Done 2026-11-12 (73 FR / 83 AC) |
| 4 | OpenAPI 3.x | `docs/api/openapi.yaml` | envelope, error, pagination | Planned |
| 5 | Test Plan + Exit Criteria | `docs/test/` | 10 layer `PRD.md §13.1`, 9 isolasi `PRD.md §13.2` | Done 2026-11-12 (`docs/test/test-plan.md`) |
| 6 | **RTM dua arah final** | `docs/RTM.md` | `FR → AC → TC → API → ADR → Evidence` + CI guard fail-closed | Done 2026-11-12 (`scripts/check-rtm.py`) |
| 7 | Repository governance | `README.md`, `CODEOWNERS`, `CHANGELOG.md`, `docs/governance/` | canonical path | Planned |
| 8 | UI/UX spec | `docs/architecture/ui-ux-spec.md` | state matrix | Planned |
| 9 | V1.0 checklist 30 butir | `docs/governance/v1-release-checklist.md` | Lampiran A | Planned |

---

## 8. Strategi pengujian

| Layer | Scope (`PRD.md §13.1`) | Gate |
|---|---|---|
| Unit | Domain rules, state transitions, schema | `cargo test --workspace` |
| Integration | PostgreSQL, Redis, API, migrations | testcontainers per PR |
| Contract | OpenAPI drift | schema diff gagal bila drift |
| Security | Auth, RBAC, RLS, SSRF, upload, redaction | zero critical/high |
| Tenant isolation | 9 butir `PRD.md §13.2` | release-blocking P1, jalur riil |
| Workflow | Order, ticket, approval, outbox crash | E2E per phase |
| AI | Adapter, tools, guardrails, RAG scope | mock provider matrix |
| Load | 5 skenario `PRD.md §13.4` | benchmark, bukan kapasitas yang dijamin |
| Recovery | Backup restore | drill |
| E2E | Customer + admin journey | sebelum go-live |

Fail-closed CI mengikuti `PRD.md §14.2`. Negative control wajib untuk setiap guard.

---

## 9. Risiko → mitigasi → bukti

| Risiko (`PRD.md §17`) | Mitigasi | Bukti |
|---|---|---|
| Tenant data leakage | P1-7 + P1-10 + TenantGuard | `docs/evidence/phase-1-isolation.txt` |
| VPS exhaustion | P0-6 limits + P0-8 metrics + P8-2 | resource report |
| LLM outage | adapter + circuit breaker | provider-failure test |
| AI cost | token/cost quota | usage report |
| Overselling | row lock + reservation | overselling fixture |
| Duplicate payment | idempotency + webhook dedup | replay test |
| Prompt injection | guardrails + tool validation | injection suite |
| Malicious provider URL | 9 kontrol SSRF | SSRF suite |
| Database loss | external backup + WAL + restore | restore row-count |
| Domain takeover | resolver + conditional P7-4 | spoof/removal |
| Duplicate jobs | outbox idempotent | worker crash |
| Privilege abuse | RBAC + approval + audit | permission matrix |
| Scope complexity | gate + pilot tenant | sign-off |

---

## 10. Definition of Done — 14 butir `PRD.md §16`

```text
[ ] 1 Functional requirements (FR/AC tertaut di RTM final)
[ ] 2 Authorization + object-level check
[ ] 3 Tenant isolation test bila relevan
[ ] 4 Migration + constraint + RLS
[ ] 5 Input validation
[ ] 6 Error handling tanpa stack trace/SQL/secret
[ ] 7 Audit log untuk tindakan sensitif
[ ] 8 Unit + integration tests
[ ] 9 API docs tidak drift
[ ] 10 UI loading/error/empty states
[ ] 11 Tidak ada secret di log/response/trace
[ ] 12 Zero critical/high sebelum production
[ ] 13 Pengukuran resource performance-critical
[ ] 14 Acceptance test berulang + exit code
```

---

## 11. Urutan berbasis gate

```text
Gate A0: P0-0 repo skeleton netral (tanpa bentuk crate)
Gate A: ADR-0002 + ADR-0013 DISETUJUI
  -> P0-1..P0-10; D-01 DISETUJUI sebelum TLS produksi
  -> Gate P0

Gate B: D-01, D-08, D-11, D-12 privacy/terms selesai
  -> P1-1..P1-10 -> Gate isolasi riil
  -> FSD/AC/TC/API/RTM final + OpenAPI + Test Plan (setelah Gate B, sebelum P2+)

Gate C: D-05, D-07, ADR-0014 -> P2   (DISETUJUI 2026-11-12; D-07 sementara lokal)
Gate D: D-02, D-03, D-12 privacy/terms transaksi (P3) -> P3   (DISETUJUI 2026-11-12; QRIS statis ADR-0004, guest+linking ADR-0005; Gate P3 lulus 2026-11-12, docs/evidence/phase-3-commerce.txt)
Gate E: D-09, D-12 consent AI -> P5
Gate F: D-04 -> P6
Gate G: D-06, D-10, D-12 billing -> P7 (P7-4 conditional)
Gate H: P8 + 30/30 V1.0 checklist + D-12 sign-off -> Go-live
```

**Larangan:** memilih cabang keputusan sebelum `DISETUJUI`; membuat endpoint rekaan; hardcode limit; plaintext secret; `tenant_id` dari browser; row lock saat HTTP eksternal; klaim RPO/RTO tanpa drill; menyatakan selesai hanya karena HTTP 200.

---

## Lampiran A — 30 butir V1.0 checklist (`PRD.md §18`)

| # | Kategori | Butir | Task | Bukti | Status |
|---:|---|---|---|---|---|
| 1 | SaaS foundation | Tenant registration dan authentication | P1-2/P1-3 | `docs/evidence/V1-01.txt` | Planned |
| 2 | SaaS foundation | Subdomain generation/resolution | P1-4 | `docs/evidence/V1-02.txt` | Planned |
| 3 | SaaS foundation | Membership dan granular RBAC | P1-5 | `docs/evidence/V1-03.txt` | Planned |
| 4 | SaaS foundation | PostgreSQL RLS | P1-7 | `docs/evidence/V1-04.txt` | Planned |
| 5 | SaaS foundation | Cross-tenant isolation suite | P1-10 | `docs/evidence/V1-05.txt` | Planned |
| 6 | Digital presence | Template selection dan page editor | P2-2/P2-3 | `docs/evidence/V1-06.txt` | Planned |
| 7 | Digital presence | Business profile, hours, location | P2-5 | `docs/evidence/V1-07.txt` | Planned |
| 8 | Digital presence | Responsive public website | P2-7 | `docs/evidence/V1-08.txt` | Planned |
| 9 | Digital presence | Publish dan version rollback | P2-4 | `docs/evidence/V1-09.txt` | Planned |
| 10 | Commerce | Product catalog dan variants | P3-1 | `docs/evidence/V1-10.txt` | Planned |
| 11 | Commerce | Inventory movement/reservation | P3-2 | `docs/evidence/V1-11.txt` | Planned |
| 12 | Commerce | Customer cart dan checkout | P3-4/P3-5 | `docs/evidence/V1-12.txt` | Planned |
| 13 | Commerce | Order lifecycle dan approval | P3-6/P3-8 | `docs/evidence/V1-13.txt` | Planned |
| 14 | Commerce | Payment integration atau verified manual payment workflow | P3-7 | `docs/evidence/V1-14.txt` | Planned |
| 15 | Operations | Multi-admin management | P4-1 | `docs/evidence/V1-15.txt` | Planned |
| 16 | Operations | Ticketing dan assignment | P4-3 | `docs/evidence/V1-16.txt` | Planned |
| 17 | Operations | Approval engine | P4-2 | `docs/evidence/V1-17.txt` | Planned |
| 18 | Operations | Notification dan audit log | P4-4/P4-5 | `docs/evidence/V1-18.txt` | Planned |
| 19 | AI platform | OpenAI-compatible provider configuration | P5-4 | `docs/evidence/V1-19.txt` | Planned |
| 20 | AI platform | Encrypted API credentials | P5-2 | `docs/evidence/V1-20.txt` | Planned |
| 21 | AI platform | AI orchestration dan tool registry | P5-5/P5-6 | `docs/evidence/V1-21.txt` | Planned |
| 22 | AI platform | Knowledge base dan tenant-scoped RAG | P5-7 | `docs/evidence/V1-22.txt` | Planned |
| 23 | AI platform | Chat ID dan conversation viewer | P6-1/P6-3 | `docs/evidence/V1-23.txt` | Planned |
| 24 | AI platform | AI-to-human handover | P6-4 | `docs/evidence/V1-24.txt` | Planned |
| 25 | Production | Rust API dan worker deployment | P0-6/P8-5 | `docs/evidence/V1-25.txt` | Planned |
| 26 | Production | VPS resource limits | P0-6/P8-5 | `docs/evidence/V1-26.txt` | Planned |
| 27 | Production | Automated external backups | P0-10/P8-4 | `docs/evidence/V1-27.txt` | Planned |
| 28 | Production | Monitoring dan alerting | P0-8/P8-5 | `docs/evidence/V1-28.txt` | Planned |
| 29 | Production | Security assessment | P8-1/P8-2 | `docs/evidence/V1-29.txt` | Planned |
| 30 | Production | Load dan recovery testing | P0-10/P8-4 | `docs/evidence/V1-30.txt` | Planned |

---

## Lampiran B — 17 keputusan baseline (`PRD.md §19`)

| # | Decision | Baseline | Di mana direpresentasikan | ADR / status |
|---:|---|---|---|---|
| 1 | Backend | Rust + Axum | `BUILD_PLAN.md §0`, `BUILD_PLAN.md §3.1` | Baseline PRD — `MENUNGGU` |
| 2 | Architecture | Modular monolith | `BUILD_PLAN.md §1`, `BUILD_PLAN.md §3.3` | Baseline PRD — `MENUNGGU` |
| 3 | Database | PostgreSQL, shared schema | `BUILD_PLAN.md §3.4` | Baseline PRD — `MENUNGGU` |
| 4 | Tenant isolation | `tenant_id` + RLS + application authorization | `BUILD_PLAN.md §2`, `BUILD_PLAN.md §5` P1-7 | Baseline PRD — `MENUNGGU` |
| 5 | DB access | SQLx | `BUILD_PLAN.md §3.2`, `BUILD_PLAN.md §5` P0-4 | Baseline PRD — `MENUNGGU` |
| 6 | Cache/coordination | Redis | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §5` P0-5 | Baseline PRD — `MENUNGGU` |
| 7 | Background work | Rust Worker + PostgreSQL transactional outbox | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §5` P1-10 | Baseline PRD — `MENUNGGU` |
| 8 | Frontend | Next.js static export where feasible | `BUILD_PLAN.md §3.5` | ADR-0014 — `MENUNGGU` |
| 9 | Realtime | SSE + WebSocket | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §5` P4-6/P6-2 | Baseline PRD — `MENUNGGU` |
| 10 | AI | OpenAI-compatible adapter/router | `BUILD_PLAN.md §4`, `BUILD_PLAN.md §5` P5-4 | Baseline PRD — `MENUNGGU` |
| 11 | Vector search | pgvector | `BUILD_PLAN.md §3.4`, `BUILD_PLAN.md §5` P5-7 | Baseline PRD — `MENUNGGU` |
| 12 | File storage | S3-compatible object storage | `BUILD_PLAN.md §5` P2-6 | Baseline PRD — `MENUNGGU` |
| 13 | Reverse proxy | Caddy | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §5` P0-6 | Baseline PRD — `MENUNGGU` |
| 14 | Deployment | Docker Compose | `BUILD_PLAN.md §5` P0-6 | Baseline PRD — `MENUNGGU` |
| 15 | Monitoring | Prometheus-compatible metrics; Grafana optional | `BUILD_PLAN.md §5` P0-8 | Baseline PRD — `MENUNGGU` |
| 16 | CI/CD | GitHub Actions or equivalent | `BUILD_PLAN.md §5` P0-7 | Baseline PRD — `MENUNGGU` |
| 17 | Initial server | VPS RAM approximately 8 GB | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §9` | Baseline PRD — `MENUNGGU` |

**Catatan jujur:** `PRD.md §4.3`–`PRD.md §4.15` belum diturunkan menjadi `FR-...` bernomor. Itu adalah pekerjaan FSD artefak #3 dan sengaja tidak direkayasa di sini agar tidak mengubah kontrak PRD tanpa otoritas.
