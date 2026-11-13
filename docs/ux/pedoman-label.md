# Pedoman label & pola UI dasbor

Sumber kode: `frontend/src/lib/labels.ts` (status, format), `frontend/src/ui/*` (komponen), `frontend/src/app/globals.css` (token).

## Bahasa

| Aturan | Benar | Salah |
| --- | --- | --- |
| Huruf kapital hanya di awal kalimat | Tambah produk | Tambah Produk |
| Tombol = kata kerja (+ objek bila perlu) | Simpan, Batal, Tambah produk, Arsipkan | OK, Submit, Produk baru |
| Istilah Indonesia baku | Draf, Persetujuan, Pengembalian dana, Peran | Draft, Approval, Refund, Role |
| Istilah yang tetap dipakai apa adanya | QRIS, SKU, MFA, AI, URL | — |
| Pesan sukses berupa kalimat lengkap bertitik | Produk tersimpan. | sukses! |
| Pesan error menjelaskan apa yang harus dilakukan | Periksa 2 isian yang ditandai. | Error 422 |
| Waktu relatif untuk kejadian baru, tanggal untuk lainnya | 5 menit lalu · 13 Nov 2026 | 2026-11-13T07:12Z |
| Uang selalu `rupiah()` | Rp41.000 | 41000.00 |

Pengecualian lama yang dipertahankan karena dikunci e2e: "Simpan draft" (editor website),
"Draft tersimpan.", "Role kustom baru", "Simpan role", "Nama role". Ganti bersamaan dengan spesifikasinya.

## Pola halaman

1. `PageHeader`: judul (h1) = nama menu di sidebar, satu kalimat penjelas, aksi utama di kanan (maks. satu tombol primer).
2. Isi dalam `Card`; daftar memakai tabel `d-table` atau `d-list`; kosong memakai `EmptyState` dengan aksi berikutnya.
3. Membuat/mengubah entitas panjang (produk, pesanan) di `Sheet` panel samping: daftar tetap terlihat.
4. Aksi berisiko (hapus, arsipkan, tolak, matikan) wajib `confirm()`; tombolnya bergaya `d-btn-danger`.
5. Umpan balik sukses = `toast.ok()`; error validasi = inline di dekat isian, error umum = `Alert` di atas form.
6. Muat pertama = `PageSkeleton`/`Skeleton`, bukan teks "Memuat…".
7. Status selalu `Badge` dengan tone dari `labels.ts` (ok = selesai/aktif, warn = butuh tindakan, info = sedang berjalan, danger = gagal/ditolak, neutral = pasif).

## Motion

- Hanya `transform`/`opacity`; ≤ 250 ms; easing `--ease-out` untuk masuk, `--ease-drawer` untuk panel.
- Transisi halaman 240 ms fade + geser 4 px; popover 150 ms dari titik pemicu; dialog skala 0,96 → 1.
- Command palette (⌘K) tanpa animasi masuk karena sering dipakai.
- `prefers-reduced-motion` mematikan semua gerak.
