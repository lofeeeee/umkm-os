# Kebijakan Privasi — UMKM OS Penghubung GoPay Merchant

Ekstensi ini membantu pemilik toko menghubungkan akun GoPay/GoBiz Merchant mereka ke dashboard
UMKM OS mereka sendiri, tanpa menyalin token secara manual.

## Data yang diakses
- **Cookie sesi GoBiz** (`portal.gofoodmerchant.co.id`), khususnya `access_token` dan `refresh_token`.
  Dibaca hanya saat Anda menekan **"Hubungkan sekarang"**.

## Penggunaan & pengiriman data
- Cookie sesi **hanya** dikirim ke alamat dashboard UMKM OS yang **Anda tentukan sendiri** di ekstensi,
  lewat koneksi HTTPS. Token dilewatkan melalui fragment URL (tidak dikirim ke server lain).
- Ekstensi **tidak** mengirim data ke server pengembang ekstensi, tidak menjual data, dan tidak
  melakukan pelacakan. Satu-satunya penyimpanan lokal adalah alamat dashboard yang Anda isi.

## Izin
- `cookies` + akses host `portal.gofoodmerchant.co.id`: untuk membaca cookie sesi Anda sendiri.
- `storage`: menyimpan alamat dashboard agar tidak perlu diketik ulang.

## Kontak
Diterbitkan oleh pengelola UMKM OS. Pertanyaan privasi: ganti dengan email kontak Anda sebelum rilis.
