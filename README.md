# UMKM Business OS

Platform SaaS multi-tenant untuk UMKM: website, katalog & stok, order, ticketing, multi-admin, dan chatbot LLM. Backend wajib Rust (`docs/PRD.md §7`).

## Status

Gate A `DISETUJUI` (ADR-0002, ADR-0013). Phase 0 dan Phase 1 selesai (Gate P1: `docs/evidence/phase-1-isolation.txt`). Phase 2 (website builder) selesai: `docs/evidence/phase-2-website.txt`; P0-7 (CI) siap tetapi belum pernah jalan di GitHub karena repo belum punya remote — lihat `docs/governance/ci.md`. Cara menjalankan API: lihat bagian "Menjalankan stack lengkap" di bawah (API membutuhkan `UMKM_DATABASE_URL` dan `UMKM_REDIS_URL`).

## Perintah

```bash
eval "$(scripts/dev-db.sh up)"         # Postgres (55432) + Redis (56379) lokal -> DATABASE_URL, REDIS_URL; wajib untuk tes
sqlx migrate run --source migrations
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo deny check                   # advisory + lisensi dependency (PRD.md §7.1)
scripts/check-domain-boundary.sh   # domain tanpa axum/sqlx/reqwest/tokio (ADR-0013)
python3 scripts/check-rtm.py       # RTM dua arah FR -> AC -> TC (fail-closed)

# Frontend (ADR-0014: static export, tanpa Node di produksi)
cd frontend && npm ci && npm run lint && npm run typecheck && npm test
NEXT_PUBLIC_API_ORIGIN=http://api.umkm.test npm run build      # -> out/ + out/csp.caddy
npm run dev                         # dashboard di localhost:3000 (API dev: CORS mengizinkan localhost:3000)
```

Uji browser terhadap stack nyata (`frontend/e2e`, Playwright) dan smoke Caddy (`scripts/smoke-adr0014.sh`): lihat langkah di `docs/evidence/P2-7.txt`. Gunakan domain lokal `umkm.test`, **bukan** `*.localhost`: browser memperlakukan `app.localhost` dan `api.localhost` sebagai site berbeda sehingga cookie sesi ditolak.

## Dokumen

| Dokumen | Isi |
|---|---|
| `docs/PRD.md` | Sumber otoritatif requirement (v1.0) |
| `docs/BUILD_PLAN.md` | Urutan kerja berbasis gate |
| `docs/OPEN_DECISIONS.md` | 12 keputusan + ADR yang diusulkan |
| `docs/RTM.md` | RTM dua arah final (§8: FR → AC → TC → API → ADR → Evidence) |
| `docs/requirements/02-FSD.md` | FSD: 73 FR / 83 AC dari `PRD.md §4.3–§4.15` |
| `docs/test/test-plan.md` | Test plan, exit criteria per gate, katalog TC |
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
frontend/              Next.js static export: dashboard (app.) + renderer situs tenant ({slug}.)
ops/caddy/             reverse proxy (Caddyfile; routing host ADR-0014)
ops/monitoring/        Prometheus config + alert rules (+ tes promtool)
ops/backup/            backup harian terenkripsi (age) + restore; runbook: docs/governance/runbook-restore.md
config/              konfigurasi non-rahasia per environment (P0-2)
docs/                PRD, rencana, keputusan, bukti
.github/workflows/   CI (P0-7)
```
