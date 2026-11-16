---
status: Accepted
date: 2026-11-15
decision-makers: Product owner (pemilik repo)
---

# ADR-0021 — Decode payload QRIS dari gambar yang diunggah

## Context and Problem Statement

ADR-0004 memakai gambar QRIS untuk pembayaran manual. ADR-0020 menambah konfirmasi
otomatis GoPay tetapi meminta pemilik menempel **teks** payload QRIS statis secara manual;
ini membingungkan dan terasa seperti keharusan memasang ekstensi lagi. Padahal gambar QRIS
sudah diunggah dan berisi payload yang sama.

## Considered Options

* Mempertahankan tempel teks manual.
* Decode di browser saat unggah.
* Decode di server dari `stored_objects.bytes` saat settings disimpan + lazy backfill.

## Decision Outcome

Chosen option: decode di server dari gambar yang sudah diunggah, karena sumber datanya sama,
tidak menambah kerja pengguna, tidak mengekspos payload ke bundle publik, dan dapat dipakai
ulang untuk instalasi lama. Kegagalan decode tidak menggagalkan simpan pengaturan; konfirmasi
manual tetap tersedia.

### Consequences

* Good, because pemilik tidak lagi menyalin payload QRIS atau memasang ekstensi hanya untuk QRIS.
* Good, because `commerce_settings` menjadi sumber kebenaran payload dan status GoPay sinkron.
* Bad, because decode bergantung pada kualitas gambar; gambar buruk menonaktifkan auto-confirm.
* Bad, because `rqrr` menambah dependensi CPU-bound yang harus dijalankan lewat `spawn_blocking`.

## More Information

ADR ini tidak menimpa ADR-0020; ia mengganti cara memperoleh payload statis. Mekanisme kode
unik, QRIS dinamis, polling GoBiz, dan fallback manual tetap berlaku. `gopay_sessions.static_qris`
dipertahankan sebagai kolom legacy untuk kompatibilitas rollback.
