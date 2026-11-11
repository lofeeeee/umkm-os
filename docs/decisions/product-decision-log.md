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
