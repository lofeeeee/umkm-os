# config/

Konfigurasi non-rahasia per environment (`PRD.md §14.1`: development, staging, production).

| Aturan | Sumber |
|---|---|
| Tidak ada secret di file ini; secret di-inject saat runtime | `PRD.md §6.7`, `PRD.md §14.3` |
| Database dan secret setiap environment terpisah | `PRD.md §14.1` |
| Root domain memakai `<ROOT_DOMAIN>` sampai D-01 `DISETUJUI` | `OPEN_DECISIONS.md` D-01 |

File: `development.toml`, `staging.toml`, `production.toml`. Loader: `crates/shared/src/config.rs`. Environment dipilih lewat `UMKM_ENV` (wajib, tanpa default).
