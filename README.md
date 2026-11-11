# UMKM Business OS

Platform SaaS multi-tenant untuk UMKM: website, katalog & stok, order, ticketing, multi-admin, dan chatbot LLM. Backend wajib Rust (`docs/PRD.md §7`).

## Status

Gate A `DISETUJUI` (ADR-0002, ADR-0013). Selesai: P0-0..P0-6, P0-8..P0-10; P0-7 (CI) siap tetapi belum pernah jalan di GitHub karena repo belum punya remote — lihat `docs/governance/ci.md`. Cara menjalankan API: lihat bagian "Menjalankan stack lengkap" di bawah (API membutuhkan `UMKM_DATABASE_URL` dan `UMKM_REDIS_URL`).

## Perintah

```bash
eval "$(scripts/dev-db.sh up)"         # Postgres (55432) + Redis (56379) lokal -> DATABASE_URL, REDIS_URL; wajib untuk tes
sqlx migrate run --source migrations
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo deny check                   # advisory + lisensi dependency (PRD.md §7.1)
scripts/check-domain-boundary.sh   # domain tanpa axum/sqlx/reqwest/tokio (ADR-0013)
```

## Dokumen

| Dokumen | Isi |
|---|---|
| `docs/PRD.md` | Sumber otoritatif requirement (v1.0) |
| `docs/BUILD_PLAN.md` | Urutan kerja berbasis gate |
| `docs/OPEN_DECISIONS.md` | 12 keputusan + ADR yang diusulkan |
| `docs/RTM.md` | Register requirement (draft) |
| `docs/api/openapi.yaml` | Kontrak OpenAPI (snapshot dari kode; `UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi`) |
| `docs/decisions/` | ADR (MADR 4.0.0, append-only) |
| `docs/evidence/` | Bukti perintah + exit code per task |

## Menjalankan stack lengkap (Docker Compose)

```bash
cp .env.example .env    # isi password; jangan commit .env
docker compose up -d --build --wait
curl -H "Host: api.localhost" localhost/readyz
```

TLS produksi belum dikonfigurasi (menunggu D-01); stack hanya untuk lokal/staging.

Monitoring (Prometheus, profile opsional): `docker compose --profile monitoring up -d`. Metrics ada di port internal 9090 (`api:9090/metrics`, `worker:9090/metrics`), tidak diproxy Caddy dan tidak dipublish.

## Struktur

```text
apps/api, apps/worker  entrypoint HTTP API dan background worker
crates/shared          tipe/utilitas bersama
crates/domain          aturan bisnis murni (tanpa framework)
crates/application     use case
crates/infrastructure  adapter eksternal
crates/database        PostgreSQL/SQLx, tenant context transaction-local
migrations/            SQLx migrations (reversible: *.up.sql / *.down.sql)
scripts/               guard CI lokal, dev-db
ops/caddy/             reverse proxy (Caddyfile)
ops/monitoring/        Prometheus config + alert rules (+ tes promtool)
ops/backup/            backup harian terenkripsi (age) + restore; runbook: docs/governance/runbook-restore.md
config/              konfigurasi non-rahasia per environment (P0-2)
docs/                PRD, rencana, keputusan, bukti
.github/workflows/   CI (P0-7)
```
