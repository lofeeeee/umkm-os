# Register Keputusan Gate 0 — UMKM Business OS

**Status (2026-11-12):** D-01, D-05, D-11, ADR-0014 `DISETUJUI`; D-07 `DISETUJUI` sementara (adapter lokal); D-12 bagian privacy/terms registrasi `DISETUJUI` bersyarat; D-08 ditunda (tetap `MENUNGGU`). Sisanya `MENUNGGU`.
**Sumber:** `PRD.md §20` — *Open Decisions — Must Be Resolved Before Implementation*.
**Status vocabulary:** `MENUNGGU` / `DISETUJUI` / `DITOLAK`. `BUILD_PLAN.md` memakai vocabulary yang sama. Template MADR di dalam ADR boleh memakai `Proposed/Accepted/...` sebagai lifecycle dokumen, bukan status keputusan produk.
**Aturan kontrak:** jika dua pembacaan sah mengubah perilaku, ini contract question untuk pemilik produk. Tidak ada perilaku yang diubah berdasarkan asumsi.

---

## Ringkasan blokir fase

| Keputusan | Menghalangi | Prioritas |
|---|---|---|
| ADR-0002, ADR-0013 | P0-1 bentuk workspace | **DISETUJUI 2026-11-11** |
| D-01 brand/root domain | P0-6 TLS produksi, P1 | **DISETUJUI** (TLS menunggu provider DNS) |
| D-08 email provider | P1 verification/invitation/reset | Ditunda; adapter log-only |
| D-11 retention | P1 purge/deletion | **DISETUJUI** |
| D-12 privacy/terms registrasi | P1 | **DISETUJUI bersyarat** |
| D-12 consent AI | P5 | Sebelum P5 |
| D-12 privacy/terms transaksi | P3 | Sebelum P3 |
| D-12 legal billing | P7 | Sebelum P7 |
| D-12 sign-off | Go-live | Go-live |
| D-05 editor depth | P2 | **DISETUJUI** 2026-11-12 (section-based) |
| D-07 storage | P2 | **DISETUJUI sementara** 2026-11-12 (lokal PostgreSQL) |
| ADR-0014 frontend contract | P2 | **DISETUJUI** 2026-11-12 |
| D-02 payment | P3 | Sebelum P3 |
| D-03 customer identity | P3/P6 | Sebelum P3 |
| D-09 AI key policy | P5 | Sebelum P5 |
| D-04 messaging channel | P6 | Sebelum P6 |
| D-06 custom domain | P7 / status V1.0 | Sebelum P7 |
| D-10 pricing | P7 | Sebelum P7 |

**Yang dapat dimulai tanpa keputusan produk:** hanya P0-0 repo skeleton netral. Task yang menyentuh bentuk workspace menunggu ADR-0013; Caddy/TLS produksi menunggu D-01.

---

## Daftar keputusan

