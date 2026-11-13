# Runbook — Deploy & rollback (P8-5, `PRD.md §14.3`)

**Status:** prosedur disusun dari artefak yang ada (Compose, image, migration, backup). **Belum dilatih di VPS
produksi** (drill rollback menunggu VPS + DNS, lihat checklist V1.0 #25). Jangan anggap RTO sudah terbukti.

## 0. Prinsip

1. **Image dari GHCR, dibangun CI** (`.github/workflows/images.yml`, `ghcr.io/ryasrk/umkm-os/{api,worker,web,backup}`):
   - `:latest` — selalu commit `main` terbaru yang CI-nya hijau (dipindahkan hanya bila commit masih HEAD `main`).
     Untuk staging/uji cepat: `IMAGE_PULL_POLICY=always` + `docker compose up -d` selalu menarik yang terbaru.
   - `:sha-<7>` — tetap per commit; `:vX.Y.Z` — dari tag git `vX.Y.Z` (hanya bila CI commit itu hijau).
   **Produksi memakai tag tetap** (`vX.Y.Z` atau `sha-<7>`), bukan `latest`, agar versi berjalan tercatat dan
   rollback = kembali ke tag sebelumnya. Compose memilih lewat `API_IMAGE` / `WORKER_IMAGE` / `WEB_IMAGE` / `BACKUP_IMAGE`.
2. **Backup sebelum migrasi.** Service `migrate` (`umkm-api migrate`) berjalan otomatis sebelum `api`/`worker`
   (depends_on). Migrasi hanya maju di produksi.
3. **Migrasi expand/contract.** Migrasi rilis N harus tetap bisa dipakai binary rilis N-1 (tambah kolom/tabel dulu,
   hapus/ubah yang lama di rilis berikutnya). Dengan begitu rollback aplikasi **tidak** butuh rollback skema.
   File `*.down.sql` hanya diuji di DB kosong (CI: run → revert 0 → run) — **bukan** alat rollback produksi berisi data.
4. **Satu perubahan per rilis bila memungkinkan**; rilis berisi migrasi destruktif wajib disetujui dan dijadwalkan.

## 1. Prasyarat (sekali)

| Item | Cara | Status |
|---|---|---|
| VPS ±8 GB, Docker + Compose v2 | `PRD.md §9.3`, `docker-compose.yml` | Menunggu VPS |
| DNS `*.ryasai.my.id` + TLS wildcard (DNS-01) | ADR-0001, `ops/caddy/Caddyfile` | Menunggu provider DNS |
| `.env` produksi (dari `.env.example`, permission 600) | secret dari generator acak; `DATA_KEK` disimpan juga di luar VPS | — |
| Backup terenkripsi + salinan luar VPS | `ops/backup/`, `BACKUP_OFFSITE_CMD` | Menunggu D-07 |
| Monitoring (`--profile monitoring`) + alert Telegram | `ops/monitoring/alertmanager.yml` (salin, isi `chat_id`) → `ALERTMANAGER_CONFIG`; token bot → `ALERT_TELEGRAM_BOT_TOKEN_FILE` | Menunggu token bot & chat on-call |
| Config produksi: `[billing].payment_instructions`, `[ai.platform]` harga, teks legal final | `config/production.toml` | Placeholder |

## 2. Deploy rilis baru

```sh
# 0) Catat versi berjalan untuk rollback.
docker compose ps --format '{{.Service}} {{.Image}}' > /root/deploy/prev-$(date -u +%Y%m%dT%H%M%SZ).txt

# 1) Image dibangun CI (bukan di VPS: hemat RAM/disk). Rilis: `git tag vX.Y.Z && git push origin vX.Y.Z`
#    → workflow Images menerbitkan :vX.Y.Z. Tarik di VPS:
R=ghcr.io/ryasrk/umkm-os
export API_IMAGE=$R/api:vX.Y.Z WORKER_IMAGE=$R/worker:vX.Y.Z WEB_IMAGE=$R/web:vX.Y.Z BACKUP_IMAGE=$R/backup:vX.Y.Z
docker compose pull api worker caddy

# 2) Backup sekali sebelum migrasi (wajib bila rilis berisi migrasi).
docker compose --profile backup run --rm backup /usr/local/bin/backup.sh
#    -> harus exit 0 (exit 3 = salinan luar VPS belum dikonfigurasi: BERHENTI untuk rilis bermigrasi).

# 3) Terapkan. `migrate` berjalan dulu; api/worker hanya mulai bila migrasi sukses.
docker compose up -d

# 4) Verifikasi (maks 5 menit; gagal → langkah 4).
docker compose ps                              # api healthy, migrate exited 0
curl -fsS https://api.ryasai.my.id/readyz      # {"status":"ready"}
DOM=ryasai.my.id scripts/smoke-adr0014.sh /tmp/smoke   # (adaptasi https) SMOKE-OK
docker compose logs --since 5m api worker | grep -i '"level":"error"' || true
```

Kriteria sukses: `/readyz` siap, smoke OK, tidak ada lonjakan 5xx di Prometheus (`ApiHigh5xxRate` tidak firing),
antrean outbox turun (`worker_queue_depth`).

## 3. Rollback aplikasi (tanpa rollback skema)

Gunakan bila rilis baru gagal tetapi migrasinya bersifat expand (prinsip 3).

```sh
PREV=vX.Y.(Z-1)   # dari file prev-*.txt
R=ghcr.io/ryasrk/umkm-os
API_IMAGE=$R/api:$PREV WORKER_IMAGE=$R/worker:$PREV WEB_IMAGE=$R/web:$PREV docker compose up -d
curl -fsS https://api.ryasai.my.id/readyz
```

Catatan: service `migrate` binary lama memakai `deploy_migrator()` (`crates/database/src/lib.rs`) yang
**mengabaikan** migrasi lebih baru yang sudah diterapkan, sehingga langkah migrate tidak memblokir rollback; skema
tidak diturunkan (expand/contract). Diuji di `crates/database/tests/deploy_migrator.rs` (dengan negative control).
Checksum migrasi yang dikenal tetap diverifikasi — migrasi yang sudah diterapkan **tidak boleh diedit**, hanya ditambah.

## 4. Rollback data (insiden berat)

Bila migrasi/rilis merusak data dan forward-fix tidak memungkinkan:

1. Hentikan tulis: `docker compose stop api worker` (Caddy menampilkan 502 — umumkan pemeliharaan).
2. Restore backup pra-deploy ke database baru mengikuti `docs/governance/runbook-restore.md`
   (`ops/backup/restore.sh`, checksum + `--single-transaction`).
3. Arahkan `POSTGRES_DB` ke DB hasil restore, jalankan versi aplikasi sebelumnya (langkah 3).
4. Data yang masuk setelah backup **hilang** (RPO aktual ±24 jam tanpa WAL archiving) — catat di laporan insiden.

## 5. Kegagalan umum

| Gejala | Kemungkinan | Tindakan |
|---|---|---|
| `migrate` exit ≠ 0, api tidak start | migrasi gagal; transaksi migrasi di-rollback | perbaiki migrasi (rilis baru), versi lama tetap jalan setelah `up -d` dengan image lama |
| api start lalu exit: KEK/config | `DATA_KEK` salah/hilang, config tidak valid | proses fail-closed by design; pulihkan `.env` |
| 503 di login/checkout | Redis mati (rate limit fail-closed) | `docker compose restart redis`; cek memori |
| Antrean outbox naik terus | worker mati / provider email | `docker compose logs worker`; restart worker |
| Disk penuh (`HostDiskSpace*`) | log/backup/volume | rotasi log sudah 3×10 MB per service; periksa `backup_data`, `pg_data`; jangan hapus volume Postgres |

## 6. Drill (wajib sebelum go-live)

Di VPS staging: deploy vN → deploy vN+1 berisi migrasi expand → rollback ke vN (langkah 3) → ukur waktu sampai
`/readyz` siap → restore backup ke DB baru (langkah 4) → catat waktu & selisih data. Hasil → `docs/evidence/P8-5.txt`
dan `P8-4.txt` (RPO/RTO terukur vs target yang disetujui).
