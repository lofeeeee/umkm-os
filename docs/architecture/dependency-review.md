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
| axum | 0.8.9 | MIT | 1.80 | api, worker (endpoint `/metrics`) | HTTP framework `PRD.md §7.1` | P0-3 |
| tower | 0.5.3 | MIT | 1.64 | api | Komposisi middleware | P0-3 |
| tower-http | 0.7.1 | MIT | 1.65 | api | trace, catch-panic, body limit, timeout, security headers | P0-3 |
| tracing | 0.1.44 | MIT | 1.65 | api | Structured logging `PRD.md §9.8` | P0-3 |
| tracing-subscriber | 0.3.23 | MIT | 1.65 | api | Formatter JSON/pretty + `RUST_LOG` filter | P0-3 |
| http-body-util | 0.1.5 | MIT | 1.61 | api (dev) | Membaca body di tes integrasi | P0-3 |
| redis | 1.8.1 | BSD-3-Clause | 1.88 | infrastructure | Klien Redis `PRD.md §9.5`; `default-features = false` + `tokio-comp`, `connection-manager`, `script` (P1-2 rate limit). 2.0 masih RC, tidak dipakai. Transitif `xxhash-rust` ber-lisensi BSL-1.0 → ditambahkan ke allowlist `deny.toml` | P0-6 |
| prometheus-client | 0.26.0 | Apache-2.0 | – | infrastructure | Library resmi Prometheus/OpenMetrics untuk 16 metric `PRD.md §9.8`; tanpa fitur protobuf | P0-8 |
| utoipa | 6.0.0 | MIT OR Apache-2.0 | 1.88 | api; shared (opsional, feature `openapi`) | OpenAPI dari kode `PRD.md §7.1`; fitur `yaml` hanya di api. Tidak masuk ke domain (diverifikasi `cargo tree`) | P0-9 |
| argon2 | 0.6.0 | MIT OR Apache-2.0 | 1.85 | infrastructure | Argon2id password hashing `PRD.md §4.1`, `PRD.md §6.1` (RustCrypto) | P1-2 |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | 1.88 | infrastructure | SHA-256 hash token opak & subjek rate limit | P1-2 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | 1.85 | infrastructure | Sumber acak OS untuk token 256-bit | P1-2 |
| base64 | 0.22.1 | MIT OR Apache-2.0 | 1.48 | infrastructure | Encoding token base64url | P1-2 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | 1.71 | api | Stream SSE (`stream::unfold`); sudah transitif lewat axum, `default-features = false` + `std` | P1-10 |
| sqlx | 0.9.0 | MIT OR Apache-2.0 | 1.94 | database | DB access + migration `PRD.md §7.1`; `default-features = false`, hanya postgres/tokio/macros/migrate/uuid + `json` (P2-1: kolom `jsonb` website, memakai serde_json yang sudah ada) | P0-4, P2-1 |
| hmac | 0.13.0 | MIT OR Apache-2.0 | 1.85 | infrastructure | HMAC-SHA256 signed URL file (ADR-0009), verifikasi konstan-waktu; sudah ada di Cargo.lock lewat sqlx-postgres (RustCrypto) | P2-6 |
| chacha20poly1305 | 0.11.0 (2026-06-28) | Apache-2.0 OR MIT | 1.85 | infrastructure | Envelope encryption rahasia (kunci AI BYOK, rahasia MFA): XChaCha20-Poly1305 untuk DEK & data (ADR-0011). RustCrypto; diaudit NCC Group (2020). Menambah aead 0.6.1, poly1305 0.9.1; chacha20 & getrandom sudah ada. `default-features = false`, `alloc` + `getrandom` | P5-2 |
| reqwest | 0.13.5 (2026-09-08) | MIT OR Apache-2.0 | 1.85 | infrastructure | Satu-satunya klien HTTP keluar (provider AI/embedding). `default-features = false`: `rustls-no-provider` + `http2`; tanpa proxy sistem, tanpa native-tls/OpenSSL. Dipakai lewat SSRF guard (DNS dipatok ke IP tervalidasi, redirect mati, batas ukuran/timeout) | P5-3 |
| rustls | 0.23.46 (2026-10-14) | Apache-2.0 OR ISC OR MIT | 1.79 | infrastructure | TLS untuk reqwest dengan provider kripto `ring` (bukan aws-lc: tanpa build C/CMake). Membawa ring 0.17.14 (Apache-2.0 AND ISC), hyper-rustls 0.27.10, tokio-rustls, rustls-platform-verifier 0.7.1 (sertifikat OS; image runtime kini memasang ca-certificates) | P5-3 |
| sha1 | 0.11.0 | MIT OR Apache-2.0 | 1.85 | infrastructure | HMAC-SHA1 untuk TOTP RFC 6238 (AUTH-011; SHA-1 dipilih di Gate F demi kompatibilitas authenticator — HMAC-SHA1 tidak terdampak serangan tabrakan SHA-1). RustCrypto; sudah ada di Cargo.lock secara transitif, `default-features = false`. Tidak dipakai untuk hal lain | AUTH-011 |
| webpki-root-certs | 1.0.11 (2026-10-23) | CDLA-Permissive-2.0 | — | transitif | Fallback root CA untuk platform verifier. Lisensi data permisif → pengecualian deny.toml khusus crate ini. 1.0.12 (2026-11-10) belum 2 minggu → dipin 1.0.11 | P5-3 |

