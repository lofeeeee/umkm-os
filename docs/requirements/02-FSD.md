# Functional Specification (FSD) — UMKM Business OS

**Artefak:** #3 (`BUILD_PLAN.md §7`). **Versi:** 1.0, 2026-11-12.
**Sumber otoritatif:** `PRD.md §4.3`–`PRD.md §4.15` (+ `PRD.md §6.8` untuk upload). FSD ini **menurunkan**, bukan mengubah, PRD. Jika teks di sini bertentangan dengan PRD, PRD menang dan FSD diperbaiki.
**Aturan ID:** `FR-<AREA>-NNN` (requirement fungsional), `AC-<AREA>-NNN` (acceptance criterion yang dapat diuji). ID stabil: tidak dipakai ulang, tidak dinomori ulang; requirement yang dihapus ditandai `Dihapus`.
**Kolom "Keputusan":** cabang keputusan yang belum `DISETUJUI` tidak diturunkan menjadi perilaku; AC-nya ditandai `Contingent <D-xx>` dan baru dirinci setelah keputusan.

Status keputusan per 2026-11-12: D-05, ADR-0014 `DISETUJUI`; D-07 `DISETUJUI` sementara (lokal); D-02, D-03, D-04, D-06, D-09, D-10, D-08 `MENUNGGU`.

---

## 1. WEB — Website builder (`PRD.md §4.3`, ADR-0007, ADR-0014)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-WEB-001 | Katalog template dapat dilihat dan dipreview | AC-WEB-001, AC-WEB-002 | P2-2 |
| FR-WEB-002 | Template berversi; website tenant mengacu ke versi template tertentu | AC-WEB-003 | P2-2 |
| FR-WEB-003 | Page content = JSON schema tervalidasi server (ADR-0007) | AC-WEB-004, AC-WEB-005, AC-WEB-006 | P2-1 |
| FR-WEB-004 | Editor section: tambah, ubah data, urutkan, enable/disable section | AC-WEB-007, AC-WEB-008 | P2-3 |
| FR-WEB-005 | Pengaturan logo, warna, typography preset, teks, gambar, CTA | AC-WEB-009 | P2-3 |
| FR-WEB-006 | Draft dapat disimpan dan diedit berulang | AC-WEB-010 | P2-4 |
| FR-WEB-007 | Preview mobile/desktop di dashboard tanpa mengubah versi terbit | AC-WEB-011 | P2-4 |
| FR-WEB-008 | Publish membuat versi immutable baru | AC-WEB-012, AC-WEB-013 | P2-4 |
| FR-WEB-009 | Rollback mengembalikan versi yang dipilih | AC-WEB-014 | P2-4 |
| FR-WEB-010 | Halaman publik responsif, hanya versi terbit, dari Host | AC-WEB-015, AC-WEB-016, AC-WEB-017 | P2-7 |
| FR-WEB-011 | Arbitrary HTML/JS tidak pernah dieksekusi di website tenant | AC-WEB-005, AC-WEB-018 | P2-1/P2-7 |

