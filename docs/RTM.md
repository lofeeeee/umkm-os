# Requirements Traceability Matrix — UMKM Business OS

**Status:** **register satu arah, draft**. Ini bukan RTM dua arah final dan bukan bukti implementasi.
**Sumber:** `PRD.md` v1.0; rencana `BUILD_PLAN.md`; keputusan `OPEN_DECISIONS.md`.
**Yang belum ada:** `FR-...`, `AC-...`, `TC-...`, `API-...`, dan `Evidence-...` stabil. Semuanya dibuat pada FSD/Test Plan/RTM final (`BUILD_PLAN.md §7` artefak #3/#5/#6). Karena itu klaim "dua arah, tanpa orphan" **tidak** dibuat di sini.

## Cara baca

| Kolom | Arti |
|---|---|
| ID | ID stabil dari `PRD.md`; tidak ada ID rekaan |
| Requirement | Ringkasan; `PRD.md` tetap otoritatif |
| PRD | Selalu `PRD.md §N.M`, tidak ada sitasi telanjang |
| Rencana | Task di `BUILD_PLAN.md` |
| Verifikasi | Rencana pengujian, belum bukti |
| Status | `Planned` / `Blocked by decision` / `Verified` (hanya dengan `docs/evidence/*`); keputusan memakai `MENUNGGU`/`DISETUJUI`/`DITOLAK` |

> **Status hari ini:** 53 baris requirement berstatus `Planned` atau `Blocked by decision`; 12 baris keputusan di `RTM.md §7` berstatus `MENUNGGU`. Tidak ada `Verified`. Tidak ada kode aplikasi yang dibangun dalam sesi ini.

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
| BR-020 | Data deletion ikut retention policy | `PRD.md §11` | P1-9 (ADR-0012) | deletion/purge test | Planned |

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
| 1 | Tenant A baca resource miliknya | P1-10 jalur riil | Planned |
| 2 | Tenant A tak baca Tenant B | P1-10 jalur riil | Planned |
| 3 | Tenant A tak ubah/hapus Tenant B | P1-10 jalur riil | Planned |
| 4 | Tak baca conversation/AI config/ticket/order/file Tenant B | P1-10 jalur riil | Planned |
| 5 | Background job tak proses tenant salah | P1-10 worker minimum nyata | Planned |
| 6 | Cache tidak collision | P0-5, P1-10 | Planned |
| 7 | WebSocket/SSE tidak kirim event lintas tenant | P1-10 realtime minimum nyata | Planned |
| 8 | RLS bekerja pada pooled connection reuse | P1-7, P1-10 | Planned |
| 9 | App DB role tidak bisa bypass RLS | P1-7, P1-10 | Planned |

## 7. Open decisions (`PRD.md §20`)

| ID | Keputusan | Fase terblokir | Status |
|---|---|---|---|
| D-01 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | DISETUJUI |
| D-02 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-03 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-04 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-05 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-06 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
| D-07 | lihat `OPEN_DECISIONS.md` | lihat `BUILD_PLAN.md §6` | MENUNGGU |
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
