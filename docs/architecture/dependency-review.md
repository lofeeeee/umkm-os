# Dependency review (`PRD.md §7.1`)

Setiap dependency eksternal dicatat di sini **sebelum** ditambahkan ke `Cargo.toml`. Versi dipin di `Cargo.lock` (di-commit). Advisory/lisensi dicek otomatis oleh `cargo deny check` (`deny.toml`), dijalankan di CI mulai P0-7.

| Crate | Versi | Lisensi | MSRV | Dipakai oleh | Alasan | Task |
|---|---|---|---|---|---|---|
| serde | 1.0.229 | MIT OR Apache-2.0 | 1.56 | shared | (De)serialisasi config/API; de-facto standar | P0-2 |
| toml | 1.2.1 | MIT OR Apache-2.0 | 1.85 | shared | Parser `config/*.toml` | P0-2 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | 1.77 | shared | Tipe error tanpa boilerplate (`PRD.md §7.1`) | P0-2 |

Kriteria: maintained aktif, tanpa advisory RustSec terbuka, lisensi permisif (MIT/Apache-2.0/BSD/ISC/Unicode/Zlib), MSRV ≤ `rust-version` workspace (1.85).