| ID | Keputusan (`PRD.md §20`) | Rekomendasi engineering (bukan keputusan) | Konsekuensi jika ditolak | Blokir | ADR / log | Owner | Status |
|---|---|---|---|---|---|---|---|
| D-01 | Brand, root domain, format subdomain | `platform.id` hanya contoh; tetapkan root domain sebelum Caddy/TLS wildcard | sertifikat wildcard dan `tenant_domains.hostname` harus dimigrasi ulang | P0, P1 | ADR-0001 | Product + Ops | **DISETUJUI** 2026-11-11: `ryasai.my.id`, tenant `{slug}.ryasai.my.id` (ADR-0001). Terbuka: provider DNS untuk TLS wildcard |
| D-02 | MVP payment manual/COD atau gateway | Manual transfer/COD + verified manual workflow; adapter tetap provider-neutral | provider/webhook/PCI masuk lebih awal | P3 | ADR-0004 | Product + Finance | MENUNGGU |
| D-03 | Customer identity guest/account/linking | Guest + account linking setelah verifikasi; cabang lain didokumentasikan | registered-only mengubah onboarding; guest-only membatasi `get_customer_orders` | P3, P6 | ADR-0005 | Product | MENUNGGU |
| D-04 | Messaging channel web chat/WhatsApp | Web chat dulu; WhatsApp lewat adapter fase lanjutan | butuh provider messaging, opt-in/consent, webhook | P6 | ADR-0006 | Product | MENUNGGU |
| D-05 | Website builder section-based/canvas penuh | Section-based sebagai default usulan; R-03 tetap contract question | canvas penuh mengubah `content_json`, renderer, rollback | P2 | ADR-0007 | Product + Design | **DISETUJUI** 2026-11-12: section-based / stacked blocks (ADR-0007) |
| D-06 | Custom domain masuk MVP atau lanjutan | Fase lanjutan; P7-4 conditional/out-of-V1.0 | DNS ownership, TLS per domain, lifecycle anti-takeover | P7 / V1.0 scope | ADR-0008 | Product + Ops | MENUNGGU |
| D-07 | Storage S3-compatible + biaya | Provider eksternal; bukan MinIO di VPS yang sama | budget 8 GB dan RPO dihitung ulang | P2 | ADR-0009 | Ops + Finance | **DISETUJUI sementara** 2026-11-12: "local database for now" → adapter PostgreSQL di belakang port provider-neutral (ADR-0009). Terbuka: provider eksternal + off-site backup |
| D-08 | Email provider | Transactional email + SPF/DKIM/DMARC + sandbox; **outbox tetap provider-neutral** | tanpa ini AUTH-003/005/009/010 tidak bisa diuji end-to-end | P1 | ADR-0010 | Ops + Product | MENUNGGU — **ditunda** 2026-11-11 (product-decision-log); P1-6 memakai adapter log-only |
| D-09 | AI provider BYOK/platform key/keduanya | BYOK dulu; platform key menyusul | cost metering/quota/liability lebih awal | P5 | ADR-0011 | Product + Finance + Legal | MENUNGGU |
| D-10 | Subscription pricing/limit/cycle | Simpan limit di `plans.limits_json`; angka final menyusul | billing integration tertahan | P7 | Product decision log | Product + Finance | MENUNGGU |
| D-11 | Retention conversation/audit/file/customer | Tetapkan angka sebelum P1 | purge/deletion/export tidak bisa Done | P1 | ADR-0012 | Legal + Ops | **DISETUJUI** 2026-11-11: percakapan 30 hari, audit 1 tahun, file/data pelanggan +30 hari setelah tenant tutup/permintaan hapus (ADR-0012) |
| D-12 | Legal/compliance | Pecah: privacy/terms registrasi (P1), privacy/terms transaksi (P3), consent AI (P5), legal billing (P7), sign-off go-live | masing-masing gate tertahan bila belum `DISETUJUI` | P1/P3/P5/P7/Go-live | Legal decision log | Legal | Privacy/terms registrasi **DISETUJUI bersyarat** 2026-11-11 (teks final sebelum go-live); bagian lain MENUNGGU |

---

## Cara memutus

1. Ubah **Status** menjadi `DISETUJUI` atau `DITOLAK`, cantumkan tanggal + pemutus. Jangan ubah sebelum keputusan benar-benar diambil.
2. Untuk keputusan arsitektural sulit dibalik, buat ADR MADR 4.0.0 append-only di `docs/decisions/NNNN-judul.md`.
3. Untuk keputusan produk murni, cukup product decision log bertanda tangan.
4. Update `BUILD_PLAN.md §6`, Lampiran A/B, dan `RTM.md` setelah keputusan berubah. ADR lama tidak ditimpa; buat ADR baru yang menautkannya.

## ADR yang diusulkan

ADR-0002 dan ADR-0013 `DISETUJUI` 2026-11-11 (Gate A). Sisanya belum `DISETUJUI`.

| ADR | Judul | Alasan |
|---|---|---|
| ADR-0001 | Root domain dan format subdomain | D-01 |
| ADR-0002 | Penyesuaian urutan phase untuk fondasi lintas-cutting | `BUILD_PLAN.md §1` — **DISETUJUI 2026-11-11**, `docs/decisions/0002-fondasi-lintas-cutting-lebih-awal.md` |
| ADR-0003 | Strategi isolasi tenant: RLS + application authorization | `PRD.md §5.7`, `PRD.md §6.3` |
| ADR-0004 | MVP payment workflow | D-02 |
| ADR-0005 | Customer identity dan account linking | D-03 |
| ADR-0006 | Messaging channel MVP | D-04 |
| ADR-0007 | Page schema dan batasan editor | D-05 / R-03 — **DISETUJUI 2026-11-12** |
| ADR-0008 | Custom domain lifecycle dan status V1.0 | D-06 / R-04 |
| ADR-0009 | Object storage dan signed URL | D-07 — **DISETUJUI sementara 2026-11-12** |
| ADR-0010 | Email provider dan outbox delivery | D-08 / R-02 |
| ADR-0011 | AI credential ownership | D-09 |
| ADR-0012 | Retention dan purge policy | D-11 |
| ADR-0013 | Batas crate workspace | `BUILD_PLAN.md §3.2` — **DISETUJUI 2026-11-11** (5 crate inti), `docs/decisions/0013-batas-crate-workspace.md` |
| ADR-0014 | Kontrak frontend, Host/domain-registry, Caddy, API | R-01 — **DISETUJUI 2026-11-12**, `docs/decisions/0014-kontrak-frontend-host-caddy-api.md` |
| ADR-0015 | Transport sesi: cookie HttpOnly host-only + CSRF | P1-3 — **DISETUJUI 2026-11-11**, `docs/decisions/0015-transport-sesi-cookie.md` |
