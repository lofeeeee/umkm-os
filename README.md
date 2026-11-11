# UMKM Business OS

Platform SaaS multi-tenant untuk UMKM: website, katalog & stok, order, ticketing, multi-admin, dan chatbot LLM. Backend wajib Rust (`docs/PRD.md §7`).

## Status

Gate A `DISETUJUI` (ADR-0002, ADR-0013). Selesai: P0-0..P0-4. Jalankan API lokal: `UMKM_ENV=development cargo run -p umkm-api` lalu `curl localhost:8080/healthz`.

## Perintah

```bash
eval "$(scripts/dev-db.sh up)"         # Postgres lokal (port 55432) -> DATABASE_URL; wajib untuk tes DB
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
| `docs/decisions/` | ADR (MADR 4.0.0, append-only) |
| `docs/evidence/` | Bukti perintah + exit code per task |

## Struktur

```text
apps/api, apps/worker  entrypoint HTTP API dan background worker
crates/shared          tipe/utilitas bersama
crates/domain          aturan bisnis murni (tanpa framework)
crates/application     use case
crates/infrastructure  adapter eksternal
crates/database        PostgreSQL/SQLx, tenant context transaction-local
migrations/            SQLx migrations (reversible: *.up.sql / *.down.sql)
scripts/               guard CI lokal
config/              konfigurasi non-rahasia per environment (P0-2)
docs/                PRD, rencana, keputusan, bukti
.github/workflows/   CI (P0-7)
```
