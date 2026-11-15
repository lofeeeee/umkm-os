# Desain: Decode QRIS Otomatis dari Gambar Unggahan

Status: Disetujui (desain) — 2026-11-15
Terkait: ADR-0004, ADR-0020, P10 GoPay Merchant

## Masalah

Konfirmasi pembayaran otomatis GoPay membutuhkan payload EMVCo QRIS statis toko untuk
menyusun QR dinamis bernominal unik. Saat ini pemilik harus menempel teks QRIS secara manual
di kartu GoPay, padahal gambar QRIS sudah diunggah untuk checkout manual. Dua langkah ini
membingungkan dan rawan salah tempel.

## Tujuan

- Menghapus input teks QRIS manual dari UI.
- Mengambil payload QRIS langsung dari gambar QRIS yang sudah diunggah.
- Tetap mendukung gambar lama tanpa meminta unggah ulang.
- Menjaga checkout manual tetap berjalan bila gambar tidak terbaca.
- Menjaga sumber kebenaran payload di server, bukan klien.

## Non-tujuan

- Mengubah alur checkout manual atau verifikasi pembayaran.
- Mengambil QRIS langsung dari API GoBiz.
- Decode QRIS di browser.
- Mengubah tarif, refund, atau mekanisme kode unik.

## Keputusan

Pendekatan yang dipilih: **decode di server saat pengaturan disimpan, dengan backfill
saat pengaturan/status GoPay dibaca**. Payload disimpan di `commerce_settings`, bukan di
sesi GoPay, karena QRIS adalah konfigurasi toko dan harus tetap ada meskipun sesi GoPay
terputus.

## Perubahan Data

Migrasi baru `migrations/0039_qris_payload.up.sql`:

```sql
ALTER TABLE commerce_settings
    ADD COLUMN qris_static_payload text;

ALTER TABLE commerce_settings
    ADD CONSTRAINT ck_commerce_settings_qris_payload
    CHECK (qris_static_payload IS NULL OR octet_length(qris_static_payload) BETWEEN 20 AND 1024);

UPDATE commerce_settings cs
SET qris_static_payload = gs.static_qris
FROM gopay_sessions gs
WHERE gs.tenant_id = cs.tenant_id
  AND gs.static_qris IS NOT NULL
  AND cs.qris_static_payload IS NULL;
```

Migrasi turun menghapus kolom. `gopay_sessions.static_qris` dibiarkan sebagai legacy;
tidak dibaca untuk keputusan baru dan tidak ditulis lagi.

`commerce_repo::Settings` mendapat field:

```rust
pub qris_static_payload: Option<String>,
```

`settings_in` membaca kolom; `put_settings` menulis kolom. Audit
`commerce.settings_changed` menambahkan `qris_static_ready: bool`, tanpa menulis payload
ke audit/log.

## Backend

### Decoder

Modul baru `crates/infrastructure/src/qris_decode.rs`:

- Input: byte gambar (PNG/JPEG/WebP) dari `stored_objects`.
- Decode dengan `rqrr` tanpa fitur `img`; buffer grayscale dibuat dari crate `image`
  yang sudah dipakai.
- Coba gambar asli; bila tidak terdeteksi dan dimensi besar, coba satu versi downscale.
- Ambil hasil decode pertama yang lolos `umkm_domain::commerce::qris::verify`.
- Error dikembalikan sebagai tipe `QrisDecodeError` (bukan panic).

Batas: gambar dengan dimensi ekstrem ditolak/downscale agar tidak memakan CPU/memori.
Decode hanya dipanggil untuk pemilik toko (`tenant.update`) atau saat membaca pengaturan
miliknya.

### Simpan pengaturan

`apply_settings` (jalur langsung maupun setelah approval):

1. Baca pengaturan lama.
2. Tentukan payload:
   - QRIS kosong → `None`.
   - Gambar sama dengan lama dan payload lama ada → pertahankan payload lama.
   - Gambar baru/berubah atau payload kosong → muat aset tenant, decode, validasi.
   - Decode gagal → `None`; simpan tetap berhasil.
