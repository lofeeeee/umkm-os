# UMKM OS — Penghubung GoPay Merchant (ekstensi browser)

Menghubungkan akun GoPay/GoBiz Merchant ke UMKM OS **tanpa menyalin token manual**.
Pemilik login GoBiz di browsernya sendiri (nomor + OTP + reCAPTCHA seperti biasa), lalu
ekstensi membaca cookie sesi (httpOnly) dan mengirimkannya ke dashboard UMKM OS.

Kenapa ekstensi: token GoBiz disimpan sebagai cookie **httpOnly**, jadi tidak bisa dibaca
JavaScript/bookmarklet biasa — hanya ekstensi (`chrome.cookies`) yang boleh membacanya.

## Pasang (sementara, sebelum terbit di Web Store)
1. Buka `chrome://extensions` (atau `edge://extensions`).
2. Aktifkan **Developer mode**.
3. **Load unpacked** → pilih folder `tools/gobiz-connector`.

## Pakai
1. Klik ikon ekstensi → isi **URL dashboard UMKM OS** (sekali saja).
2. Tombol **"Buka & login GoBiz"** → login di tab GoBiz (nomor, OTP, reCAPTCHA).
3. Kembali ke ekstensi → **"Hubungkan ke UMKM OS"**. Dashboard akan terisi & terhubung otomatis.

Catatan: token hanya dilewatkan lewat fragment URL (tidak terkirim ke server lain), lalu
halaman dashboard mengirimnya ke UMKM OS via HTTPS. Untuk pemakaian luas, terbitkan ekstensi
ini ke Chrome Web Store agar pemasangannya satu klik.
