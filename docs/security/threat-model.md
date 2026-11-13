# Threat model — UMKM Business OS V1.0 (STRIDE, P8-1 bagian internal)

Diperbarui: 2026-11-13. Ruang lingkup: kode di repo ini pada Gate P7. Dokumen ini **bukan** pengganti pentest/assessment
pihak ketiga (syarat P8-1: zero critical/high) — ia memetakan aset, batas kepercayaan, kontrol yang sudah diuji, dan
risiko yang masih terbuka agar assessment luar terarah.

## 1. Aset

| Aset | Mengapa penting |
|---|---|
| Data tenant (produk, pesanan, pelanggan, tiket, percakapan, file) | Kebocoran lintas toko = pelanggaran utama SaaS multi-tenant |
| Data pribadi pelanggan (nama, email, telepon, alamat, isi chat) | Kewajiban privasi; dimasking untuk peran terbatas |
| Kredensial: password (Argon2id), sesi, token sekali pakai, rahasia MFA, kunci API AI BYOK | Pengambilalihan akun / penyalahgunaan biaya AI |
| Uang & status langganan (pesanan terbayar, tagihan langganan) | Penipuan pembayaran manual, aktivasi paket tanpa bayar |
| Kunci induk (`UMKM_DATA_KEK`), kunci tanda tangan URL file, kunci provider AI platform | Kompromi = dekripsi semua rahasia / pemalsuan URL |
| Ketersediaan (VPS tunggal ±8 GB) | Toko UMKM bergantung pada website & checkout |

## 2. Komponen & batas kepercayaan

```
Internet ──► Caddy (TLS, CSP, security headers, routing per Host)
              ├─ app.<root>      → dashboard statis (Next export) ──┐ cookie sesi host-only api.<root> + CSRF
              ├─ api.<root>      → Rust API (Axum) ◄────────────────┘
              └─ {slug}.<root>   → situs toko statis + /_api/public/* (tenant dari Host; cookie hanya untuk akun pelanggan, checkout, chat)
Rust API ──► PostgreSQL (role umkm_app: NOBYPASSRLS, RLS FORCE di 60 tabel tenant; fungsi SECURITY DEFINER sempit)
         ──► Redis (rate limit, fan-out realtime)  ── fail-closed: Redis mati → 503 untuk endpoint ber-rate-limit
         ──► Provider AI (egress ber-SSRF-guard; platform = operator, BYOK = URL tenant)
Rust worker ──► PostgreSQL (transactional outbox, idempoten) ──► email (log-only sampai D-08)
Platform admin (super_admin + sesi MFA) ──► /api/v1/platform/* (satu-satunya jalur lintas tenant di API)
```

Batas utama: (B1) internet ↔ Caddy, (B2) host toko ↔ host dashboard/API (cookie & CSP terpisah), (B3) API ↔ DB
(RLS + tenant context per transaksi), (B4) API ↔ provider AI (egress tak tepercaya, output LLM tak tepercaya),
(B5) anggota tenant ↔ admin platform.

## 3. STRIDE — ancaman, kontrol, bukti