| AC | Kriteria (Given / When / Then) |
|---|---|
| AC-WEB-001 | Given user dengan `website.read`, When GET daftar template, Then hanya template berstatus aktif dikembalikan beserta `key`, `name`, `category`, `version` |
| AC-WEB-002 | Given template, When preview template diminta, Then konten contoh template dirender tanpa membuat/mengubah website tenant |
| AC-WEB-003 | Given tenant memilih template versi N, When template versi N+1 diterbitkan platform, Then website tenant tetap memakai N sampai tenant memilih ulang |
| AC-WEB-004 | Given `content_json` valid sesuai ADR-0007, When disimpan sebagai draft, Then diterima dan tersimpan apa adanya (normalisasi kanonik) |
| AC-WEB-005 | Given `content_json` dengan field tak dikenal, tipe section tak dikenal, `schema_version ≠ 1`, URL `javascript:`/`data:`, teks melebihi batas, > 30 section, atau id ganda, When disimpan/dipublish, Then ditolak `422 VALIDATION_FAILED` (envelope P0) dengan path field, dan tidak ada yang tersimpan |
| AC-WEB-006 | Given input acak (fuzz), When divalidasi, Then validator tidak panic dan hanya menerima dokumen yang lolos aturan |
| AC-WEB-007 | Given draft dengan section A,B,C, When urutan disimpan C,A,B, Then draft memuat urutan C,A,B |
| AC-WEB-008 | Given section `enabled=false`, When versi itu dipublish dan dibuka publik, Then section tidak dirender |
| AC-WEB-009 | Given `theme.primary` bukan `#RRGGBB` atau font di luar preset, When disimpan, Then ditolak; Given `asset:<uuid>` milik tenant lain atau tidak ada, Then ditolak |
| AC-WEB-010 | Given draft tersimpan, When diedit dan disimpan lagi, Then draft terbaru menggantikan draft lama dan versi terbit tidak berubah |
| AC-WEB-011 | Given versi terbit V1 dan draft berbeda, When preview dibuka, Then dashboard menampilkan draft dan publik tetap menampilkan V1 (`PRD.md §13.3` Website) |
| AC-WEB-012 | Given user dengan `website.publish`, When publish, Then versi baru N+1 dibuat, pointer terbit menunjuk N+1, audit `website.publish` tercatat; user tanpa `website.publish` (mis. Manager) → 404/403 sesuai aturan RBAC |
| AC-WEB-013 | Given versi terbit, When ada upaya mengubah baris versi (UPDATE/DELETE) lewat app role, Then ditolak database |
| AC-WEB-014 | Given versi 1..3 dan terbit = 3, When rollback ke 1, Then pointer terbit = 1, publik menampilkan konten versi 1, versi 2–3 tetap ada, audit `website.rollback` tercatat; rollback ke versi milik tenant lain → 404 |
| AC-WEB-015 | Given Host `{slug}.ryasai.my.id` aktif dan website terbit, When GET publik, Then konten versi terbit dikembalikan; tanpa versi terbit → 404 |
| AC-WEB-016 | Given Host tak dikenal/suspended/disabled, When GET publik, Then 404 tanpa membocorkan keberadaan tenant (TEN-009) |
| AC-WEB-017 | Given halaman publik, When dirender pada lebar 375/768/1024/1440, Then tidak ada scroll horizontal dan semua section aktif terlihat |
| AC-WEB-018 | Given teks section berisi `<script>`/HTML, When dirender publik, Then tampil sebagai teks literal; respons membawa CSP ADR-0014 |

## 2. BIZ — Business profile (`PRD.md §4.4`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-BIZ-001 | Nama, deskripsi, logo, cover, telepon, email, WhatsApp, social links | AC-BIZ-001, AC-BIZ-002 | P2-5 |
| FR-BIZ-002 | Satu atau lebih alamat, koordinat opsional | AC-BIZ-003 | P2-5 |
| FR-BIZ-003 | Jam operasional per hari | AC-BIZ-004 | P2-5 |
| FR-BIZ-004 | Hari libur / exception hours | AC-BIZ-005 | P2-5 |
| FR-BIZ-005 | Status buka/tutup dihitung dari timezone tenant | AC-BIZ-006 | P2-5 |
| FR-BIZ-006 | Informasi pickup/delivery bila tersedia | AC-BIZ-007 | P2-5 |

| AC | Kriteria |
|---|---|
| AC-BIZ-001 | Given `tenant.update`, When profil diubah dengan data valid, Then tersimpan dan audit `business.update` tercatat; tanpa permission → ditolak |
| AC-BIZ-002 | Given email/telepon/URL social tidak valid, When disimpan, Then `422 VALIDATION_FAILED`; logo/cover hanya `asset:<uuid>` milik tenant |
| AC-BIZ-003 | Given latitude ∉ [-90,90] atau longitude ∉ [-180,180], When disimpan, Then ditolak; alamat tanpa koordinat diterima |
| AC-BIZ-004 | Given jam buka ≥ jam tutup pada hari yang tidak melewati tengah malam, When disimpan, Then ditolak; maksimal satu baris per `day_of_week` |
| AC-BIZ-005 | Given exception tanggal D `is_closed=true`, When status dihitung pada D, Then tutup meski jam reguler buka |
| AC-BIZ-006 | Given timezone `Asia/Jakarta` dan jam 08:00–17:00, When waktu UTC 09:30 (16:30 WIB), Then `open`; UTC 10:30 → `closed`; timezone tidak valid ditolak |
| AC-BIZ-007 | Given `pickup_available`/`delivery_available` + catatan, When dibaca publik, Then dikembalikan apa adanya |

