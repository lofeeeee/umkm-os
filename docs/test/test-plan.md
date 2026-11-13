# Test Plan + Exit Criteria — UMKM Business OS

**Artefak:** #5 (`BUILD_PLAN.md §7`). **Versi:** 1.0, 2026-11-12.
**Sumber:** `PRD.md §13`, `BUILD_PLAN.md §8`, FSD `docs/requirements/02-FSD.md`.
**Aturan ID:** `TC-<AREA>-NNN` menguji `AC-<AREA>-NNN` dengan nomor yang sama (1:1). Lokasi uji diisi saat task mengimplementasikannya; status `Verified` hanya boleh bila lokasi uji ada di repo dan bukti `docs/evidence/<task>.txt` ada — dijaga `scripts/check-rtm.py` di CI (fail-closed).

## 1. Prinsip

1. Uji jalur nyata (HTTP → application → PostgreSQL dengan `umkm_app_role`, RLS aktif). Superuser tidak dipakai untuk membuktikan isolasi.
2. Setiap guard keamanan punya **negative control**: implementasi yang dilemahkan harus membuat tes gagal (dicatat di evidence).
3. "HTTP 200" bukan bukti selesai; yang diuji adalah state database + respons + audit.
4. Tidak ada angka benchmark yang diklaim sebagai kapasitas (`PRD.md §13.4`).

## 2. Layer (`PRD.md §13.1`)

| Layer | Scope | Alat | Kapan |
|---|---|---|---|
| Unit | Domain rules, state transition, schema validator | `cargo test` (crate `domain`) | setiap PR |
| Integration | PostgreSQL, Redis, API, migration up/down/up | `cargo test --workspace` + DB uji `scripts/dev-db.sh` / service CI | setiap PR |
| Contract | OpenAPI ↔ kode | tes drift `apps/api` (P0-9) | setiap PR |
| Security | Auth, RBAC, RLS, SSRF, upload, redaction, XSS renderer | tes integrasi + negative control | setiap PR |
| Tenant isolation | 9 butir `PRD.md §13.2` + tabel baru otomatis | `crates/infrastructure/tests/tenancy*` | setiap PR, release-blocking |
| Workflow | Order, ticket, approval, outbox crash | tes integrasi multi-langkah | per phase |
| AI | Adapter, tools, guardrails, RAG scope | mock provider matrix | P5 |
| Load | 5 skenario `PRD.md §13.4` | k6/oha, env + dataset + threshold dicatat | P8 |
| Recovery | Backup → restore ke DB kosong | `scripts/test-backup-restore.sh` | P0-10, drill P8-4 |
| E2E | Journey customer + admin; frontend | Playwright (frontend, setelah ADR-0014) | P2-7 dst, sebelum go-live |

Frontend (ADR-0014): `npm run lint`, `npm run typecheck`, `npm test` (unit renderer: XSS/teks literal, section disabled), dan uji responsif 375/768/1024/1440 (AC-WEB-017).

## 3. Exit criteria per gate

| Gate | Exit criteria |
|---|---|
| P1 | 9 tes isolasi riil lulus; app role tidak bypass RLS — **lulus** (`docs/evidence/phase-1-isolation.txt`) |
| P2 | Semua TC-WEB/TC-BIZ/TC-FILE `Verified`; 5 skenario Website `PRD.md §13.3` lulus; tabel P2 tercakup penjaga isolasi; fuzz validator ≥ 10.000 input tanpa panic; frontend lint/typecheck/test lulus; perintah gate exit 0 |
| P3 | TC-CAT/TC-INV/TC-ORD `Verified` (cabang contingent setelah D-02/D-03); overselling fixture lulus |
| P4 | TC-TKT/TC-APR/TC-NTF `Verified`; 10 event audit minimum |
| P5 | TC-AIC/TC-AIO `Verified`; suite injection + SSRF 9 kontrol |
| P6 | TC-CNV `Verified`; isolasi #4 penuh (conversation/AI/ticket/order/file) |
| P7–P8 | TC-ANL/TC-SUB `Verified`; load + recovery drill; 30/30 checklist V1.0 |

