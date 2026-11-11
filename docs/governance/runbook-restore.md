# Runbook — Restore PostgreSQL dari backup (P0-10, `PRD.md §6.10`)

**Status:** skrip dan uji lokal tersedia. RPO/RTO **belum diklaim**; divalidasi pada restore drill P8-4.

## Yang ada

| Komponen | Lokasi |
|---|---|
| Backup harian (pg_dump -Fc → validasi `pg_restore --list` → enkripsi age → rotasi 7 daily + 4 weekly) | `ops/backup/backup.sh` |
| Penjadwal harian non-root (`BACKUP_AT_UTC`, default 02:00) | `ops/backup/backup-loop.sh`, service Compose `backup` (profile `backup`) |
| Restore ke database kosong (checksum, dekripsi, `--single-transaction`) | `ops/backup/restore.sh` |
| Uji otomatis + negative control | `scripts/test-backup-restore.sh` |

## Belum ada (jujur)

| Gap | Akibat | Menunggu |
|---|---|---|
| Salinan di luar VPS (`BACKUP_OFFSITE_CMD`) | Backup hanya di VPS; `backup.sh` sengaja **exit 3** setiap run sampai diisi | D-07 (storage) |
| WAL archiving kontinu | RPO aktual ≈ 24 jam (dump harian), **bukan** target 15 menit | D-07 + task baru (mis. WAL-G/pgBackRest) |
| Drill bulanan terjadwal + pengukuran RTO | RTO 4 jam belum terbukti | P8-4 |
| Alert bila backup gagal | Kegagalan hanya terlihat di log JSON | P8-5 |

## Kunci enkripsi

- Server hanya memegang **public key** age (`BACKUP_AGE_RECIPIENT=age1...`). Server yang diretas tidak dapat membaca backup lama.
- Private key (`age-keygen -o id.txt`) disimpan **di luar server** (password manager/HSM tim ops), minimal 2 pemegang.
- Kehilangan private key = semua backup tidak dapat dibaca. Uji dekripsi saat setiap drill.

## Prosedur restore

1. **Hentikan penulisan.** `docker compose stop api worker` (cegah data baru selama pemulihan).
2. **Ambil arsip** `*.dump.age` + `*.sha256` dari salinan off-site (atau volume `backup_data`). Pilih timestamp terbaru sebelum insiden.
3. **Siapkan database kosong** (jangan menimpa DB rusak; simpan untuk forensik):
   ```bash
   docker compose exec postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "CREATE DATABASE umkm_restore"'
   ```
4. **Restore** (private key di-mount read-only dari mesin operator, dihapus setelahnya):
   ```bash
   docker run --rm --network umkm-os_backend \
     -v /path/backup:/b:ro -v /path/key:/k:ro -e TMPDIR=/tmp \
     -e PGHOST=postgres -e PGUSER=... -e PGPASSWORD=... -e PGDATABASE=umkm_restore \
     umkm-os/backup:local restore.sh /b/<file>.dump.age /k/id.txt
   ```
   Skrip menolak target yang tidak kosong (exit 4), checksum salah (exit 1), atau key salah (exit 1).
5. **Verifikasi**: `_sqlx_migrations` sesuai versi aplikasi; hitung baris tabel kritis; jalankan `sqlx migrate run` (harus no-op atau hanya migration yang lebih baru).
6. **Alihkan** `POSTGRES_DB`/URL aplikasi ke database hasil restore, `docker compose up -d api worker`, cek `/readyz`.
7. **Catat** waktu mulai/selesai (RTO), timestamp backup yang dipakai (RPO aktual), dan hasil verifikasi di `docs/evidence/`.

## Uji lokal

```bash
scripts/test-backup-restore.sh   # 17 kasus: restore identik, negative control, rotasi terlama-dulu
```