## 3. FILE — Upload & object storage (`PRD.md §6.8`, `PRD.md §6.3`, ADR-0009)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-FILE-001 | Upload gambar tenant dengan validasi tipe/ukuran server-side | AC-FILE-001, AC-FILE-002 | P2-6 |
| FR-FILE-002 | Key storage acak, tidak membuka path internal | AC-FILE-003 | P2-6 |
| FR-FILE-003 | Akses via signed URL ber-expiry dan ber-scope tenant | AC-FILE-004, AC-FILE-005 | P2-6 |
| FR-FILE-004 | Objek terisolasi per tenant (RLS) | AC-FILE-006 | P2-6 |

| AC | Kriteria |
|---|---|
| AC-FILE-001 | Given PNG/JPEG/WebP ≤ 2 MB, When diupload user dengan `website.edit`, Then tersimpan dan id objek dikembalikan |
| AC-FILE-002 | Given file > 2 MB (→ 413), SVG, HTML berganti nama `.png`, polyglot, atau magic bytes tak cocok, When diupload, Then ditolak dan tidak ada yang tersimpan; kuota objek tenant terlampaui → ditolak |
| AC-FILE-003 | Given objek tersimpan, When respons/URL diperiksa, Then tidak memuat nama file asli atau path internal |
| AC-FILE-004 | Given signed URL kedaluwarsa atau signature diubah, When diakses, Then 404 |
| AC-FILE-005 | Given signed URL tenant A, When `object_id` diganti objek tenant B (atau `t` diganti B), Then 404 |
| AC-FILE-006 | Given app role dengan `app.tenant_id = A`, When SELECT objek B, Then 0 baris (RLS) |

## 4. CAT / INV — Product catalog & inventory (`PRD.md §4.5`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-CAT-001 | Produk create/update/archive/list, status draft/active/archived | AC-CAT-001 | P3-1 |
| FR-CAT-002 | Kategori + subkategori | AC-CAT-002 | P3-1 |
| FR-CAT-003 | SKU unik per tenant | AC-CAT-003 | P3-1 |
| FR-CAT-004 | Deskripsi, harga, harga promo (`NUMERIC`), varian & opsi, foto, availability | AC-CAT-004 | P3-1 |
| FR-INV-001 | Stock in/out/adjustment dengan movement history lengkap | AC-INV-001 | P3-2 |
| FR-INV-002 | Minimum stock threshold + low-stock event | AC-INV-002 | P3-2 |
| FR-INV-003 | Reservation untuk checkout | AC-INV-003 | P3-2 |
| FR-INV-004 | Stok negatif nonaktif secara default | AC-INV-004 | P3-2 |
| FR-INV-005 | Mutasi transactional, aman dari race/overselling | AC-INV-005 | P3-2 |

| AC | Kriteria |
|---|---|
| AC-CAT-001 | Given produk archived, When katalog publik dibaca, Then tidak tampil; transisi status hanya draft→active→archived dan archived→active |
| AC-CAT-002 | Given kategori induk milik tenant lain, When dijadikan parent, Then ditolak (composite FK); siklus parent ditolak |
| AC-CAT-003 | Given SKU X ada di tenant A, When tenant A membuat SKU X lagi, Then ditolak; tenant B boleh SKU X |
| AC-CAT-004 | Given harga promo > harga normal atau harga negatif, When disimpan, Then ditolak |
| AC-INV-001 | Given mutasi stok, Then movement menyimpan actor, tenant, item/variant, delta, reason, reference, timestamp; movement tidak dapat diubah |
| AC-INV-002 | Given stok turun di bawah threshold, Then satu event low-stock masuk outbox |
| AC-INV-003 | Given reservasi kedaluwarsa, Then stok tersedia kembali |
| AC-INV-004 | Given policy default, When mutasi membuat stok < 0, Then ditolak |
| AC-INV-005 | Given stok 1 dan N checkout paralel, Then tepat 1 berhasil (`PRD.md §13.3` Commerce) |