3. Tulis pengaturan beserta payload.
4. Kembalikan `SettingsView` dengan `qris_static_ready = payload.is_some()`.

Jika gambar berubah dan decode gagal, payload lama **tidak** dipertahankan agar QRIS
merchant lama tidak dipakai untuk pembayaran baru.

### Backfill gambar lama

- `GET /api/v1/tenants/{tenant_id}/commerce/settings`: bila `qris_object_id` ada dan
  `qris_static_payload` kosong, decode lalu simpan secara kondisional
  (`WHERE qris_object_id = $id`).
- `GET /api/v1/tenants/{tenant_id}/gopay`: lakukan backfill yang sama sebelum menghitung
  `has_static_qris`.
- `POST /api/v1/public/orders/current/gopay-qris`: hanya membaca payload tersimpan; tidak
  melakukan decode agar endpoint publik tidak mahal.

### GoPay

- `current_gopay_qris` memakai `commerce_settings.qris_static_payload`.
- `has_static_qris` berasal dari payload tersebut.
- Sesi GoPay tetap wajib aktif.
- `gopay_sessions.static_qris` tidak lagi menjadi fallback; migrasi sudah menyalin nilai
  lama ke `commerce_settings`.
- Hapus `PUT /api/v1/tenants/{tenant_id}/gopay/static-qris`, `StaticQrisDto`, dan
  handler `put_static_qris`. Regenerasi OpenAPI.

### API response

`SettingsView` (response GET/PUT) menambahkan:

```json
{ "qris_static_ready": true }
```

Field ini hanya untuk response; tidak diterima sebagai input.

## Frontend

File: `frontend/src/app/(app)/shop-settings/page.tsx`.

Kartu **Pembayaran QRIS**:

- Tetap memakai `ImagePicker` untuk unggah gambar.
- Bila `qris` ada dan `qris_static_ready=false`:
  peringatan “QRIS belum bisa dibaca otomatis. Checkout manual tetap bisa; unggah gambar
  QRIS yang lebih jelas untuk mengaktifkan konfirmasi otomatis.”
- Bila `qris_static_ready=true`: teks kecil “QRIS terbaca — konfirmasi otomatis siap.”

Kartu **Konfirmasi otomatis (GoPay Merchant)**:

- Hapus state `qris`, `saveQris`, textarea “QRIS statis toko (teks)”, dan tombol simpan QRIS.
- Bila terhubung dan `has_static_qris=false`: peringatan “QRIS belum terbaca dari gambar.
  Unggah ulang gambar QRIS di bagian Pembayaran QRIS.”
- Setelah pengaturan QRIS disimpan, kartu GoPay memuat ulang status agar
  `has_static_qris` langsung sinkron.

## Error Handling

| Kondisi | Perilaku |
|---|---|
| QRIS valid | Simpan, payload tersimpan, auto-confirm siap |
| QR bukan QRIS / CRC salah | Simpan gambar, payload kosong, warning, manual tetap jalan |
| Gambar rusak/tidak bisa dibaca | Simpan gambar, payload kosong, warning, manual tetap jalan |
| Gambar diganti, decode gagal | Payload lama dikosongkan |
| Gambar sama, decode gagal sementara | Payload lama dipertahankan |
| Tanpa gambar QRIS | `qris_static_ready=false` |
| Aset bukan milik tenant | Tetap ditolak validasi `asset:<uuid>` yang ada |

Tidak ada error 500 karena decode gagal; kegagalan decode adalah kondisi bisnis yang
dikembalikan sebagai `qris_static_ready=false`.

## Keamanan dan Privasi

- Payload QRIS bersifat semi-publik (tercetak di QR toko), tetapi tidak dikembalikan ke
  response pengaturan; hanya status boolean.
- Validasi format dan CRC mencegah teks sembarang disimpan sebagai QRIS.
- Hanya `tenant.update` yang dapat mengganti gambar; perubahan aset tetap diaudit.
- Tidak ada panggilan jaringan baru; decode berjalan lokal.
- Decode dibatasi untuk mencegah penyalahgunaan CPU/memori.

