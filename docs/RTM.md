# Requirements Traceability Matrix — UMKM Business OS

**Status:** **final dua arah** (artefak #6, 2026-11-12). §1–§7 = register ID PRD (AUTH/TEN/BR/NFR). §8 = rantai `FR → AC → TC → API → ADR → Evidence` untuk `PRD.md §4.3`–`§4.15`, diturunkan dari FSD `docs/requirements/02-FSD.md` dan Test Plan `docs/test/test-plan.md`.
**Sumber:** `PRD.md` v1.0; rencana `BUILD_PLAN.md`; keputusan `OPEN_DECISIONS.md`.
**Penjaga:** `scripts/check-rtm.py` (CI job `lint`, fail-closed): setiap FR FSD tepat satu baris §8; setiap AC FSD tercakup; setiap TC ada di Test Plan dan menunjuk AC yang ada; tidak ada ID orphan; status `Verified` wajib menunjuk evidence + lokasi uji yang ada.

## Cara baca

| Kolom | Arti |
|---|---|
| ID | ID stabil dari `PRD.md`; tidak ada ID rekaan |
| Requirement | Ringkasan; `PRD.md` tetap otoritatif |
| PRD | Selalu `PRD.md §N.M`, tidak ada sitasi telanjang |
| Rencana | Task di `BUILD_PLAN.md` |
| Verifikasi | Rencana pengujian, belum bukti |
| Status | `Planned` / `Blocked by decision` / `Verified` (hanya dengan `docs/evidence/*`); keputusan memakai `MENUNGGU`/`DISETUJUI`/`DITOLAK` |

> **Status (2026-11-11):** Phase 0 dan Phase 1 dibangun; baris yang `Verified` menunjuk ke `docs/evidence/*`. Lihat `docs/evidence/phase-1-isolation.txt` untuk Gate P1.

---

## 1. AUTH — `PRD.md §4.1`

| ID | Requirement | PRD | Rencana | Verifikasi | Status |
|---|---|---|---|---|---|
| AUTH-001 | Register email + password | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-002 | Email unik global case-insensitive | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-003 | Email verification sebelum publish | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Blocked by decision |
| AUTH-004 | Login/logout | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-005 | Forgot password token sekali pakai | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Blocked by decision |
| AUTH-006 | Session revoke per/semua perangkat | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-007 | Rate limit login & reset | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-008 | User bisa member beberapa tenant | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-009 | Owner undang anggota via email | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Blocked by decision |
| AUTH-010 | Invitation expiry + one-time | `PRD.md §4.1` | P1-2 | `PRD.md §13.3` Registration / security test | Blocked by decision |
| AUTH-011 | MFA/TOTP Platform Super Admin | `PRD.md §4.1` | P4-1 | `PRD.md §13.3` Registration / security test | Planned |
| AUTH-012 | Owner bisa mewajibkan MFA tenant admin | `PRD.md §4.1` | P4-1 | `PRD.md §13.3` Registration / security test | Planned |

## 2. TEN — `PRD.md §4.2`

| ID | Requirement | PRD | Rencana | Verifikasi | Status |
|---|---|---|---|---|---|
| TEN-001 | Registrasi bisnis membuat tenant | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-002 | Membership Owner dibuat | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-003 | Business profile + konfigurasi default | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-004 | Subdomain otomatis | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-005 | Slug unik & tervalidasi | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-006 | Reserved subdomain ditolak | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |
| TEN-007 | Ganti subdomain melalui validasi | `PRD.md §4.2` | P7-4 conditional | `PRD.md §13.3` Registration / domain test | Blocked by decision |
| TEN-008 | Custom domain daftar + verifikasi | `PRD.md §4.2` | P7-4 conditional | `PRD.md §13.3` Registration / domain test | Blocked by decision |
| TEN-009 | Domain suspended/disabled tidak melayani | `PRD.md §4.2` | P1-3 | `PRD.md §13.3` Registration / domain test | Planned |

## 3. BR — `PRD.md §11`

| ID | Requirement | PRD | Rencana | Verifikasi | Status |
|---|---|---|---|---|---|
| BR-001 | User bisa member beberapa tenant | `PRD.md §11` | P1-1/P1-3 | workflow/integration test | Planned |
| BR-002 | Minimal satu active Owner | `PRD.md §11` | P1-1/P1-3 | workflow/integration test | Planned |
| BR-003 | Primary subdomain | `PRD.md §11` | P1-1/P1-3 | workflow/integration test | Planned |
| BR-004 | Hostname unik global | `PRD.md §11` | P1-1/P1-3 | workflow/integration test | Planned |
| BR-005 | Business profile wajib | `PRD.md §11` | P1-1/P1-3 | workflow/integration test | Planned |
| BR-006 | Immutable order item snapshot | `PRD.md §11` | P3-4/P3-5 | workflow/integration test | Planned |
| BR-007 | Stock adjustment membuat movement | `PRD.md §11` | P3-2 | workflow/integration test | Planned |
| BR-008 | Payment webhook idempotent | `PRD.md §11` | P3-7 | workflow/integration test | Planned |
| BR-009 | AI tidak lewati approval | `PRD.md §11` | P4-2/P5-6 | workflow/integration test | Planned |
| BR-010 | Sensitive admin actions diaudit | `PRD.md §11` | P1-8/P4-5 | workflow/integration test | Planned |
| BR-011 | Tenant suspended tidak terima transaksi baru | `PRD.md §11` | P1-4/P3-5 | workflow/integration test | Planned |
| BR-012 | Tenant tidak akses data tenant lain | `PRD.md §11` | P1-10 | 9 tes `PRD.md §13.2` | Planned |
| BR-013 | Conversation/message tenant-scoped | `PRD.md §11` | P6-1/P4-6 | workflow/integration test | Planned |
| BR-014 | Provider secret tidak dapat dibaca kembali | `PRD.md §11` | P5-2 | workflow/integration test | Planned |
| BR-015 | Ticket dan order lifecycle terpisah | `PRD.md §11` | P4-3/P3-6 | workflow/integration test | Planned |
| BR-016 | Customer guest atau registered | `PRD.md §11` | P3-3 contingent D-03 | account-linking test; D-03 | Blocked by decision |
| BR-017 | Background jobs retry & idempotency | `PRD.md §11` | P1-6/P4-4 | workflow/integration test | Planned |
| BR-018 | External integration timeout & error handling | `PRD.md §11` | P3-7/P5-4/P5-8 | workflow/integration test | Planned |
| BR-019 | Subscription limits dicek backend | `PRD.md §11` | P7-2 | workflow/integration test | Planned |
| BR-020 | Data deletion ikut retention policy | `PRD.md §11` | P1-9 (ADR-0012) | deletion/purge test | Verified (`docs/evidence/P1-9.txt`; percakapan/file/pelanggan menyusul modulnya) |

## 4. NFR — `PRD.md §10`

| ID | Requirement | PRD | Rencana | Verifikasi | Status |
|---|---|---|---|---|---|
| NFR-001 | API read p95 < 300 ms | `PRD.md §10` | P8-2/P8-3 | benchmark/report dengan env + dataset + threshold | Planned |
| NFR-002 | API write p95 < 500 ms | `PRD.md §10` | P8-2/P8-3 | benchmark/report dengan env + dataset + threshold | Planned |
| NFR-003 | Public page TTFB p95 < 500 ms | `PRD.md §10` | P2-7/P8-2 | benchmark/report dengan env + dataset + threshold | Planned |
| NFR-004 | Dashboard load p95 < 2 s | `PRD.md §10` | P2-7/P8-2 | benchmark/report dengan env + dataset + threshold | Planned |
| NFR-005 | Indexed DB query p95 < 100 ms | `PRD.md §10` | P8-3 | benchmark/report dengan env + dataset + threshold | Planned |
| NFR-006 | Availability 99.5% objective | `PRD.md §10` | P0-8/P8-5 | monitoring; objective bukan jaminan | Planned |
| NFR-007 | RPO 15 mnt / RTO 4 jam target | `PRD.md §10` | P0-10/P8-4 | restore drill | Planned |
| NFR-008 | ID default, English-ready | `PRD.md §10` | semua task | review/check | Planned |
| NFR-009 | Keyboard navigation + readable contrast | `PRD.md §10` | P2-3/P2-7 | review/check | Planned |
| NFR-010 | Mobile-first/tablet/desktop | `PRD.md §10` | P2-3/P2-7 | review/check | Planned |
| NFR-011 | Versioned migrations, tests, ADR, API docs | `PRD.md §10` | P0-4/P0-9/`BUILD_PLAN.md §7` | review/check | Planned |
| NFR-012 | Dapat memisahkan API/worker/DB/storage kemudian | `PRD.md §10` | `BUILD_PLAN.md §3.1`, `BUILD_PLAN.md §3.2` | review/check | Planned |

---

## 5. Acceptance → phase (`PRD.md §13.3`)

| Kelompok | Skenario | Rencana | Status |
|---|---|---|---|
| Registration | user+tenant, Owner, subdomain unik, reserved slug ditolak | P1-3, P1-10 | Planned |
| Website | 5 skenario: template, draft, preview, publish, rollback | P2-1..P2-4 | Planned |
| Commerce | harga server-side, tidak oversold, duplicate checkout, snapshot | P3-2, P3-5 | Planned |
| Ticketing | ticket, assign/reassign, history, takeover | P4-3, P6-4 | Planned |
| AI | provider, API key masked, tenant context, unauthorized tool, provider failure | P5-2..P5-8 | Planned |
| Approval | permission, rejection reason, event, stale state | P3-8, P4-2 | Planned |

## 6. Isolation suite → Gate P1 (`PRD.md §13.2`)

| # | Test | Rencana | Status |
|---:|---|---|---|
| 1 | Tenant A baca resource miliknya | P1-10 jalur riil | Verified (`docs/evidence/P1-10.txt`) |
| 2 | Tenant A tak baca Tenant B | P1-10 jalur riil | Verified (`docs/evidence/P1-10.txt`) |
| 3 | Tenant A tak ubah/hapus Tenant B | P1-10 jalur riil | Verified (`docs/evidence/P1-10.txt`) |
| 4 | Tak baca conversation/AI config/ticket/order/file Tenant B | P1-10 jalur riil | Planned (penjaga katalog aktif; tabel modul belum ada) |
| 5 | Background job tak proses tenant salah | P1-10 worker minimum nyata | Verified (`docs/evidence/P1-10.txt`) |
| 6 | Cache tidak collision | P0-5, P1-10 | Verified (`docs/evidence/P1-10.txt`) |
| 7 | WebSocket/SSE tidak kirim event lintas tenant | P1-10 realtime minimum nyata | Verified untuk SSE (`docs/evidence/P1-10.txt`); WebSocket P6-2 |
| 8 | RLS bekerja pada pooled connection reuse | P1-7, P1-10 | Verified (`docs/evidence/P1-10.txt`) |
| 9 | App DB role tidak bisa bypass RLS | P1-7, P1-10 | Verified (`docs/evidence/P1-10.txt`) |

## 7. Open decisions (`PRD.md §20`)

| ID | Keputusan | Fase terblokir | Status |
|---|---|---|---|
| D-01 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | DISETUJUI |
| D-02 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-03 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-04 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-05 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | DISETUJUI |
| D-06 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-07 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | DISETUJUI sementara (lokal) |
| D-08 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-09 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-10 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-11 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | DISETUJUI |
| D-12 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU (privacy/terms registrasi DISETUJUI bersyarat) |

---

## 9. Pemeriksaan register (dijalankan saat review)

Guard ini **hanya** memeriksa register satu arah: setiap ID `PRD.md` muncul, dan setiap baris requirement memiliki jumlah sel sama dengan header. Guard **tidak** mengklaim traceability dua arah; itu baru dibuat pada RTM final dengan `FR/AC/TC/API/Evidence` dan CI guard tersendiri.

```bash
python3 - <<'PY'
import re, pathlib, sys
lines = pathlib.Path('docs/RTM.md').read_text().splitlines()
prd = pathlib.Path('docs/PRD.md').read_text()
ids = sorted(set(re.findall(r'\b(?:AUTH|TEN|BR|NFR)-\d{3}\b', prd)))
missing = [i for i in ids if not any(re.match(r'^\|\s*' + re.escape(i) + r'\s*\|', l) for l in lines)]
sections = []; header_cols = None
for n, line in enumerate(lines, 1):
    if re.match(r'^\|\s*ID\s*\|', line):
        header_cols = line.count('|') - 1
    elif re.match(r'^\|\s*-', line):
        continue
    elif re.match(r'^\|\s*(?:AUTH|TEN|BR|NFR)-\d{3}\s*\|', line):
        if header_cols is not None:
            sections.append((n, line, line.count('|') - 1, header_cols))
    elif line.strip() == '':
        header_cols = None
bad_width = [(n, got, expected) for n, l, got, expected in sections if got != expected]
empty = [(n, l[:100]) for n, l, got, expected in sections if re.search(r'\|\s*\|', l)]
print('PRD ids:', len(ids), 'missing:', len(missing), missing)
print('RTM rows:', len(sections), 'wrong-width:', len(bad_width), bad_width, 'empty:', len(empty), empty)
if missing or bad_width or empty:
    sys.exit(1)
PY
```

**Cara lulus:** `missing = 0`, `wrong-width = 0`, `empty = 0`, **exit code `0`**. Negative control wajib: tambah satu `|` pada salinan temp, buktikan `exit 1`, pulihkan, buktikan `exit 0`.

---

## 8. RTM dua arah: FR → AC → TC → API → ADR → Evidence

Arah balik: TC → AC lewat nomor yang sama (Test Plan §4); AC → FR lewat FSD; Evidence → task lewat nama file. API final dirujuk ke `docs/api/openapi.yaml` saat task selesai.

| FR | AC | TC | API | ADR | Evidence | Status |
|---|---|---|---|---|---|---|
| FR-WEB-001 | AC-WEB-001, AC-WEB-002 | TC-WEB-001, TC-WEB-002 | GET /api/v1/tenants/{tenant_id}/website/templates; GET /api/v1/tenants/{tenant_id}/website/templates/{key}/preview | ADR-0007, ADR-0014 | `docs/evidence/P2-2.txt` | Verified |
| FR-WEB-002 | AC-WEB-003 | TC-WEB-003 | PUT /api/v1/tenants/{tenant_id}/website | ADR-0007, ADR-0014 | `docs/evidence/P2-2.txt` | Verified |
| FR-WEB-003 | AC-WEB-004, AC-WEB-005, AC-WEB-006 | TC-WEB-004, TC-WEB-005, TC-WEB-006 | PUT /api/v1/tenants/{tenant_id}/website/pages/{slug}/draft | ADR-0007, ADR-0014 | `docs/evidence/P2-1.txt` | Verified |
| FR-WEB-004 | AC-WEB-007, AC-WEB-008 | TC-WEB-007, TC-WEB-008 | PUT /api/v1/tenants/{tenant_id}/website/pages/{slug}/draft | ADR-0007, ADR-0014 | `docs/evidence/P2-3.txt` | Verified |
| FR-WEB-005 | AC-WEB-009 | TC-WEB-009 | PUT /api/v1/tenants/{tenant_id}/website/pages/{slug}/draft | ADR-0007, ADR-0014 | `docs/evidence/P2-3.txt` | Verified |
| FR-WEB-006 | AC-WEB-010 | TC-WEB-010 | GET/PUT /api/v1/tenants/{tenant_id}/website/pages/{slug}/draft | ADR-0007, ADR-0014 | `docs/evidence/P2-4.txt` | Verified |
| FR-WEB-007 | AC-WEB-011 | TC-WEB-011 | GET /api/v1/tenants/{tenant_id}/website/pages/{slug}/draft | ADR-0007, ADR-0014 | `docs/evidence/P2-4.txt` | Verified |
| FR-WEB-008 | AC-WEB-012, AC-WEB-013 | TC-WEB-012, TC-WEB-013 | POST /api/v1/tenants/{tenant_id}/website/pages/{slug}/publish; GET /api/v1/tenants/{tenant_id}/website/pages/{slug}/versions | ADR-0007, ADR-0014 | `docs/evidence/P2-4.txt` | Verified |
| FR-WEB-009 | AC-WEB-014 | TC-WEB-014 | POST /api/v1/tenants/{tenant_id}/website/pages/{slug}/rollback | ADR-0007, ADR-0014 | `docs/evidence/P2-4.txt` | Verified |
| FR-WEB-010 | AC-WEB-015, AC-WEB-016, AC-WEB-017 | TC-WEB-015, TC-WEB-016, TC-WEB-017 | GET /api/v1/public/site (Host) | ADR-0007, ADR-0014 | `docs/evidence/P2-7.txt` | Verified |
| FR-WEB-011 | AC-WEB-005, AC-WEB-018 | TC-WEB-005, TC-WEB-018 | GET /api/v1/public/site (Host) | ADR-0007, ADR-0014 | `docs/evidence/P2-1.txt` | Verified |
| FR-BIZ-001 | AC-BIZ-001, AC-BIZ-002 | TC-BIZ-001, TC-BIZ-002 | GET/PUT /api/v1/tenants/{tenant_id}/business/profile | — | `docs/evidence/P2-5.txt` | Verified |
| FR-BIZ-002 | AC-BIZ-003 | TC-BIZ-003 | PUT /api/v1/tenants/{tenant_id}/business/addresses | — | `docs/evidence/P2-5.txt` | Verified |
| FR-BIZ-003 | AC-BIZ-004 | TC-BIZ-004 | PUT /api/v1/tenants/{tenant_id}/business/hours | — | `docs/evidence/P2-5.txt` | Verified |
| FR-BIZ-004 | AC-BIZ-005 | TC-BIZ-005 | PUT /api/v1/tenants/{tenant_id}/business/hour-exceptions | — | `docs/evidence/P2-5.txt` | Verified |
| FR-BIZ-005 | AC-BIZ-006 | TC-BIZ-006 | GET /api/v1/tenants/{tenant_id}/business/status | — | `docs/evidence/P2-5.txt` | Verified |
| FR-BIZ-006 | AC-BIZ-007 | TC-BIZ-007 | GET/PUT /api/v1/tenants/{tenant_id}/business/profile | — | `docs/evidence/P2-5.txt` | Verified |
| FR-FILE-001 | AC-FILE-001, AC-FILE-002 | TC-FILE-001, TC-FILE-002 | POST /api/v1/tenants/{tenant_id}/uploads | ADR-0009 | `docs/evidence/P2-6.txt` | Verified |
| FR-FILE-002 | AC-FILE-003 | TC-FILE-003 | POST /api/v1/tenants/{tenant_id}/uploads | ADR-0009 | `docs/evidence/P2-6.txt` | Verified |
| FR-FILE-003 | AC-FILE-004, AC-FILE-005 | TC-FILE-004, TC-FILE-005 | GET /files/{object_id} | ADR-0009 | `docs/evidence/P2-6.txt` | Verified |
| FR-FILE-004 | AC-FILE-006 | TC-FILE-006 | GET /files/{object_id} | ADR-0009 | `docs/evidence/P2-6.txt` | Verified |
| FR-CAT-001 | AC-CAT-001 | TC-CAT-001 | /api/v1/tenants/{tenant_id}/products, /products/{id}/status, /api/v1/public/products | — | `docs/evidence/P3-1.txt` | Verified |
| FR-CAT-002 | AC-CAT-002 | TC-CAT-002 | /api/v1/tenants/{tenant_id}/categories | — | `docs/evidence/P3-1.txt` | Verified |
| FR-CAT-003 | AC-CAT-003 | TC-CAT-003 | POST/PUT /api/v1/tenants/{tenant_id}/products | — | `docs/evidence/P3-1.txt` | Verified |
| FR-CAT-004 | AC-CAT-004 | TC-CAT-004 | POST/PUT /api/v1/tenants/{tenant_id}/products | — | `docs/evidence/P3-1.txt` | Verified |
| FR-INV-001 | AC-INV-001 | TC-INV-001 | GET/POST /api/v1/tenants/{tenant_id}/inventory/{variant_id}/movements | — | `docs/evidence/P3-2.txt` | Verified |
| FR-INV-002 | AC-INV-002 | TC-INV-002 | PUT /api/v1/tenants/{tenant_id}/inventory/{variant_id}/settings, GET /inventory?low=true | — | `docs/evidence/P3-2.txt` | Verified |
| FR-INV-003 | AC-INV-003 | TC-INV-003 | internal: inventory_repo::reserve_in (checkout P3-5) | — | `docs/evidence/P3-2.txt` | Verified |
| FR-INV-004 | AC-INV-004 | TC-INV-004 | POST /inventory/{variant_id}/movements, PUT /settings | — | `docs/evidence/P3-2.txt` | Verified |
| FR-INV-005 | AC-INV-005 | TC-INV-005 | internal: inventory_repo::reserve_in (checkout P3-5) | — | `docs/evidence/P3-2.txt` | Verified |
| FR-CRM-001 | AC-CRM-001 | TC-CRM-001 | /api/v1/public/customer/{register,login,me} | ADR-0005 (D-03) | `docs/evidence/P3-3.txt` | Verified |
| FR-CRM-002 | AC-CRM-002 | TC-CRM-002 | GET /api/v1/tenants/{tenant_id}/customers | ADR-0005 (D-03) | `docs/evidence/P3-3.txt` | Verified |
| FR-CRM-003 | AC-CRM-003 | TC-CRM-003 | ditetapkan di OpenAPI saat task | ADR-0005 (D-03) | — | Planned |
| FR-CRM-004 | AC-CRM-004 | TC-CRM-004 | GET/PUT /api/v1/tenants/{tenant_id}/customers/{id} | ADR-0005 (D-03) | `docs/evidence/P3-3.txt` | Verified |
| FR-CRM-005 | AC-CRM-005 | TC-CRM-005 | POST /api/v1/public/customer/verify-email | ADR-0005 (D-03) | `docs/evidence/P3-3.txt` | Verified |
| FR-CRM-006 | AC-CRM-006 | TC-CRM-006 | POST /customers/{id}/deletion-request, GET /customers/{id}/export | ADR-0005 (D-03) | `docs/evidence/P3-3.txt` | Verified |
| FR-ORD-001 | AC-ORD-001 | TC-ORD-001 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-002 | AC-ORD-002 | TC-ORD-002 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-003 | AC-ORD-003 | TC-ORD-003 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-004 | AC-ORD-004 | TC-ORD-004 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-005 | AC-ORD-005 | TC-ORD-005 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-006 | AC-ORD-006 | TC-ORD-006 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-007 | AC-ORD-007 | TC-ORD-007 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-008 | AC-ORD-008 | TC-ORD-008 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-009 | AC-ORD-009 | TC-ORD-009 | ditetapkan di OpenAPI saat task | ADR-0004 (D-02) | — | Planned |
| FR-ORD-010 | AC-ORD-010 | TC-ORD-010 | ditetapkan di OpenAPI saat task | D-12 (product-decision-log) | — | Planned |
| FR-TKT-001 | AC-TKT-001 | TC-TKT-001 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-TKT-002 | AC-TKT-002 | TC-TKT-002 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-TKT-003 | AC-TKT-003 | TC-TKT-003 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-TKT-004 | AC-TKT-004 | TC-TKT-004 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-TKT-005 | AC-TKT-005 | TC-TKT-005 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-TKT-006 | AC-TKT-006 | TC-TKT-006 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-APR-001 | AC-APR-001 | TC-APR-001 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-APR-002 | AC-APR-002 | TC-APR-002 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-APR-003 | AC-APR-003 | TC-APR-003 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-AIC-001 | AC-AIC-001 | TC-AIC-001 | ditetapkan di OpenAPI saat task | ADR-0011 (D-09) | — | Planned |
| FR-AIC-002 | AC-AIC-002 | TC-AIC-002 | ditetapkan di OpenAPI saat task | ADR-0011 (D-09) | — | Planned |
| FR-AIC-003 | AC-AIC-003 | TC-AIC-003 | ditetapkan di OpenAPI saat task | ADR-0011 (D-09) | — | Planned |
| FR-AIC-004 | AC-AIC-004 | TC-AIC-004 | ditetapkan di OpenAPI saat task | ADR-0011 (D-09) | — | Planned |
| FR-AIO-001 | AC-AIO-001 | TC-AIO-001 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-AIO-002 | AC-AIO-002 | TC-AIO-002 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-AIO-003 | AC-AIO-003 | TC-AIO-003 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-AIO-004 | AC-AIO-004 | TC-AIO-004 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-CNV-001 | AC-CNV-001 | TC-CNV-001 | ditetapkan di OpenAPI saat task | ADR-0006 (D-04) | — | Planned |
| FR-CNV-002 | AC-CNV-002 | TC-CNV-002 | ditetapkan di OpenAPI saat task | ADR-0006 (D-04) | — | Planned |
| FR-CNV-003 | AC-CNV-003 | TC-CNV-003 | ditetapkan di OpenAPI saat task | ADR-0006 (D-04) | — | Planned |
| FR-CNV-004 | AC-CNV-004 | TC-CNV-004 | ditetapkan di OpenAPI saat task | ADR-0006 (D-04) | — | Planned |
| FR-NTF-001 | AC-NTF-001 | TC-NTF-001 | ditetapkan di OpenAPI saat task | ADR-0010 (D-08) | — | Planned |
| FR-NTF-002 | AC-NTF-002 | TC-NTF-002 | ditetapkan di OpenAPI saat task | ADR-0010 (D-08) | — | Planned |
| FR-NTF-003 | AC-NTF-003 | TC-NTF-003 | ditetapkan di OpenAPI saat task | ADR-0010 (D-08) | — | Planned |
| FR-ANL-001 | AC-ANL-001 | TC-ANL-001 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-ANL-002 | AC-ANL-002 | TC-ANL-002 | ditetapkan di OpenAPI saat task | — | — | Planned |
| FR-SUB-001 | AC-SUB-001 | TC-SUB-001 | ditetapkan di OpenAPI saat task | D-10 | — | Planned |
| FR-SUB-002 | AC-SUB-002 | TC-SUB-002 | ditetapkan di OpenAPI saat task | D-10 | — | Planned |
