---
status: Accepted
date: 2026-11-11
decision-makers: Product owner (pemilik repo)
---

# ADR-0012 — Retensi dan purge data

## Context and Problem Statement

D-11 (`PRD.md §20`), `PRD.md §5.2` (explicit retention dan purge policy), BR-020 (deletion ikut retention policy).

## Decision Outcome

| Data | Retensi | Pemicu purge |
|---|---|---|
| Percakapan (conversation, session, message) | **30 hari** | 30 hari setelah pesan terakhir |
| Audit log | **1 tahun** | 365 hari setelah `created_at` |
| File tenant (object storage) | Selama tenant aktif | 30 hari setelah tenant ditutup, atau 30 hari setelah permintaan hapus |
| Data pelanggan (CRM) | Selama tenant aktif | 30 hari setelah tenant ditutup, atau 30 hari setelah permintaan hapus pelanggan |

Purge dijalankan worker (P1-9) sebagai job idempotent; angka disimpan di konfigurasi, bukan di-hardcode.

### Consequences

* Good: P1-9 purge/deletion dapat diimplementasikan dan diuji.
* Bad: 30 hari percakapan membatasi analitik percakapan jangka panjang (P6-6) → analitik harus memakai agregat yang disimpan terpisah, bukan isi pesan.
* Terbuka: retensi transaksi/order (kewajiban pajak/akuntansi) belum termasuk keputusan ini; ditetapkan sebelum P3. Audit log 1 tahun perlu dicek terhadap kewajiban hukum saat sign-off legal D-12.

## More Information

Status di `OPEN_DECISIONS.md`: D-11 `DISETUJUI` 2026-11-11.
