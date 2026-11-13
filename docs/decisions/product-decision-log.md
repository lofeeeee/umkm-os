# Product decision log (append-only)

Keputusan produk yang tidak membutuhkan ADR. Entri tidak ditimpa; perubahan = entri baru.

## 2026-11-11 — D-08 email provider: DITUNDA

- Pemutus: Product owner. Status tetap `MENUNGGU` (ditunda secara eksplisit).
- Konsekuensi: P1-6 membangun outbox provider-neutral + adapter dev yang hanya mencatat ke log (tidak mengirim email). AUTH-003/005/009/010 tidak dapat diuji end-to-end dan tetap `Blocked by decision`. Go-live diblokir sampai D-08 diputus.

## 2026-11-11 — D-12 privacy policy & terms untuk registrasi: DISETUJUI bersyarat

- Pemutus: Product owner.
- Disetujui: registrasi **wajib** menyetujui privacy policy + terms; versi dokumen dan waktu persetujuan disimpan per user.
- Syarat: teks dokumen saat ini placeholder dan **wajib diganti teks final yang di-review legal sebelum go-live**. Bagian D-12 lain (transaksi P3, consent AI P5, billing P7, sign-off go-live) tetap `MENUNGGU`.

## 2026-11-11 — Subdomain ryasai.my.id yang sudah terpakai: DITUNDA

- Pemutus: Product owner ("nanti kalau sudah ada").
- Konsekuensi / syarat: sebelum registrasi tenant dibuka untuk publik, daftar subdomain yang dipakai layanan lain di `ryasai.my.id` WAJIB ditambahkan ke `reserved_subdomains` (migration baru). Dicatat juga di ADR-0001 sebagai item terbuka.

## 2026-11-11 — Preset permission 6 role (P1-5): DISETUJUI

- Pemutus: Product owner (usulan engineering disetujui tanpa perubahan).
- Owner 33 (semua). Manager 26: semua kecuali `tenant.update`, `members.invite`, `members.manage`, `website.publish`, `ai_config.manage`, `ai_provider.manage`, `audit.read`. Support Agent 9: `tenant.read`, `tickets.read/reply/close`, `conversations.read/reply/takeover`, `orders.read`, `products.read` (+ object-level: hanya yang ditugaskan, saat modul P4/P6 ada). Catalog Manager 7: `tenant.read`, `products.*`, `inventory.*`. Viewer 7: `tenant.read`, `website.read`, `products.read`, `inventory.read`, `orders.read`, `tickets.read`, `analytics.read`. Admin 1: `tenant.read` (hak lain diberikan per anggota lewat role kustom).
- Sumber kebenaran: `migrations/0005_rbac_presets.up.sql`; diuji matriks 6 × 33 di `crates/infrastructure/tests/rbac.rs`.

## 2026-11-12 — Gate C: D-05, D-07, ADR-0014

- Pemutus: Product owner ("stacked block, local database for now, ur suggestion").
- D-05: **DISETUJUI** — editor section-based / stacked blocks (ADR-0007).
- D-07: **DISETUJUI sementara** — penyimpanan file lokal di PostgreSQL di belakang port provider-neutral (ADR-0009). Provider S3 eksternal + salinan backup off-site tetap terbuka.
- ADR-0014: **DISETUJUI** usulan engineering — static export dilayani Caddy, data tenant dari API berdasarkan Host (ADR-0014).

## 2026-11-12 — Gate D: D-02, D-03, D-12 transaksi

- Pemutus: Product owner ("1. Qris Gopay, 2. the last one, 3. Approved"; pembacaan QRIS dikonfirmasi: "Static QRIS").
- D-02: **DISETUJUI** — QRIS statis GoPay milik tenant, pembeli membayar langsung ke merchant tenant, staf memverifikasi manual (ADR-0004). Tanpa gateway/webhook di V1.
- D-03: **DISETUJUI** — guest checkout + akun customer opsional; linking hanya setelah email terverifikasi (ADR-0005). Verifikasi email nyata tetap menunggu D-08.
- D-12 privacy/terms transaksi: **DISETUJUI bersyarat** — checkout wajib menyetujui privacy + terms transaksi (versi + waktu disimpan di order); teks placeholder wajib diganti teks final hasil review legal sebelum go-live.
- Perluasan preset permission (engineering, mengikuti pola role 2026-11-11): `payments.verify` (Owner, Manager), `payments.refund` (Owner), `customers.read` (Owner, Manager, Support Agent, Viewer), `customers.manage` (Owner, Manager). Total permission 33 → 37.

## 2026-11-12 — Gate E: D-09, D-12 consent AI

- Pemutus: Product owner ("1. Both, it can use platform as a provider or they bring one. 2. Sure").
- D-09: **DISETUJUI** — tenant dapat memakai provider AI platform (biaya platform, dibatasi kuota token bulanan per
  tenant) atau membawa kunci/endpoint sendiri (BYOK). Detail di ADR-0011.
