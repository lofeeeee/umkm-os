# CI fail-closed (`PRD.md §14.2`, P0-7)

Workflow: `.github/workflows/ci.yml`.

| Job | Isi |
|---|---|
| `lint` | `cargo fmt --check`, `cargo clippy -D warnings`, `scripts/check-domain-boundary.sh`, `scripts/check-ci-needs.sh` |
| `test` | Postgres pgvector:pg16 service; migration up/down/up via `sqlx-cli`; `cargo test --workspace --locked` |
| `supply-chain` | `cargo deny check` (advisory RustSec, lisensi, sumber) |
| `containers` | `docker compose config -q`, `docker build` target `api` + `worker` |
| `workflow-lint` | actionlint |
| `ci-gate` | `if: always()`; `scripts/ci-gate.sh` lulus hanya bila semua job `success` |

## Aturan fail-closed

- `ci-gate` gagal bila ada job `failure`, `skipped`, `cancelled`, result hilang, atau `needs` kosong.
- `scripts/check-ci-needs.sh` gagal bila ada job yang tidak masuk `ci-gate.needs`, atau `if: always()` hilang.
- Semua action dipin ke commit SHA; `permissions: contents: read`; `persist-credentials: false`.
- Toolchain dipin di `rust-toolchain.toml`.

## Wajib diset manual setelah repo punya remote GitHub

Branch protection untuk `main`:

1. Require a pull request before merging.
2. Require status checks to pass: **hanya `ci-gate`** (cukup satu; ia mengagregasi semuanya).
3. Require branches to be up to date before merging.
4. Do not allow bypassing the above settings.

Belum dikerjakan (tahap berikutnya `PRD.md §14.2`): frontend tests (setelah ADR-0014), staging deploy, smoke test, production approval/deploy.