## 5. CRM — Customer (`PRD.md §4.6`) — **Contingent D-03**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-CRM-001 | Guest dan/atau registered customer | AC-CRM-001 (Contingent D-03) | P3-3 |
| FR-CRM-002 | Profil nama/email/telepon, alamat | AC-CRM-002 | P3-3 |
| FR-CRM-003 | Order history, conversation/ticket history | AC-CRM-003 | P3-3 |
| FR-CRM-004 | Tags, internal notes, pencarian | AC-CRM-004 | P3-3 |
| FR-CRM-005 | Account linking hanya setelah verifikasi | AC-CRM-005 (Contingent D-03) | P3-3 |
| FR-CRM-006 | Export/deletion mengikuti retention (ADR-0012) | AC-CRM-006 | P3-3 |

| AC | Kriteria |
|---|---|
| AC-CRM-001 | Contingent D-03 — dirinci setelah keputusan guest/registered/linking |
| AC-CRM-002 | Given customer tenant A, When tenant B mencari email yang sama, Then tidak ditemukan |
| AC-CRM-003 | Given customer, Then history hanya memuat order/ticket tenant yang sama |
| AC-CRM-004 | Given internal note, Then tidak pernah terlihat oleh customer |
| AC-CRM-005 | Contingent D-03 — linking tanpa verifikasi ditolak |
| AC-CRM-006 | Given permintaan hapus, Then data customer di-purge +30 hari (ADR-0012) |

## 6. ORD — Cart, checkout, order (`PRD.md §4.7`) — payment **Contingent D-02**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-ORD-001 | Cart: tambah item, ubah quantity | AC-ORD-001 | P3-4 |
| FR-ORD-002 | Server menghitung ulang harga, promo, ongkir, total | AC-ORD-002 | P3-5 |
| FR-ORD-003 | Snapshot item order immutable | AC-ORD-003 | P3-4 |
| FR-ORD-004 | 13 status, transisi eksplisit & tervalidasi | AC-ORD-004 | P3-6 |
| FR-ORD-005 | Approval order dengan alasan rejection, actor, SoD, notifikasi | AC-ORD-005 | P3-8 |
| FR-ORD-006 | Idempotency key checkout | AC-ORD-006 | P3-5 |
| FR-ORD-007 | Payment adapter + webhook signature + dedup | AC-ORD-007 (Contingent D-02) | P3-7 |
| FR-ORD-008 | Refund audit trail | AC-ORD-008 | P3-7 |
| FR-ORD-009 | Pickup/delivery bila tersedia; customer melihat status order | AC-ORD-009 | P3-5 |

| AC | Kriteria |
|---|---|
| AC-ORD-001 | Given quantity ≤ 0 atau produk non-active, Then ditolak |
| AC-ORD-002 | Given client mengirim harga/total, Then diabaikan; total = perhitungan server |
| AC-ORD-003 | Given order dibuat lalu nama/harga produk diubah, Then item order tidak berubah |
| AC-ORD-004 | Given transisi tidak terdaftar (mis. COMPLETED→PENDING), Then ditolak |
| AC-ORD-005 | Given SoD aktif, When pemohon meng-approve permintaannya sendiri, Then ditolak; reject tanpa alasan ditolak |
| AC-ORD-006 | Given idempotency key sama dikirim dua kali, Then tepat 1 order |
| AC-ORD-007 | Contingent D-02 — signature salah ditolak; event ganda diproses sekali |
| AC-ORD-008 | Given refund, Then audit memuat actor, jumlah, alasan |
| AC-ORD-009 | Given order customer, When dilihat customer lain, Then tidak dapat diakses |