Kriteria: maintained aktif, tanpa advisory RustSec terbuka, lisensi permisif (MIT/Apache-2.0/BSD/ISC/Unicode/Zlib/BSL-1.0), MSRV ≤ `rust-version` workspace (1.94; dinaikkan dari 1.85 di P0-4 untuk sqlx 0.9.0, rilis 2026-05-21).

## Frontend (npm) — ADR-0014, P2-3/P2-7

Versi dipin **exact** (`.npmrc save-exact`), dipilih sebagai rilis stabil tertinggi yang terbit ≥ 2 minggu sebelum 2026-11-12 (≤ 2026-10-29). Tidak ada Node di image produksi (target `web` = Caddy + `out/`).

| Paket | Versi | Lisensi | Peran | Alasan / catatan |
|---|---|---|---|---|
| next | 16.4.3 | MIT | build | Static export `PRD.md §9.2`; tidak dijalankan sebagai server |
| react, react-dom | 19.3.1 | MIT | runtime browser | UI |
| typescript | 6.0.3 | Apache-2.0 | dev | 7.x (port native) dihindari: build Next memakai API JS TypeScript |
| eslint | 9.39.5 | MIT | dev | `eslint-config-next` 16.4.3 belum mendukung ESLint 10. npm menandai 9.x deprecated → naikkan saat config Next mendukung 10 |
| eslint-config-next | 16.4.3 | MIT | dev | Aturan Next/React hooks; + `react/no-danger` & larangan `dangerouslySetInnerHTML` |
| vitest | 4.1.12 | MIT | dev | Unit test renderer (`renderToStaticMarkup`, tanpa DOM). 5.x memicu crash resolver npm 10 |
| @vitejs/plugin-react | 6.1.2 | MIT | dev | JSX untuk vitest |
| @playwright/test | 1.64.1 | Apache-2.0 | dev | Uji browser responsif + alur editor terhadap stack nyata |
| @types/react, @types/react-dom | 19.3.0 | MIT | dev | Tipe |
| @types/node | 26.6.6 | MIT | dev | Tipe |

Catatan:
- `.npmrc legacy-peer-deps=true`: npm 10.9.8 crash (`edgesOut` null) saat menelusuri optional peer `vitest@*` → `@vitest/browser-playwright`. Konflik peer tetap diperiksa: CI menjalankan `npm ls` (gagal bila ada peer invalid/missing).
- jsdom & @testing-library/react tidak dipakai (rantai optional peer `@napi-rs/canvas` memicu crash yang sama; render ke string lebih ketat untuk uji escaping).
- `npm audit --audit-level=high`: lulus. Ada 1 advisory **moderate** (minimatch ReDoS, GHSA-g3hv-75wg-m249) di rantai dev `typescript-eslint` ← `eslint-config-next`; hanya tooling lint dengan pola glob milik repo, tidak masuk build/produksi. Perbaikan menunggu `eslint-config-next` yang memakai typescript-eslint > 8.72.2.
