# UMKM OS — Penghubung GoPay Merchant (ekstensi browser)

Menghubungkan akun GoPay/GoBiz Merchant ke UMKM OS **tanpa menyalin token manual**.
Pemilik login GoBiz di browsernya sendiri (nomor + OTP + reCAPTCHA seperti biasa), lalu
ekstensi membaca cookie sesi (httpOnly, di `portal.gofoodmerchant.co.id`) dan mengirimnya
ke dashboard UMKM OS yang Anda tentukan.

Kenapa ekstensi: token GoBiz adalah cookie **httpOnly** — tidak bisa dibaca JavaScript biasa/
bookmarklet. Hanya ekstensi (`chrome.cookies`) yang boleh membacanya. Tidak ada captcha yang
dibypass: captcha tetap diselesaikan oleh pemilik di browser.

## Alur pemakaian (2 langkah setelah pasang)
1. Klik ikon ekstensi → **"Buka halaman login GoBiz"** → login. Status berubah hijau otomatis.
2. Isi **alamat dashboard UMKM OS** (sekali, diingat) → **"Hubungkan sekarang"**. Dashboard
   terbuka & tersambung otomatis.

## Pasang untuk pengujian (sebelum terbit)
`chrome://extensions` → aktifkan **Developer mode** → **Load unpacked** → pilih folder ini.

## Checklist rilis Chrome Web Store (agar pemasangan 1 klik untuk semua toko)
- [x] Ikon 16/48/128 (`icons/`)
- [x] `description` ≤ 132 karakter
- [x] Izin minimal: `cookies`, `storage`, host `portal.gofoodmerchant.co.id`
- [x] Kebijakan privasi (`PRIVACY.md`) — unggah ke halaman publik & isi URL-nya di form store
- [ ] **Deklarasi trader/non-trader (EEA):** pilih **trader** (ekstensi untuk produk komersial UMKM OS)
- [ ] Email kontak di `PRIVACY.md` diganti dengan email Anda
- [ ] Minimal 1 screenshot 1280×800 (listing) + ringkasan
- [ ] Zip folder ini (tanpa `README.md`/`PRIVACY.md` opsional) lalu unggah; bayar biaya developer $5 sekali
- [ ] Mode distribusi: **Unlisted** cukup (hanya lewat tautan; tak muncul di pencarian)

## Catatan multi-domain
Alamat dashboard diisi pemilik di ekstensi (diingat per perangkat). Bila nanti UMKM OS punya
domain tetap, alamat ini bisa dipandu dari dashboard. Token hanya dilewatkan via fragment URL
(tidak terkirim ke server lain) lalu dashboard mengirimnya ke UMKM OS via HTTPS.