## 7. TKT — Ticketing (`PRD.md §4.8`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-TKT-001 | Ticket dari chat, order issue, manual, system event | AC-TKT-001 | P4-3 |
| FR-TKT-002 | Field ticket lengkap; lifecycle 7 status | AC-TKT-002 | P4-3 |
| FR-TKT-003 | Assign/reassign, priority, tags | AC-TKT-003 | P4-3 |
| FR-TKT-004 | Public reply vs internal note; attachment | AC-TKT-004 | P4-3 |
| FR-TKT-005 | Search/filter, history, SLA timestamp/target, reopen sesuai policy | AC-TKT-005 | P4-3 |
| FR-TKT-006 | Ticket ≠ conversation; 1 conversation → ≥ 1 ticket | AC-TKT-006 | P6-5 |

| AC | Kriteria |
|---|---|
| AC-TKT-001 | Given setiap sumber, Then ticket mencatat `source` dan referensinya dalam tenant yang sama |
| AC-TKT-002 | Given transisi tidak terdaftar, Then ditolak; `resolved_at` terisi saat RESOLVED |
| AC-TKT-003 | Given assignee bukan member tenant, Then ditolak |
| AC-TKT-004 | Given internal note, Then tidak pernah terlihat customer |
| AC-TKT-005 | Given setiap perubahan, Then masuk history; reopen setelah CLOSED mengikuti policy |
| AC-TKT-006 | Given ticket ditutup, Then conversation tidak ikut tertutup |

## 8. APR — Approval engine (`PRD.md §4.9`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-APR-001 | Approval reusable untuk 6 entity type | AC-APR-001 | P4-2 |
| FR-APR-002 | Single + sequential multi-step, rejection reason, timeout, delegation, history | AC-APR-002 | P4-2 |
| FR-APR-003 | Cek permission, assignment, state, tenant; tidak bisa di-bypass | AC-APR-003 | P4-2 |

| AC | Kriteria |
|---|---|
| AC-APR-001 | Given entity order/refund/stock adjustment/price change/AI response/business config, Then dapat membuat approval request |
| AC-APR-002 | Given 2 step, When step 2 di-approve sebelum step 1, Then ditolak; timeout → status expired |
| AC-APR-003 | Given update status entity langsung tanpa approval yang disyaratkan, Then ditolak |

## 9. AIC — AI provider & config (`PRD.md §4.10`) — **Contingent D-09**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-AIC-001 | Konfigurasi AI per tenant (16 parameter `PRD.md §4.10`) | AC-AIC-001 | P5 |
| FR-AIC-002 | Provider target + capability registry | AC-AIC-002 | P5-4 |
| FR-AIC-003 | API key terenkripsi, masked, tidak di log, rotasi/disable | AC-AIC-003 | P5-2 |
| FR-AIC-004 | Mode Auto/Assisted/Hybrid/Human Only, default Hybrid | AC-AIC-004 | P5-9 |

| AC | Kriteria |
|---|---|
| AC-AIC-001 | Given nilai di luar batas (temperature, max tokens, timeout), Then ditolak |
| AC-AIC-002 | Given provider tanpa tool calling, Then tool tidak dikirim ke provider itu |
| AC-AIC-003 | Given GET config, Then key hanya masked; log tidak memuat key |
| AC-AIC-004 | Given Human Only, Then AI tidak pernah mengirim jawaban |

## 10. AIO — AI orchestration & tools (`PRD.md §4.11`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-AIO-001 | Pipeline 9 tahap | AC-AIO-001 | P5-5 |
| FR-AIO-002 | 10 tool awal dengan level permission | AC-AIO-002 | P5-6 |
| FR-AIO-003 | LLM tanpa SQL/HTTP bebas/credential/admin API; output untrusted | AC-AIO-003 | P5-5 |
| FR-AIO-004 | Retrieval tenant-scoped | AC-AIO-004 | P5-7 |

