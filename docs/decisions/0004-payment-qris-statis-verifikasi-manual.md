---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0004 — Payment MVP: QRIS statis GoPay milik tenant + verifikasi manual

## Context and Problem Statement

D-02 (`PRD.md §20 #2`, `PRD.md §4.7`, `PRD.md §11.1`): MVP memakai manual transfer/COD atau payment gateway. Pemilik produk memilih **"QRIS GoPay"** dan, saat ditanya pembacaannya, memilih **QRIS statis** (2026-11-12): setiap toko mengunggah gambar QRIS merchant GoPay miliknya sendiri, pembeli memindai dan membayar, pemilik toko mengonfirmasi pembayaran di dashboard.

## Considered Options

* Manual transfer bank + COD (rekomendasi awal `OPEN_DECISIONS.md`)
* QRIS dinamis lewat gateway (Midtrans/GoPay API, webhook bertanda tangan)
* QRIS statis milik tenant + verifikasi manual

## Decision Outcome

Chosen option: "QRIS statis milik tenant + verifikasi manual". Uang langsung masuk ke akun merchant tenant; platform **tidak pernah memegang dana, kredensial merchant, atau API key payment**. Tidak ada webhook pada cabang ini.

| Aturan | Nilai |
|---|---|
| Metode | satu metode V1: `qris_static`. Enum metode tetap terbuka untuk adapter lain (port `PaymentMethod`); gateway = ADR baru |
| Pengaturan tenant | gambar QRIS (objek `stored_objects`, PNG/JPEG/WebP, ADR-0009) + nama penerima tampilan + `enabled`. Checkout publik ditolak (`409 PAYMENT_NOT_CONFIGURED`) bila QRIS belum diatur |
| Siapa boleh mengganti QRIS | hanya permission `tenant.update` (Owner); setiap perubahan masuk audit log dengan before/after id objek — mitigasi staf mengganti QRIS ke rekening sendiri |
| Payment record | dibuat di transaksi checkout yang sama (`PRD.md §11.1`): `status=pending`, `amount = order.total` (server), `expires_at = reservation expiry` |
| Klaim pembeli | pembeli (dengan token order) menandai "sudah bayar" + catatan referensi opsional (≤ 120 karakter, mis. nama pembayar) + bukti gambar opsional (1 per payment, menggantikan yang lama); status `pending → claimed`. Klaim bukan bukti; status order tetap `AWAITING_PAYMENT` |
| Verifikasi | anggota dengan `payments.verify` mencocokkan dengan aplikasi GoPay Merchant lalu `confirm` (mencatat jumlah diterima) atau `reject` (alasan wajib). `confirm` → order `PAID`. Transisi memakai row lock + pemeriksaan status, jadi konfirmasi ganda diproses sekali (`BR-008` versi manual) |
| Jumlah berbeda | `confirm` dengan jumlah ≠ `amount` wajib catatan; tercatat di audit |
| Kedaluwarsa | payment `pending/claimed` yang lewat `expires_at` tidak otomatis dikonfirmasi; job expiry melepas reservasi dan order → `EXPIRED`. Payment `claimed` yang kedaluwarsa tetap dapat dikonfirmasi oleh verifier selama stok masih bisa dipotong ulang (diputuskan di P3-7, diuji) |
| Refund | dilakukan pemilik di luar platform; platform mencatat `refunds` (jumlah ≤ terkonfirmasi − refund sebelumnya, alasan wajib, actor) + audit; `payments.refund` |
| Data | tidak menyimpan nomor kartu/akun/PIN pembeli. Bukti gambar = data pelanggan, ikut retensi ADR-0012 |

Permission baru (memperluas preset 2026-11-11, dicatat di product decision log): `payments.verify` (Owner, Manager), `payments.refund` (Owner).

### Consequences

* Good: tanpa kontrak gateway, PCI, webhook publik, atau rahasia per tenant; bisa diuji penuh secara lokal.
* Good: dana langsung ke tenant; platform bukan penyelenggara jasa pembayaran.
* Bad: verifikasi manual memperlambat order dan rawan salah cocok (dua pembeli, jumlah sama). Order code ditampilkan agar pembeli mencantumkannya; kode unik nominal **tidak** dipakai di V1.
* Bad: platform tidak memvalidasi isi QR (tidak men-decode payload EMVCo QRIS); gambar ditampilkan apa adanya.
* AC-ORD-007 untuk cabang ini = otorisasi verifier + idempotensi konfirmasi + audit; signature webhook baru berlaku saat adapter gateway ditambahkan (ADR baru).

## More Information

Status di `OPEN_DECISIONS.md`: D-02 `DISETUJUI` 2026-11-12. Append-only; ganti/tambah metode = ADR baru yang menautkan ADR ini.