| # | STRIDE | Ancaman | Kontrol yang ada | Bukti (uji otomatis) | Sisa risiko |
|---|---|---|---|---|---|
| S1 | Spoofing | Pengambilalihan akun staf (tebak password, credential stuffing) | Argon2id; rate limit login 5/email & 30/IP per 15 mnt (fail-closed); pesan gagal tidak membedakan email ada/tidak; MFA TOTP (wajib super_admin, opsional/wajib per toko untuk admin) | `apps/api/tests/auth_http.rs`, `mfa_http.rs`, `docs/evidence/P1-2.txt`, `P4-1b.txt` | MFA belum wajib untuk semua Owner (keputusan toko). Belum ada deteksi credential stuffing lintas IP selain rate limit. |
| S2 | Spoofing | Sesi dicuri/ditanam (cookie, CSRF) | Cookie `__Host-` HttpOnly Secure SameSite=Lax host-only (ADR-0015); CSRF header terikat sesi; cek Origin; rotasi sesi saat login; MFA mencabut sesi lain | `auth_http.rs`, `tenancy.rs`, `P1-3.txt` | XSS di dashboard akan melemahkan CSRF (lihat T2). |
| S3 | Spoofing | Pelanggan berpura-pura pemilik pesanan / sesi chat orang lain | Token akses pesanan & sesi chat acak 256-bit, hanya hash di DB, host-only per toko; akun pelanggan hanya ditautkan setelah email terverifikasi (ADR-0005) | `checkout_http.rs`, `chat_http.rs`, `crm_http.rs` | — |
| T1 | Tampering | Mengubah data tenant lain (IDOR) | Tenant dari membership (path diverifikasi), RLS FORCE + tenant context transaksi, FK komposit `(tenant_id, id)` | `crates/infrastructure/tests/rls.rs`, `isolation_suite_4.rs`, RTM §6 (9/9) | Fungsi SECURITY DEFINER dapat dieksekusi role aplikasi; otorisasinya di API (lihat E3). |
| T2 | Tampering | XSS tersimpan via konten website/pesan chat/nama produk | Konten halaman = JSON schema tervalidasi server (tanpa HTML bebas, ADR-0007); React meng-escape; CSP tanpa `unsafe-inline` untuk script (hash per halaman); URL disaring (`unsafe_url`) | `crates/domain/tests/website_fuzz.rs` (≥10.000 input), `P2-1.txt`, `P2-7.txt` (NC CSP), `e2e/chat.spec.ts` (HTML tampil sebagai teks) | `style-src 'unsafe-inline'` masih diizinkan (gaya); dampak rendah. |
| T3 | Tampering | Memalsukan pembayaran / mengaktifkan paket sendiri | Pesanan: verifikasi staf `payments.verify`, SoD opsional, state machine trigger DB. Langganan: nominal dari katalog (trigger), toko tidak bisa menandai lunas (trigger 42501), keputusan hanya fungsi platform | `payment_http.rs`, `subscription_billing_http.rs` (negative control), `P7-3.txt` | Verifikasi manual bergantung kejelian manusia terhadap bukti bayar palsu. |
| T4 | Tampering | Mengubah audit log / riwayat | Audit, event pesanan/tiket/percakapan append-only (REVOKE UPDATE/DELETE); purge hanya via fungsi retensi | `audit_http.rs`, `conversation_schema.rs` | Pemilik DB/operator tetap bisa mengubah — perlu kontrol operasional. |
| R1 | Repudiation | Menyangkal aksi sensitif | Audit 10/10 kategori PRD §6.9 dengan aktor (user/platform), request id, IP, ringkasan before/after teredaksi | `audit_http.rs` (`covered == 10`) | Retensi audit 1 tahun (ADR-0012). |
| I1 | Info disclosure | Kebocoran lintas tenant via API, SSE, cache, job | RLS; SSE TenantGuard per event; SSE staf hanya ping (tanpa isi chat); SSE pelanggan per percakapan + cek sesi; namespace Redis; worker memakai tenant context event | `realtime_http.rs`, `conversation_staff_http.rs`, `chat_http.rs`, `P1-10.txt` | — |
| I2 | Info disclosure | Peran terbatas melihat PII | Masking viewer percakapan (FR-CNV-003) untuk pembaca tanpa `conversations.reply`+`customers.read` | `conversation_viewer_http.rs` | Aturan masking = tafsiran engineering (product-decision-log). |
| I3 | Info disclosure | Rahasia bocor (kunci AI, rahasia MFA, token) | Enkripsi envelope XChaCha20-Poly1305 + KEK berversi (ADR-0011); kunci hanya ditampilkan masked; redaksi log/audit; token & kode pemulihan hanya hash | `P5-2a-crypto.txt`, `P5-2b.txt`, `mfa_http.rs` | Kompromi KEK di host = dekripsi semua rahasia; rotasi KEK manual. |
| I4 | Info disclosure | File privat diakses tanpa izin | Signed URL HMAC ber-expiry terikat tenant+objek; RLS objek; gambar diperiksa magic bytes; `nosniff` + `sandbox` CSP | `upload_http.rs`, `P2-6.txt` | — |
| I5 | Info disclosure | Prompt injection membocorkan instruksi/data | Guardrail input/output, canary bocor instruksi, tool backend tenant-scoped dengan otorisasi per tool, data pelanggan hanya untuk pelanggan terverifikasi | `crates/application/tests/orchestrator.rs`, `P5-5.txt` | Model dapat berhalusinasi; mode Hybrid/Assisted untuk mitigasi. |
| D1 | Denial of service | Banjir permintaan publik (registrasi, checkout, chat, login) | Rate limit per IP/subjek fail-closed; batas body & timeout request; batas koneksi SSE | `auth_http.rs` (429/503), `P1-2.txt` | **Tidak ada WAF/proteksi DDoS di depan VPS tunggal**; rate limit per IP lemah terhadap botnet. |
| D2 | Denial of service | Penyalahgunaan biaya AI | Kuota token per paket (Starter 0 token platform), anggaran BYOK, circuit breaker, timeout provider | `plan_limits_http.rs`, `ai_runtime.rs` | — |
| D3 | Denial of service | Kehabisan sumber daya VPS | Batas memori/CPU per service di Compose; batas paket (produk, file, percakapan) | `docker-compose.yml`, `plan_limits_http.rs` | Belum diukur di bawah beban (P8-2). |
| E1 | Elevation | Anggota menaikkan hak sendiri / role kustom | Anti-eskalasi: hanya boleh memberi permission yang dimiliki; Owner terakhir dilindungi | `team_http.rs`, `P4-1.txt` | — |
| E2 | Elevation | Melewati approval | Gate approval di endpoint + executor atas nama pemohon dengan hak saat ini | `approval_http.rs`, `conversation_staff_http.rs` | — |
| E3 | Elevation | Mengakses area platform | `PlatformAdmin`: `platform_role = super_admin` aktif + sesi lolos MFA; fungsi lintas tenant hanya dipanggil setelah itu; akses diaudit | `subscription_billing_http.rs`, `analytics_overview_http.rs` (Owner ber-MFA ditolak) | Fungsi `app.platform_*` dapat dieksekusi role aplikasi → bug otorisasi di API = akses lintas tenant (assessment luar perlu fokus di sini). |
| E4 | Elevation | SSRF ke jaringan internal lewat URL provider BYOK | 9 kontrol egress: blok IP privat/metadata, DNS campuran, pinning anti-rebinding, tanpa redirect, batas ukuran & waktu | `crates/infrastructure/tests/egress.rs`, `phase-5-ai.txt` | — |

## 4. Risiko terbuka (perlu keputusan / pihak luar)

1. **Tidak ada backup luar VPS** (D-07): kehilangan VPS = kehilangan data. RPO ±24 jam (tanpa WAL archiving).
2. **Tidak ada pentest pihak ketiga** — prioritas: E3 (fungsi SECURITY DEFINER), T3 (pembayaran manual), I5 (prompt
   injection), T2 (CSP/editor).
3. **VPS tunggal tanpa WAF/DDoS protection** (D1).
4. **Email transaksional log-only** (D-08): verifikasi/reset/undangan belum sampai ke pengguna nyata.
5. **Teks legal placeholder** (D-12): privacy, terms, consent AI, billing.
6. **Kunci induk di environment host**: rotasi & penyimpanan (mis. secret manager) perlu prosedur ops.
7. **Alert tanpa kanal on-call**: insiden tidak terdeteksi tepat waktu.

## 5. Untuk assessment luar

Lingkup yang disarankan: OWASP ASVS L2 untuk API & dashboard, uji isolasi tenant (path/Host/SSE/file), alur pembayaran
manual & area platform, prompt injection pada chat publik, konfigurasi Caddy/CSP/TLS, dan review fungsi SECURITY DEFINER
di `migrations/`.
