# Checklist rilis V1.0 — 30 butir (`PRD.md §18`, BUILD_PLAN Lampiran A, P8-6)

Diperbarui: 2026-11-13. Status dinilai dari bukti yang ada di repo, bukan dari niat. Gate go-live menuntut **30/30
Terpenuhi** + seluruh keputusan produk final + sign-off legal D-12 (BUILD_PLAN §5 "Gate Go-live").

Arti status:
- **Terpenuhi** — fitur ada, diuji otomatis (termasuk negative control), dan lulus gate fasenya.
- **Terpenuhi (catatan)** — terpenuhi untuk V1.0 dengan batasan tercatat yang tidak menghalangi checklist.
- **Sebagian** — artefak ada tetapi belum divalidasi di lingkungan produksi / masih ada pekerjaan.
- **Terblokir** — menunggu keputusan produk/ops atau pihak luar.

**Ringkasan: 24 Terpenuhi · 4 Sebagian · 2 Terblokir → belum siap go-live (24/30).**

| # | Kategori | Butir | Status | Bukti | Catatan / sisa pekerjaan |
|---:|---|---|---|---|---|
| 1 | SaaS foundation | Tenant registration dan authentication | Terpenuhi (catatan) | `docs/evidence/P1-2.txt`, `P1-3.txt`, `gate-P0.txt`, `P4-1b.txt` (MFA) | Email verifikasi/undangan memakai adapter log-only sampai D-08 diputus (AUTH-003/005/009/010 "Blocked by decision" untuk pengiriman email nyata). |
| 2 | SaaS foundation | Subdomain generation/resolution | Terpenuhi | `P1-4.txt`, `P2-7.txt` | ADR-0001/0014. TLS wildcard produksi menunggu provider DNS (lihat #25). |
| 3 | SaaS foundation | Membership dan granular RBAC | Terpenuhi | `P1-5.txt`, `P4-1.txt` | 37 permission, role kustom anti-eskalasi. |
| 4 | SaaS foundation | PostgreSQL RLS | Terpenuhi | `P1-7.txt`, `phase-1-isolation.txt` | 60 tabel tenant ber-RLS ENABLE+FORCE dijaga `crates/infrastructure/tests/rls.rs`. |
| 5 | SaaS foundation | Cross-tenant isolation suite | Terpenuhi | `P1-10.txt`, `phase-6-customer-service.txt` (isolasi #4) | 9/9 skenario `PRD.md §13.2` Verified (RTM §6). |
| 6 | Digital presence | Template selection dan page editor | Terpenuhi | `P2-2.txt`, `P2-3.txt` | |
| 7 | Digital presence | Business profile, hours, location | Terpenuhi | `P2-5.txt` | |
| 8 | Digital presence | Responsive public website | Terpenuhi | `P2-7.txt`, `phase-2-website.txt` | Playwright responsif 375/768/1024/1440px (AC-WEB-017). |
| 9 | Digital presence | Publish dan version rollback | Terpenuhi | `P2-4.txt` | |
| 10 | Commerce | Product catalog dan variants | Terpenuhi | `P3-1.txt` | |
| 11 | Commerce | Inventory movement/reservation | Terpenuhi | `P3-2.txt` | Overselling fixture lulus (Gate P3). |
| 12 | Commerce | Customer cart dan checkout | Terpenuhi | `P3-3.txt`, `P3-5.txt`, `phase-3-commerce.txt` | |
| 13 | Commerce | Order lifecycle dan approval | Terpenuhi | `P3-6.txt`, `P3-8.txt` | |
| 14 | Commerce | Payment integration atau verified manual payment workflow | Terpenuhi | `P3-7.txt` | Manual: QRIS statis + verifikasi staf (ADR-0004). |
| 15 | Operations | Multi-admin management | Terpenuhi | `P4-1.txt`, `P4-1b.txt` | |
| 16 | Operations | Ticketing dan assignment | Terpenuhi | `P4-3.txt`, `P6-5.txt` | |
| 17 | Operations | Approval engine | Terpenuhi | `P4-2.txt`, `P6-4.txt` (ai_response) | |
| 18 | Operations | Notification dan audit log | Terpenuhi | `P4-4.txt`, `P4-5.txt`, `P7-3.txt` | Audit 10/10 kategori `PRD.md §6.9`. Notifikasi email nyata menunggu D-08 (in-app & SSE aktif). |
| 19 | AI platform | OpenAI-compatible provider configuration | Terpenuhi | `P5-4.txt`, `phase-5-ai.txt` | Harga provider platform (`[ai.platform] *_cost_per_million_idr`) masih 0 → estimasi biaya NULL sampai diisi operator. |
| 20 | AI platform | Encrypted API credentials | Terpenuhi | `P5-2a-crypto.txt`, `P5-2b.txt` | |
| 21 | AI platform | AI orchestration dan tool registry | Terpenuhi | `P5-5.txt` | |
| 22 | AI platform | Knowledge base dan tenant-scoped RAG | Terpenuhi | `P5-7.txt` | |
| 23 | AI platform | Chat ID dan conversation viewer | Terpenuhi | `P6-1.txt`, `P6-2.txt`, `P6-3.txt` | |
| 24 | AI platform | AI-to-human handover | Terpenuhi | `P6-4.txt`, `P6-5.txt` | |
| 25 | Production | Rust API dan worker deployment | Sebagian | `P0-6.txt`, `P8-5.txt`, `docs/governance/runbook-deploy-rollback.md` | Image multi-stage + Compose (image bertag `API_IMAGE`/`WORKER_IMAGE`/`WEB_IMAGE`) + runbook deploy/rollback; `migrate` tidak memblokir rollback (diuji). **Belum** dideploy & drill di VPS; TLS wildcard menunggu provider DNS. |
| 26 | Production | VPS resource limits | Sebagian | `docker-compose.yml` (`deploy.resources.limits`) | Batas memori/CPU per service terdefinisi untuk VPS ±8 GB; **belum divalidasi** di bawah beban (P8-2). |
| 27 | Production | Automated external backups | Terblokir | `P0-10.txt`, `docs/governance/runbook-restore.md` | Backup harian terenkripsi + rotasi ada; **salinan luar VPS menunggu D-07 (provider eksternal)** — tanpa itu `backup.sh` sengaja keluar dengan kode 3. RPO aktual ±24 jam (tanpa WAL archiving). |
| 28 | Production | Monitoring dan alerting | Sebagian | `P0-8.txt`, `P8-5.txt`, `ops/monitoring/` | 16 metric aplikasi + metrik host (node-exporter) + 9 aturan alert (termasuk 3 untuk disk/memori host) teruji `promtool`; Alertmanager → **Telegram** (keputusan 2026-11-13; pengiriman diuji ke API tiruan). **Sisa:** token bot & chat on-call asli saat deploy; dashboard. |
| 29 | Production | Security assessment | Sebagian | `docs/security/threat-model.md` (STRIDE, P8-1 bagian internal) | Threat model & kontrol terpetakan; **pentest/assessment pihak ketiga belum** (syarat P8-1: zero critical/high). |
| 30 | Production | Load dan recovery testing | Terblokir | `P0-10.txt` (restore lokal) | Load 5 skenario (P8-2) butuh lingkungan, dataset, ambang & denominator yang **disetujui**; drill restore dengan target RPO/RTO (P8-4) butuh **target RPO/RTO** dan backup luar VPS (D-07). |

## Keputusan & pihak luar yang dibutuhkan untuk 30/30

| Butuh | Membuka butir | Pemilik |
|---|---|---|
| D-07: provider object storage/backup luar VPS | #27, #30 (sebagian) | Ops + Finance |
| Target RPO/RTO yang disetujui | #30 | Product + Ops |
| Lingkungan uji beban + dataset + ambang (NFR) | #26, #30 | Product + Ops |
| Provider DNS untuk TLS wildcard + VPS produksi | #25 | Ops |
| Token bot Telegram + chat on-call (kanal diputus: Telegram) | #28 | Ops |
| Pentest/assessment pihak ketiga | #29 | Security |
| D-08 email provider (bukan butir checklist, tetapi gate go-live: semua keputusan final) | #1, #18 catatan | Ops + Product |
| D-12: teks v1 dibuat sendiri & disetujui (`docs/evidence/D-12-legal-v1.txt`); sisa: isi kontak resmi + konfirmasi kebijakan refund & batas tanggung jawab | Gate go-live | Pemilik |

## Riwayat
- 2026-11-13: dokumen dibuat (P8-6 bagian checklist). Gate P0–P7 lulus.
- 2026-11-13: #25/#28 diperbarui setelah P8-5 bagian repo (runbook, deploy migrator, Alertmanager, node-exporter).
