---
status: Accepted
date: 2026-11-12
decision-makers: Product owner (pemilik repo)
---

# ADR-0011 — Kunci AI: provider platform DAN kunci milik tenant (BYOK)

## Context and Problem Statement

D-09 (`PRD.md §20 #9`, `PRD.md §4.10`, `PRD.md §6.5–6.7`): AI memakai kunci milik tenant (BYOK), kunci platform,
atau keduanya. Pemilik produk memilih **keduanya** (2026-11-12): "it can use platform as a provider or they bring one".

## Considered Options

* BYOK saja (rekomendasi awal)
* Kunci platform saja
* Keduanya: tenant memilih sumber provider per konfigurasi

## Decision Outcome

Chosen option: "Keduanya".

| Aturan | Nilai |
|---|---|
| Sumber provider tenant | `platform` atau `byok`, dipilih per tenant (provider utama dan fallback masing-masing) |
| Provider platform | didaftarkan operator lewat konfigurasi + secret runtime (`[ai.platform]` base URL, model yang diizinkan; `UMKM_PLATFORM_AI_API_KEY`). Tenant tidak pernah melihat kunci/URL platform. Endpoint internal (vLLM/Ollama) hanya boleh didaftarkan operator di sini (`PRD.md §6.5` #9) |
| Kuota platform | biaya ditanggung platform → kuota token bulanan per tenant (default dari konfigurasi; angka final & harga mengikuti D-10). Kuota habis → AI berhenti menjawab, percakapan dialihkan ke manusia (Hybrid/handover) |
| BYOK | endpoint OpenAI-compatible milik tenant: HTTPS wajib, lolos SSRF guard 9 kontrol (`PRD.md §6.5`), kunci dienkripsi envelope, hanya ditampilkan masked, dapat dirotasi/dinonaktifkan. Budget token tenant opsional (perlindungan biaya milik tenant) |
| Enkripsi rahasia | envelope: DEK acak per rahasia (XChaCha20-Poly1305), DEK dibungkus master key (KEK) dari secret runtime `UMKM_DATA_KEK` (32 byte, base64) dengan versi; rotasi = tambah versi KEK baru lalu bungkus ulang DEK (job). KEK tidak pernah di DB/repo/image. Dipakai juga untuk rahasia MFA TOTP |
| HTTP keluar | satu klien terkendali (`reqwest` + rustls): resolusi DNS sendiri, semua IP divalidasi, koneksi dipatok ke IP yang tervalidasi (anti DNS rebinding), redirect dimatikan, timeout + batas ukuran respons + batas konkurensi; kredensial hanya dikirim setelah host lolos |
| Embedding (RAG) | provider eksternal yang sama (platform atau BYOK, endpoint `/embeddings`); tidak ada inference lokal (`PRD.md §8.2`) |
| Metering | setiap panggilan mencatat token prompt/completion, model, sumber (platform/byok), latensi, hasil — untuk kuota platform dan laporan tenant |

### Consequences

* Good: tenant kecil bisa langsung memakai AI tanpa akun provider; tenant yang ingin kendali/biaya sendiri bisa BYOK.
* Bad: platform menanggung biaya token platform → kuota wajib sejak awal; harga/limit final menunggu D-10.
* Bad: dependency baru (TLS client, AEAD) — direview di `docs/architecture/dependency-review.md`.

## More Information

Status di `OPEN_DECISIONS.md`: D-09 `DISETUJUI` 2026-11-12. Consent AI (D-12 bagian AI) disetujui bersyarat di
product decision log. Append-only.
