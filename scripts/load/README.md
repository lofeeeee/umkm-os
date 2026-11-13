# Uji beban (P8-2, `docs/PRD.md §13.4`)

Benchmark lokal untuk skala keputusan Gate H (100–500 UMKM). **Bukan jaminan kapasitas produksi** — angka VPS
diukur ulang saat VPS tersedia (checklist V1.0 #30).

## Isi

| File | Fungsi |
|---|---|
| `seed.sql` | dataset `:n` toko (default 500): pemilik + sesi deterministik `lt-<slug>`, 20 produk, 50 pelanggan, 100 pesanan, 5 tiket, 10 percakapan; toko ganjil Pro + chat AI |
| `copy_website.sql` | salin website terbit milik `load-0001` (dibuat lewat API asli) ke semua toko |
| `scenarios.js` | k6: `public` 100 VU, `admin` 30, `api` 50 (70% baca / 30% tulis tanpa jeda), `chat` 20; `all` = keempatnya bersamaan |
| `run.sh` | jalankan satu skenario (image `grafana/k6:0.57.0`) sambil merekam CPU/RSS API & worker, CPU/RAM Postgres & Redis, `db_connections_active`, `db_pool_wait_seconds`, `worker_queue_depth` tiap ±2 detik |

## Langkah

```sh
eval "$(scripts/dev-db.sh up)"                       # Postgres/Redis uji (BUKAN produksi)
docker exec umkm-os-pg-test psql -U umkm_owner -d umkm_dev -c "CREATE DATABASE loadtest"
sqlx migrate run --database-url "${DATABASE_URL%/*}/loadtest"
docker exec -i umkm-os-pg-test psql -U umkm_owner -d loadtest -v n=500 -v root=umkm.test < scripts/load/seed.sql
# Jalankan umkm-api + umkm-worker (release) terhadap DB loadtest dengan config:
#   [server] trust_forwarded_for = true   (tiap pembeli virtual punya IP sendiri — rate limit per IP)
#   [database] max_connections = 20       (= produksi)
#   [ai.platform] → mock LLM: python3 -I frontend/e2e/mock-llm.py 18089
# Terbitkan website load-0001 lewat API (PUT .../website {"template_key":"modern_store"}, POST .../pages/home/publish), lalu:
docker exec -i umkm-os-pg-test psql -U umkm_owner -d loadtest < scripts/load/copy_website.sql
docker exec umkm-os-pg-test psql -U umkm_owner -d loadtest -tAc \
  "select json_agg(json_build_object('t', id, 'slug', slug, 'pro', (substr(slug,6))::int % 2 = 1) order by slug) from tenants where slug like 'load-%'" > /tmp/x/tenants.json
scripts/load/run.sh all /tmp/x/out /tmp/x/tenants.json 120s
```

Bersihkan setelahnya: hentikan API/worker/mock, `DROP DATABASE loadtest`. Image k6 ±60 MB tetap di Docker
(hapus manual bila perlu: `docker rmi grafana/k6:0.57.0`).

## Batasan yang diketahui

- Diukur langsung ke API (tanpa Caddy/TLS); NFR-003 TTFB halaman publik = shell statis + data API.
- Provider AI = mock lokal (latensi provider nyata tidak termasuk; `ai_request_duration_seconds` mencatatnya di produksi).
- Mesin uji = laptop pengembang (WSL2), Postgres/Redis/API/k6 berbagi CPU; VPS 8 GB akan berbeda.
