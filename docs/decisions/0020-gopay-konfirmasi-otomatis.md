---
status: Accepted
date: 2026-11-15
decision-makers: Product owner (pemilik repo)
---

# ADR-0020 — Konfirmasi pembayaran QRIS otomatis via GoPay Merchant (GoBiz)

## Context and Problem Statement

Alur pembayaran saat ini (ADR-0004) memakai QRIS statis: pembeli memindai gambar QRIS toko, menandai "sudah
bayar", lalu staf mengonfirmasi manual. Pemilik ingin konfirmasi **otomatis** ("otomatis cek konfirmasi dari
TF") tanpa biaya bagi UMKM.

Opsi gateway resmi (Midtrans) tidak dapat diakses pemilik (pendaftaran terganjal nomor yang sudah tertaut di
ekosistem GoTo) dan membebankan MDR. Riset menunjukkan: untuk **usaha mikro**, MDR QRIS 0% berlaku s.d.
Rp500.000 (BI, berlaku 1 Des 2024). Artinya QRIS statis toko sendiri gratis — yang kurang hanyalah deteksi
uang masuk.

Sumber deteksi uang masuk hanya dipegang bank/acquirer. Tanpa gateway, satu-satunya jalur yang berjalan adalah
API internal GoBiz/GoPay Merchant (`api.gobiz.co.id` + `api.gojekapi.com/merchant-analytics`), yang dipakai
proyek open-source `ahmadzakiyox/gopay-api-gateaway` (di-fork, file ter-obfuscate sudah di-deobfuscate &
diaudit: hanya menghubungi dua host itu).

## Considered Options

* **A. Gateway resmi (Midtrans/Xendit)** — legal, pasti, tapi berbiaya (MDR) dan terganjal pendaftaran.
* **B. Platform menampung dana lalu meneruskan** — butuh izin BI (UU Transfer Dana); ditolak.
* **C. QRIS statis + kode unik + baca notifikasi HP pemilik** — gratis, tapi bergantung HP pemilik menyala.
* **D. QRIS statis + kode unik + polling API GoBiz (tidak resmi)** — gratis, tanpa HP pemilik, tapi API privat
  (tunduk ToS GoBiz, dapat berubah/diblokir).

## Decision Outcome

Chosen: **D** (pemilik produk, 2026-11-15), dengan QRIS statis manual (ADR-0004) tetap sebagai **cadangan**
dan gateway resmi sebagai opsi masa depan. Integrasi dibuat netral agar penyedia lain bisa menyusul.

Risiko diterima pemilik secara sadar: ini API privat milik aplikasi pihak ketiga; dapat berubah/diblokir dan
tunduk pada ketentuan layanan GoBiz. Dipakai hanya atas kehendak pemilik toko (per-toko opt-in).

### Mekanisme

1. **Kode unik per pesanan.** Nominal tagihan + kode 1..999 rupiah (mis. Rp30.000 → Rp30.217), ditambahkan ke
   tagihan pembeli (bukan memotong harga toko). Dijamin tak ada dua charge *pending* bernominal sama per toko.
2. **QRIS dinamis.** QRIS statis toko (teks EMVCo) disintesis jadi dinamis bernominal terkunci — parser TLV
   benar + CRC-16 (`umkm_domain::commerce::qris`), bukan cari-ganti string seperti gateway asal. Dirender jadi
   SVG (`crate qrcode`) di halaman bayar.
3. **Sesi GoBiz per toko.** Login OTP sekali; access/refresh token **disegel envelope** (`UMKM_DATA_KEK`,
   pola `ai_provider_secrets`), auto-refresh. Token tidak pernah masuk log.
4. **Poller worker (~15 dtk).** Untuk tiap toko dengan charge pending: ambil transaksi merchant-analytics,
   cocokkan nominal unik dalam jendela waktu, dedup `transaction_id` (indeks unik DB, anti double-claim).
5. **Auto-konfirmasi.** Transaksi cocok → `payment_repo::confirm_auto` (tanpa aktor, audit sistem
   `payment.confirmed_auto` sumber `gopay_auto`) → pesanan langsung **Dibayar → Diproses**. Konfirmasi manual
   tetap tersedia bila deteksi gagal.

### Keamanan & batasan

* Semua panggilan keluar lewat `Egress` (kontrol SSRF); hanya `api.gobiz.co.id` + `api.gojekapi.com`.
* `confirm_auto` melonggarkan `ck_payments_decided` (ikat ke `decided_at`, bukan `decided_by`) agar keputusan
  sistem sah tanpa pengguna.
* Platform tidak pernah memegang dana; uang langsung ke rekening/akun toko.
* Fork gateway di-audit ulang setiap sinkronisasi upstream; file ter-obfuscate diganti versi terbaca.

## Consequences

* UMKM mikro mendapat konfirmasi otomatis **gratis** tanpa gateway.
* Ketergantungan pada API tidak resmi: bila GoBiz berubah/memblokir, fitur mati — tapi hanya fitur ini; alur
  manual tetap jalan, dan hanya toko yang opt-in terdampak.
* Komponen: `umkm_domain::commerce::{qris, gopay}`, `umkm_infrastructure::{gopay, gopay_repo}`, migrasi
  0036/0037, endpoint `/tenants/{id}/gopay/*` (pemilik) & `/public/orders/current/gopay-qris` (pembeli).
