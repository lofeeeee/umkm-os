---
status: Accepted
date: 2026-11-13
decision-makers: Product owner (pemilik repo)
---

# ADR-0016 — Langganan Starter/Pro, uji coba 14 hari, pembayaran manual

## Context and Problem Statement

D-10 (`PRD.md §4.15`, `PRD.md §12`, `PRD.md §20 #10`): paket, batas, siklus, dan cara penagihan langganan toko.
Keputusan pemilik produk (2026-11-13): paket "gratis dan pro" → kemudian "Gratis tidak include token AI, biaya
pendaftaran perbulan 50k dan 100k, hilangkan yang gratis"; toko baru uji coba 14 hari; paket 50k boleh BYOK; nama
"Starter & Pro"; penagihan "Manual dulu".

## Considered Options

* Pembayaran manual (QRIS/transfer milik platform, diverifikasi Super Admin) — pola sama dengan ADR-0004
* Payment gateway dengan webhook

## Decision Outcome

Chosen option: "Pembayaran manual".

| Aturan | Nilai awal (data `plans`, dapat diubah tanpa rebuild) |
|---|---|
| Paket | **Starter** Rp50.000/bulan · **Pro** Rp100.000/bulan; tidak ada paket gratis |
| Uji coba | toko baru: 14 hari dengan batas Starter; habis → website publik & checkout nonaktif, dashboard tetap bisa dibuka untuk membayar |
| Token AI platform / bulan | Starter 0 · Pro 1.000.000 (BYOK boleh di kedua paket, ADR-0011) |
| Anggota tim | Starter 2 · Pro 10 |
| Produk | Starter 30 · Pro 1.000 |
| Penyimpanan file | Starter 50 MB · Pro 1 GB |
| Percakapan / bulan | Starter 100 · Pro tak terbatas (fair-use) |
| Custom domain | tidak (ADR-0008) |
| Siklus | bulanan; diperpanjang per periode setelah pembayaran diverifikasi |
| Pembayaran | toko mengunggah bukti bayar ke QRIS/rekening platform; Super Admin memverifikasi → periode aktif; tidak ada gateway/webhook |

Angka batas selain harga adalah **usulan engineering** yang diterima sebagai awal; perubahan = ubah data plan + entri
product-decision-log, bukan kode.

### Consequences

* Good: tidak ada integrasi pihak ketiga; alur verifikasi manual sudah teruji di pesanan toko (ADR-0004).
* Bad: aktivasi Pro bergantung pada respons admin platform; perpanjangan tidak otomatis.

## More Information

Teks legal billing (D-12) memakai placeholder yang wajib diganti sebelum go-live. Gateway pembayaran = ADR baru.
Append-only.
