# Dependency review (`PRD.md §7.1`)

Setiap dependency eksternal dicatat di sini **sebelum** ditambahkan ke `Cargo.toml`. Versi dipin di `Cargo.lock` (di-commit). Advisory/lisensi dicek otomatis oleh `cargo deny check` (`deny.toml`), dijalankan di CI mulai P0-7.

| Crate | Versi | Lisensi | MSRV | Dipakai oleh | Alasan | Task |
|---|---|---|---|---|---|---|
| serde | 1.0.229 | MIT OR Apache-2.0 | 1.56 | shared | (De)serialisasi config/API; de-facto standar | P0-2 |
| toml | 1.2.1 | MIT OR Apache-2.0 | 1.85 | shared | Parser `config/*.toml` | P0-2 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | 1.77 | shared | Tipe error tanpa boilerplate (`PRD.md §7.1`) | P0-2 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | 1.71 | shared, api | Envelope JSON `PRD.md §7.4` | P0-3 |
| uuid | 1.26.1 | Apache-2.0 OR MIT | 1.85 | shared | Request ID `req-<uuid v4>`; nanti UUID PK `PRD.md §5.6` | P0-3 |
| tokio | 1.54.1 | MIT | 1.85 | api | Async runtime `PRD.md §7.1` | P0-3 |
| axum | 0.8.9 | MIT | 1.80 | api | HTTP framework `PRD.md §7.1` | P0-3 |
| tower | 0.5.3 | MIT | 1.64 | api | Komposisi middleware | P0-3 |
| tower-http | 0.7.1 | MIT | 1.65 | api | trace, catch-panic, body limit, timeout, security headers | P0-3 |
| tracing | 0.1.44 | MIT | 1.65 | api | Structured logging `PRD.md §9.8` | P0-3 |
| tracing-subscriber | 0.3.23 | MIT | 1.65 | api | Formatter JSON/pretty + `RUST_LOG` filter | P0-3 |
| http-body-util | 0.1.5 | MIT | 1.61 | api (dev) | Membaca body di tes integrasi | P0-3 |

Kriteria: maintained aktif, tanpa advisory RustSec terbuka, lisensi permisif (MIT/Apache-2.0/BSD/ISC/Unicode/Zlib), MSRV ≤ `rust-version` workspace (1.85).