Perintah gate P2: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && python3 scripts/check-rtm.py && (cd frontend && npm run lint && npm run typecheck && npm test && npm run build)`.

## 4. Katalog test case

| TC | Menguji | Layer | Lokasi uji | Status |
|---|---|---|---|---|
| TC-WEB-001 | AC-WEB-001 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-002 | AC-WEB-002 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-003 | AC-WEB-003 | Unit+Integration+Contract | `crates/infrastructure/tests/website_schema.rs` | Verified |
| TC-WEB-004 | AC-WEB-004 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `crates/domain/src/website/schema.rs` | Verified |
| TC-WEB-005 | AC-WEB-005 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `crates/domain/src/website/schema.rs` | Verified |
| TC-WEB-006 | AC-WEB-006 | Unit+Integration+Contract | `crates/domain/tests/website_fuzz.rs` | Verified |
| TC-WEB-007 | AC-WEB-007 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `frontend/src/renderer/Site.test.tsx` | Verified |
| TC-WEB-008 | AC-WEB-008 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `frontend/src/renderer/Site.test.tsx` | Verified |
| TC-WEB-009 | AC-WEB-009 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `crates/application/src/website/editor.rs` | Verified |
| TC-WEB-010 | AC-WEB-010 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-011 | AC-WEB-011 | Unit+Integration+Contract | `apps/api/tests/website_http.rs`, `frontend/e2e/dashboard.spec.ts` | Verified |
| TC-WEB-012 | AC-WEB-012 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-013 | AC-WEB-013 | Unit+Integration+Contract | `crates/infrastructure/tests/website_schema.rs` | Verified |
| TC-WEB-014 | AC-WEB-014 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-015 | AC-WEB-015 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-016 | AC-WEB-016 | Unit+Integration+Contract | `apps/api/tests/website_http.rs` | Verified |
| TC-WEB-017 | AC-WEB-017 | E2E (browser) | `frontend/e2e/responsive.spec.ts` | Verified |
| TC-WEB-018 | AC-WEB-018 | Unit + E2E + Security | `frontend/src/renderer/Site.test.tsx`, `frontend/e2e/dashboard.spec.ts`, `scripts/smoke-adr0014.sh` | Verified |
| TC-BIZ-001 | AC-BIZ-001 | Unit+Integration | `apps/api/tests/business_http.rs` | Verified |
| TC-BIZ-002 | AC-BIZ-002 | Unit+Integration | `apps/api/tests/business_http.rs` | Verified |
| TC-BIZ-003 | AC-BIZ-003 | Unit+Integration | `apps/api/tests/business_http.rs`, `crates/domain/src/business.rs` | Verified |
| TC-BIZ-004 | AC-BIZ-004 | Unit+Integration | `apps/api/tests/business_http.rs` | Verified |
| TC-BIZ-005 | AC-BIZ-005 | Unit+Integration | `apps/api/tests/business_http.rs`, `crates/domain/src/business.rs` | Verified |
| TC-BIZ-006 | AC-BIZ-006 | Unit+Integration | `apps/api/tests/business_http.rs`, `crates/domain/src/business.rs` | Verified |
| TC-BIZ-007 | AC-BIZ-007 | Unit+Integration | `apps/api/tests/business_http.rs` | Verified |
| TC-FILE-001 | AC-FILE-001 | Security+Integration | `apps/api/tests/upload_http.rs` | Verified |
| TC-FILE-002 | AC-FILE-002 | Security+Integration | `apps/api/tests/upload_http.rs`, `crates/domain/src/storage.rs` | Verified |
| TC-FILE-003 | AC-FILE-003 | Security+Integration | `apps/api/tests/upload_http.rs` | Verified |
| TC-FILE-004 | AC-FILE-004 | Security+Integration | `apps/api/tests/upload_http.rs`, `crates/infrastructure/src/storage.rs` | Verified |
| TC-FILE-005 | AC-FILE-005 | Security+Integration | `apps/api/tests/upload_http.rs` | Verified |
| TC-FILE-006 | AC-FILE-006 | Security+Integration | `apps/api/tests/upload_http.rs`, `crates/infrastructure/tests/rls.rs` | Verified |
| TC-CAT-001 | AC-CAT-001 | Unit+Integration | `apps/api/tests/catalog_http.rs` | Verified |
| TC-CAT-002 | AC-CAT-002 | Unit+Integration | `apps/api/tests/catalog_http.rs` | Verified |
| TC-CAT-003 | AC-CAT-003 | Unit+Integration | `apps/api/tests/catalog_http.rs` | Verified |
| TC-CAT-004 | AC-CAT-004 | Unit+Integration | `apps/api/tests/catalog_http.rs` | Verified |
| TC-INV-001 | AC-INV-001 | Unit+Integration | `apps/api/tests/inventory_http.rs` | Verified |
| TC-INV-002 | AC-INV-002 | Unit+Integration | `apps/api/tests/inventory_http.rs` | Verified |
| TC-INV-003 | AC-INV-003 | Unit+Integration | `apps/api/tests/inventory_http.rs` | Verified |
| TC-INV-004 | AC-INV-004 | Unit+Integration | `apps/api/tests/inventory_http.rs` | Verified |
| TC-INV-005 | AC-INV-005 | Unit+Integration | `apps/api/tests/inventory_http.rs` | Verified |
| TC-CRM-001 | AC-CRM-001 | Integration+Security | `apps/api/tests/crm_http.rs` | Verified |
| TC-CRM-002 | AC-CRM-002 | Integration+Security | `apps/api/tests/crm_http.rs` | Verified |
| TC-CRM-003 | AC-CRM-003 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-CRM-004 | AC-CRM-004 | Integration+Security | `apps/api/tests/crm_http.rs` | Verified |
| TC-CRM-005 | AC-CRM-005 | Integration+Security | `apps/api/tests/crm_http.rs` | Verified |
| TC-CRM-006 | AC-CRM-006 | Integration+Security | `apps/api/tests/crm_http.rs` | Verified |
| TC-ORD-001 | AC-ORD-001 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-ORD-002 | AC-ORD-002 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-ORD-003 | AC-ORD-003 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-ORD-004 | AC-ORD-004 | Unit+Integration | `crates/infrastructure/tests/commerce_schema.rs` | Verified |
| TC-ORD-005 | AC-ORD-005 | Integration+Workflow | `apps/api/tests/payment_http.rs` | Verified |
| TC-ORD-006 | AC-ORD-006 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-ORD-007 | AC-ORD-007 | Integration+Workflow | `apps/api/tests/payment_http.rs` | Verified |
| TC-ORD-008 | AC-ORD-008 | Integration+Workflow | `apps/api/tests/payment_http.rs` | Verified |
| TC-ORD-009 | AC-ORD-009 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-ORD-010 | AC-ORD-010 | Integration | `apps/api/tests/checkout_http.rs` | Verified |
| TC-TKT-001 | AC-TKT-001 | Unit+Integration | `apps/api/tests/ticket_http.rs` | Verified |
| TC-TKT-002 | AC-TKT-002 | Unit+Integration | `apps/api/tests/ticket_http.rs` | Verified |
| TC-TKT-003 | AC-TKT-003 | Unit+Integration | `apps/api/tests/ticket_http.rs` | Verified |
| TC-TKT-004 | AC-TKT-004 | Unit+Integration | `apps/api/tests/ticket_http.rs` | Verified |
| TC-TKT-005 | AC-TKT-005 | Unit+Integration | `apps/api/tests/ticket_http.rs` | Verified |
| TC-TKT-006 | AC-TKT-006 | Integration+E2E | `apps/api/tests/conversation_inbox_http.rs`, `frontend/e2e/chat.spec.ts` | Verified |
| TC-APR-001 | AC-APR-001 | Unit+Integration+Workflow | `apps/api/tests/approval_http.rs` | Verified |
| TC-APR-002 | AC-APR-002 | Unit+Integration+Workflow | `apps/api/tests/approval_http.rs` | Verified |
| TC-APR-003 | AC-APR-003 | Unit+Integration+Workflow | `apps/api/tests/approval_http.rs` | Verified |
| TC-AIC-001 | AC-AIC-001 | Unit+Integration | `crates/domain/src/ai/mod.rs`, `crates/infrastructure/tests/ai_schema.rs`, `apps/api/tests/ai_http.rs` | Verified |
| TC-AIC-002 | AC-AIC-002 | Unit+Integration | `crates/infrastructure/src/ai/provider_adapter.rs`, `crates/infrastructure/tests/provider_adapter.rs` | Verified |
| TC-AIC-003 | AC-AIC-003 | Unit+Integration | `crates/infrastructure/src/crypto.rs`, `apps/api/tests/ai_http.rs` | Verified |
| TC-AIC-004 | AC-AIC-004 | Unit+Integration | `crates/domain/src/ai/mode.rs`, `crates/application/tests/orchestrator.rs` | Verified |
| TC-AIO-001 | AC-AIO-001 | Unit+Integration | `crates/application/src/ai/guardrails.rs`, `crates/application/tests/orchestrator.rs` | Verified |
| TC-AIO-002 | AC-AIO-002 | Unit+Integration | `crates/application/src/ai/tools.rs`, `crates/application/tests/orchestrator.rs` | Verified |
| TC-AIO-003 | AC-AIO-003 | Unit+Integration | `crates/application/src/ai/orchestrator.rs`, `crates/application/tests/orchestrator.rs` | Verified |
| TC-AIO-004 | AC-AIO-004 | Integration | `crates/infrastructure/tests/rag.rs` | Verified |
| TC-CNV-001 | AC-CNV-001 | Unit+Integration | `crates/domain/src/conversation.rs`, `crates/infrastructure/tests/conversation_schema.rs` | Verified |
| TC-CNV-002 | AC-CNV-002 | Unit+Integration+E2E | `apps/api/tests/conversation_viewer_http.rs`, `frontend/e2e/chat.spec.ts` | Verified |
| TC-CNV-003 | AC-CNV-003 | Unit+Integration | `crates/domain/src/conversation.rs`, `apps/api/tests/conversation_viewer_http.rs` | Verified |
| TC-CNV-004 | AC-CNV-004 | lihat §2 | ditetapkan saat task | Planned |
| TC-NTF-001 | AC-NTF-001 | Integration+Workflow | `apps/api/tests/notification_http.rs` | Verified |
| TC-NTF-002 | AC-NTF-002 | Integration+Workflow | `apps/api/tests/notification_http.rs` | Verified |
| TC-NTF-003 | AC-NTF-003 | Integration+Workflow | `apps/api/tests/notification_http.rs` | Verified |
| TC-ANL-001 | AC-ANL-001 | lihat §2 | ditetapkan saat task | Planned |
| TC-ANL-002 | AC-ANL-002 | lihat §2 | ditetapkan saat task | Planned |
| TC-SUB-001 | AC-SUB-001 | lihat §2 | ditetapkan saat task | Planned |
| TC-SUB-002 | AC-SUB-002 | lihat §2 | ditetapkan saat task | Planned |