- D-12 consent AI: **DISETUJUI bersyarat** — pelanggan diberi tahu sedang berbicara dengan asisten AI dan persetujuan
  dicatat per percakapan; teks placeholder wajib diganti teks final hasil review legal sebelum go-live.
- MFA (AUTH-011/012): belum diputus pemilik; enkripsi envelope P5-2 dirancang agar dapat dipakai MFA.

### 2026-11-12 — Implementasi consent AI (placeholder)

- Teks pemberitahuan AI placeholder di `crates/domain/src/ai/consent.rs` (`VERSION = ai-consent-placeholder-v0`):
  sedang bicara dengan asisten AI, jawaban bisa keliru, frasa "bicara dengan admin" untuk staf (dijamin guardrail),
  retensi pesan, larangan mengirim kata sandi/PIN/kartu. P6 menampilkan teks ini di widget chat dan mencatat versi +
  waktu persetujuan per percakapan. Wajib diganti teks final hasil review legal sebelum go-live.

## 2026-11-12 — Gate F: D-04, MFA

- Pemutus: Product owner ("1. Whatsapp/telegram later, 2. sure").
- D-04: **DISETUJUI** — web chat dulu; WhatsApp/Telegram lewat adapter fase lanjutan (ADR-0006).
- MFA (AUTH-011/012): **DISETUJUI** — TOTP (RFC 6238, SHA-1, 6 digit, 30 detik, toleransi ±1 langkah) untuk akun
  pengguna; rahasia TOTP dienkripsi envelope (ADR-0011); 10 kode pemulihan sekali pakai (hash disimpan). Owner dapat
  mewajibkan MFA untuk anggota tenant dengan role admin (Owner/Manager); platform Super Admin wajib MFA.

## 2026-11-13 — Klarifikasi implementasi MFA

- "Role admin" untuk AUTH-012 diartikan sebagai role sistem Owner, Manager, dan Admin (role sistem `Admin` ada sejak
  migrasi 0003). Role kustom tidak terkena kebijakan ini. Entri Gate F tidak diubah.

## 2026-11-13 — Klarifikasi implementasi P6-3 (masking percakapan)

- AC-CNV-003 ("Viewer role → field sensitif dimasking") diartikan: role sistem Viewer mendapat `conversations.read`
  (migrasi 0028) dan selalu melihat versi termasking. Tampilan tanpa masking hanya untuk pembaca yang punya
  `conversations.reply` DAN `customers.read` (preset: Owner, Manager, Support Agent).
- Field sensitif: nama/email/telepon pelanggan, email staf, alamat email dan deret ≥ 8 digit di isi pesan & ringkasan
  approval, ringkasan argumen tool. Statistik AI (provider, model, token, latensi, status tool) tidak dianggap sensitif.
- Dapat diubah pemilik produk; perubahan = entri baru.

## 2026-11-13 — Definisi metrik analitik percakapan (P6-6)

- PRD §4.12 menyebut metrik tanpa definisi; implementasi memakai:
  - AI-resolved / human-resolved: percakapan ditutup tanpa / dengan pernah ditangani staf.
  - Waktu respons: pesan pelanggan pertama → balasan AI/staf pertama; waktu resolusi: dibuat → ditutup (jam dinding).
  - Handover rate: percakapan yang dialihkan AI → staf (sekali per percakapan) / percakapan baru. Mode Human Only
    (sejak awal menunggu staf) bukan handover.
  - Fallback rate: giliran AI yang berakhir dengan pesan fallback tenant (keputusan handover) / giliran AI chat.
  - Provider error rate: panggilan provider error/timeout / panggilan provider (tanpa giliran yang tidak memanggil
    provider: guardrail, kuota, circuit breaker, Human Only).
  - Token: prompt + completion giliran chat (uji coba admin tidak dihitung). Hari = tanggal Asia/Jakarta.
- Dapat diubah pemilik produk; perubahan = entri baru.

## 2026-11-13 — Gate G: D-06, D-10, D-12 legal billing

- Pemutus: Product owner ("Cukup subdomain, Kita paketnya gratis dan pro, boleh"; lalu "Gratis tidak include token AI,
  biaya pendaftaran perbulan 50k dan 100k hilangkan yang gratis deh"; uji coba 14 hari; paket 50k boleh BYOK;
  nama "Starter & Pro"; penagihan "Manual dulu").
- D-06: **DISETUJUI** — subdomain platform saja; custom domain (P7-4) di luar V1.0 (ADR-0008).
- D-10: **DISETUJUI** — Starter Rp50.000/bulan, Pro Rp100.000/bulan, tanpa paket gratis, uji coba 14 hari dengan batas
  Starter, pembayaran manual diverifikasi Super Admin (ADR-0016). Angka batas = usulan engineering, disimpan sebagai data.
- D-12 legal billing: **DISETUJUI bersyarat** — teks placeholder; wajib diganti teks final hasil review legal sebelum go-live.