## Pengujian

Unit (`crates/infrastructure`):

- Decode QR PNG valid yang dibuat dari payload QRIS contoh.
- QR valid tetapi bukan QRIS → error validasi.
- Gambar kosong/rusak → error tanpa panic.
- Payload dengan CRC salah → error.

Integrasi API:

- Simpan QRIS valid → response `qris_static_ready=true`.
- Simpan PNG biasa → 200, `qris_static_ready=false`, checkout manual tetap bisa.
- Ganti gambar valid dengan gambar tidak terbaca → payload lama tidak dipakai.
- Simpan ulang dengan gambar sama → payload dipertahankan.
- Gambar lama tanpa payload → `GET settings` mengisi payload (backfill).
- `GET gopay` melaporkan `has_static_qris` sesuai payload commerce.
- Endpoint `PUT /gopay/static-qris` tidak ada lagi di OpenAPI.

Frontend:

- `npm run lint`, `npm run typecheck`, `npm run build`.
- Playwright: textarea teks QRIS tidak ada; unggah QRIS valid → status siap; unggah gambar
  tidak terbaca → warning dan checkout manual tetap aktif.

Regenerasi OpenAPI:

```bash
UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi
```

## Rollout dan Kompatibilitas

- Migrasi menyalin teks QRIS lama ke `commerce_settings`, jadi toko yang sudah memakai
  konfirmasi otomatis tidak kehilangan konfigurasi.
- Toko dengan gambar QRIS lama mendapat payload saat pemilik membuka Pengaturan Toko atau
  kartu GoPay; tidak perlu unggah ulang.
- Bila decode gambar lama gagal, auto-confirm nonaktif sampai gambar diganti; checkout
  manual tidak terdampak.
- Deploy API dan frontend bersamaan karena response `SettingsView` berubah dan endpoint
  manual dihapus.

## Risiko

- Decoder bisa gagal pada QRIS berlogo berat, blur, atau screenshot terpotong. Mitigasi:
  coba downscale, beri warning, sediakan jalur manual.
- Decode menambah CPU saat simpan/backfill. Mitigasi: hanya pemilik, batas dimensi,
  tidak dipanggil di endpoint publik.
- Payload yang tersimpan bisa berasal dari QRIS merchant lain. Mitigasi: hanya pemilik
  yang dapat mengganti gambar dan perubahan tetap diaudit; validasi format tidak dapat
  membuktikan kepemilikan merchant.

## Kriteria Penerimaan

1. Tidak ada input teks QRIS di UI Pengaturan Toko.
2. Menyimpan gambar QRIS valid menghasilkan `qris_static_ready=true`.
3. Gambar QRIS lama didecode tanpa unggah ulang saat pengaturan/status dibaca.
4. Gambar tidak terbaca tetap tersimpan, menampilkan warning, dan checkout manual berjalan.
5. Mengganti gambar dengan yang tidak terbaca mengosongkan payload lama.
6. `PUT /gopay/static-qris` dan dokumentasinya hilang; OpenAPI cocok dengan kode.
7. Unit/integrasi/API/frontend checks lulus.

## File yang Disentuh

- `migrations/0039_qris_payload.up.sql`, `.down.sql`
- `Cargo.toml`, `crates/infrastructure/Cargo.toml`
- `crates/infrastructure/src/qris_decode.rs`
- `crates/infrastructure/src/commerce_repo.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/api/src/commerce.rs`
- `apps/api/src/gopay.rs`
- `apps/api/src/openapi.rs`
- `docs/api/openapi.yaml`
- `frontend/src/app/(app)/shop-settings/page.tsx`
- `apps/api/tests/checkout_http.rs`, `apps/api/tests/common/mod.rs`
- `crates/infrastructure/tests/gopay_repo.rs`
- `docs/architecture/dependency-review.md`
- `docs/decisions/0021-qris-decode-dari-gambar.md`
