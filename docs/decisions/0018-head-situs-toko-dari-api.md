---
status: Accepted
date: 2026-11-13
decision-makers: Product owner (pemilik repo)
amends: ADR-0014
---

# ADR-0018 — `<head>` situs toko (SEO & Open Graph) dirender API per toko

## Context and Problem Statement

ADR-0014 menyajikan situs toko sebagai satu shell statis; isi diambil JavaScript dari `/_api/public/site`.
Judul, deskripsi, dan Open Graph karenanya baru ditulis setelah JavaScript berjalan. Crawler pratinjau tautan
(WhatsApp, Facebook, Telegram) **tidak menjalankan JavaScript**, sehingga tautan toko yang dibagikan di WhatsApp
tidak menampilkan judul/gambar — padahal WhatsApp adalah saluran utama UMKM (riset Studio §5.1, G-SEO-02).

## Considered Options

* API merender HTML shell + meta per toko (usulan engineering)
* Caddy `templates` mengisi meta dari endpoint JSON
* Tetap meta dari JavaScript saja

## Decision Outcome

Chosen option: "API merender HTML shell + meta per toko" (pemilik produk, 2026-11-13).

* Host toko: permintaan **dokumen** (path tanpa ekstensi file, mis. `/`, `/produk/kopi`) diteruskan Caddy ke
  `GET /api/v1/public/document` dengan Host asli (tenant tetap hanya dari Host, ADR-0014 butir 1).
* API membaca shell statis hasil build (`site.html`, path dari konfigurasi, dibaca saat start) lalu
  menyisipkan sebelum `</head>`: `<title>`, `meta description`, `link rel=canonical`, Open Graph
  (`og:title/description/image/url/type`, `og:locale=id_ID`), `twitter:card`, favicon, dan JSON-LD
  `LocalBusiness` (data blok `application/ld+json`, bukan skrip yang dieksekusi). Semua nilai di-escape HTML;
  JSON-LD di-escape untuk konteks `<script>` (`<` → `\u003c`).
* Sumber data: versi **terbit** saja (ADR-0014 butir 3) + profil bisnis. Draf tidak pernah bocor.
* Aset statis (`/_next/*`, gambar, font) tetap dilayani Caddy langsung. Bila API gagal, Caddy jatuh balik ke
  shell statis (situs tetap tampil, hanya tanpa meta).
* `Cache-Control: public, max-age=60` sama seperti data publik; CSP host toko tidak berubah.
* `GET /robots.txt` dan `/sitemap.xml` per toko juga dari API.

### Consequences

* Good: pratinjau WhatsApp/Facebook dan SEO dasar bekerja tanpa Node server (ADR-0014 tetap: tanpa SSR).
* Good: tidak ada skrip inline baru → CSP berbasis hash tetap valid.
* Bad: API perlu akses baca ke file shell build frontend (volume read-only) dan Caddy perlu aturan baru.
* Bad: satu hop API tambahan untuk permintaan dokumen (di-cache 60 dtk).

## More Information

Append-only.