| AC | Kriteria |
|---|---|
| AC-AIO-001 | Given guardrail menolak input, Then provider tidak dipanggil |
| AC-AIO-002 | Given tool dengan parameter invalid atau di luar permission, Then ditolak backend |
| AC-AIO-003 | Given prompt injection meminta tool tak diizinkan, Then ditolak |
| AC-AIO-004 | Given tenant A bertanya, Then tidak ada chunk tenant B |

## 11. CNV — Conversation (`PRD.md §4.12`) — channel **Contingent D-04**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-CNV-001 | UUID terpisah untuk conversation/session/message; 4 sender type | AC-CNV-001 | P6-1 |
| FR-CNV-002 | Viewer: timeline, context, provider/model, token/latency, tool summary, assignment, status, approval | AC-CNV-002 | P6-3 |
| FR-CNV-003 | Masking data sensitif sesuai permission | AC-CNV-003 | P6-3 |
| FR-CNV-004 | Analytics conversation 8 metrik | AC-CNV-004 | P6-6 |

| AC | Kriteria |
|---|---|
| AC-CNV-001 | Given sender type di luar 4 nilai, Then ditolak |
| AC-CNV-002 | Given conversation, Then viewer menampilkan seluruh field FR-CNV-002 |
| AC-CNV-003 | Given Viewer role, Then field sensitif dimasking |
| AC-CNV-004 | Given data dua tenant, Then agregasi hanya tenant sendiri |

## 12. NTF — Notification (`PRD.md §4.13`) — email **Contingent D-08**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-NTF-001 | Channel in-app, email, SSE/WebSocket | AC-NTF-001 | P4-4 |
| FR-NTF-002 | 9 event notifikasi | AC-NTF-002 | P4-4 |
| FR-NTF-003 | Transactional outbox, delivery idempotent, retry/backoff | AC-NTF-003 | P1-6/P4-4 |

| AC | Kriteria |
|---|---|
| AC-NTF-001 | Given event tenant A, Then tidak terkirim ke subscriber tenant B |
| AC-NTF-002 | Given setiap event dari 9 event, Then tepat satu notifikasi per penerima |
| AC-NTF-003 | Given worker crash setelah kirim sebelum ack, Then tidak ada kiriman ganda yang terlihat penerima (idempotency key) |

## 13. ANL — Analytics & platform admin (`PRD.md §4.14`)

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-ANL-001 | 9 metrik analytics tenant | AC-ANL-001 | P7 |
| FR-ANL-002 | 6 metrik platform, hanya platform role berwenang | AC-ANL-002 | P7 |

| AC | Kriteria |
|---|---|
| AC-ANL-001 | Given data dua tenant, Then metrik tenant hanya dari tenant sendiri |
| AC-ANL-002 | Given user tenant (termasuk Owner), When akses agregasi platform, Then ditolak |

## 14. SUB — Subscription & limits (`PRD.md §4.15`) — angka **Contingent D-10**

| FR | Requirement | AC | Fase |
|---|---|---|---|
| FR-SUB-001 | Plan/limit foundation tanpa hardcode limit | AC-SUB-001 | P7 |
| FR-SUB-002 | AI usage dihitung token dan/atau estimasi biaya | AC-SUB-002 | P5-8 |

| AC | Kriteria |
|---|---|
| AC-SUB-001 | Given limit plan diubah di `plans.limits_json`, Then enforcement berubah tanpa deploy |
| AC-SUB-002 | Given panggilan AI, Then usage menyimpan token input/output dan estimasi biaya |

---

## Lampiran — Catatan derivasi

* Kuota 200 objek/tenant (ADR-0009) adalah batas sementara sampai D-10; disimpan sebagai konfigurasi, bukan konstanta di kode bisnis (FR-SUB-001).
* `FR-FILE-*` bersumber dari `PRD.md §6.8`/`§6.3` karena P2-6 membutuhkannya; area lain `§6` tetap di dokumen keamanan.
