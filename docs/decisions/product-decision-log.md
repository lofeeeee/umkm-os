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
