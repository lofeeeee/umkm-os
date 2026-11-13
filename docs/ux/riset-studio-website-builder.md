# Riset celah fitur Studio vs website builder pembanding

- Tanggal riset & akses sumber: **2026-11-13** (semua tautan di Daftar Pustaka diakses pada tanggal ini kecuali disebut lain).
- Lingkup: prototipe Studio (ADR-0017, status *Proposed*), dibandingkan dengan website builder global & lokal serta standar wajib (WCAG 2.2, Core Web Vitals, Google Search Central, UU PDP).
- Dokumen ini **riset**, bukan keputusan. Butir yang menyentuh kontrak `content_json`, validator Rust, CSP, atau paket berlangganan perlu ADR tersendiri.
- Notasi sitasi: `[n]` merujuk ke Daftar Pustaka (§8). `(sekunder)` = sumber pihak ketiga karena sumber primer tidak ditemukan. `belum terverifikasi` = klaim tidak dapat dipastikan dari sumber primer pada tanggal akses.

---

## 1. Ringkasan eksekutif (celah terpenting)

1. **Belum ada jalur "simpan ke server → terbit → situs publik"** (draf vs. terbit, pratinjau, rollback). Semua pembanding memisahkan draf dan versi terbit; Wix [W5][W6], Webflow [WF1], Framer [F3], Shopify (tema draf vs. live) [S5]. Ini syarat mutlak rilis (P0) dan menuntut validator Rust `schema_version = 2`.
2. **Riwayat versi & pemulihan** di sisi server tidak ada (Undo/Redo hanya sesi lokal). Wix *Site History* [W5], Webflow *backups* (titik pulih otomatis tiap 50 simpan otomatis) [WF1]. P0 ringan: simpan N versi terbit terakhir + tombol "Pulihkan".
3. **SEO dasar kosong**: renderer publik tidak menulis `<title>`/meta description/Open Graph/kanonik/sitemap/structured data (dicek di `frontend/src/renderer/Site.tsx`). Google mewajibkan judul unik & deskriptif [G1][G2], sitemap [G3], dan menyediakan LocalBusiness/Product rich result [G4][G5]; Canva Websites pun punya judul/deskripsi per halaman + favicon [C3].
4. **Aksesibilitas konten**: gambar Banner/Tentang/Promo/produk dirender dengan `alt=""` (dekoratif) padahal informatif → melanggar WCAG 2.2 SC 1.1.1 [A1]. Pembanding meminta alt text di level aset (Webflow [WF3], Framer [F5]); ATAG 2.0 B.2.1 meminta editor *mendorong* penulis membuat konten aksesibel [A2].
5. **Aksesibilitas editor**: drag/resize punya alternatif keyboard, tetapi WCAG 2.2 SC 2.5.7 mensyaratkan alternatif **single pointer tanpa seret** (mis. tombol ▲▼◀▶/lebar ±) [A1]; target sentuh minimal 24×24 px (SC 2.5.8) [A1]; pegangan resize `aria-hidden`.
6. **Multi-halaman & navigasi**: Studio hanya satu halaman. Canva Websites mendukung hingga 45 halaman dengan menu otomatis dari judul halaman [C4]; Shopify punya template per jenis halaman [S4]. UMKM minimal butuh: Beranda, Produk/Katalog, Detail produk, Tentang, Kontak, Kebijakan (privasi/pengembalian).
7. **Kontrol per perangkat**: Studio hanya "tumpuk otomatis" di ponsel. Wix [W1], Squarespace [Q2] mengizinkan layout/urutan ponsel terpisah dan **sembunyikan elemen di ponsel**; minimal yang dibutuhkan: urutan ponsel + `hide_on: mobile|desktop` per blok.
8. **Formulir & pengumpulan data pembeli** (kontak, pesan/pre-order, RSVP) belum ada; ini juga memicu kewajiban UU PDP (dasar pemrosesan, hak subjek, retensi) [L1]. Carrd menjadikan form fitur inti berbayar [K2][K3].
9. **WhatsApp-first** belum jadi unit kelas satu: tombol WA melayang, tombol "Pesan via WA" per produk dengan teks terisi otomatis (`wa.me/<no>?text=`) [M1 (sekunder)]. Bagi UMKM Indonesia ini saluran konversi utama, lebih penting dari keranjang.
10. **Pengumuman/promo terjadwal**: bilah pengumuman, pop-up promo, dan **jadwal tampil** (mulai/selesai) belum ada; Squarespace (sekunder) [Q5], Linktree *Schedule links* [L2]. Cocok untuk Ramadan/Harbolnas/gajian.
11. **Pustaka media**: belum ada pustaka aset terpusat dengan alt text, crop/focal point, dan pemakaian ulang; foto harus diunggah ulang per plugin. Webflow menyimpan alt di aset dan diwarisi semua instance [WF3].
12. **Domain & identitas situs**: favicon, nama situs, gambar berbagi (OG image), dan (nanti) domain sendiri. ADR-0008 menunda custom domain; Canva hanya membolehkan domain sendiri di paket berbayar [C2] → pembeda paket Pro yang alami.
13. **Analitik dasar** (pengunjung, klik WA, klik produk) belum ada; Canva menyambungkan Google Analytics + banner persetujuan cookie [C3]. Rekomendasi: analitik first-party tanpa cookie (CSP tetap ketat, risiko UU PDP minimal).
14. **AI di builder**: chatbot AI sudah ada, tetapi belum ada "buat situs dari deskripsi usaha" (Wix AI site generator [W9], Squarespace Blueprint [Q4], Framer Wireframer [F4]) atau "tulis ulang teks/SEO dengan AI" (Shopify Magic di theme editor [S6], Canva Magic SEO [C3]). Karena output tetap JSON tertutup, ini aman secara arsitektur.
15. **Ekosistem plugin belum punya kontrak**: belum ada versi plugin, skema pengaturan per plugin, batas jumlah per halaman/paket, `enabled_on` per jenis halaman, dan konsep *embed* (plugin tanpa kotak grid seperti chatbot melayang). Shopify app blocks/app embed memberi model rujukan yang matang (target `head`/`body`, `enabled_on`, `available_if`, batas 30 blok) [S1][S2].

---

## 2. Metode & sumber

**Metode.** (1) Membaca ADR-0007/0014/0017 dan kode prototipe (`frontend/src/studio/*`, `frontend/src/renderer/*`, `frontend/src/app/(studio)/studio/page.tsx`) untuk memetakan kondisi saat ini. (2) Menelusuri dokumentasi resmi/help center/developer docs tiap pembanding; bila tidak ada sumber primer, memakai sumber sekunder dan menandainya. (3) Memetakan standar wajib dari W3C, Google Search Central, web.dev, dan teks UU. (4) Menyusun matriks per area, lalu daftar celah dengan prioritas & dampak arsitektur.

**Keterbatasan.** Riset dilakukan dengan penelusuran web; beberapa halaman help center tidak dapat dibuka penuh (mis. faq.whatsapp.com, artikel resmi BukaOlshop/Ginee tidak ditemukan). Pembanding diuji lewat dokumentasi, **bukan** uji coba produk langsung. Fitur vendor berubah cepat (Wix meluncurkan *Harmony Editor* Jan 2026 [W2]; Framer memperbarui CMS Juni 2026 [F3]).

| Pembanding | Sumber utama | Tingkat bukti | Diakses |
|---|---|---|---|
| Canva Websites | canva.com/help | Primer | 2026-11-13 |
| Wix Editor / Studio / Harmony | support.wix.com, dev.wix.com | Primer | 2026-11-13 |
| Squarespace (Fluid Engine) | support.squarespace.com, engineering blog | Primer + sekunder | 2026-11-13 |
| Shopify Online Store 2.0 | shopify.dev, help.shopify.com | Primer | 2026-11-13 |
| Framer | framer.com/help, academy | Primer (sebagian marketing) | 2026-11-13 |
| Webflow | help.webflow.com, developers.webflow.com | Primer | 2026-11-13 |
| WordPress Gutenberg / FSE | wordpress.org/documentation, developer.wordpress.org | Primer | 2026-11-13 |
| Carrd | carrd.co/docs | Primer | 2026-11-13 |
| Linktree | help center (intercom) | Primer (terbatas) | 2026-11-13 |
| Tokopedia (Dekorasi Toko) | tokopedia.com/help | Primer | 2026-11-13 |
| Shopee (Dekorasi Toko) | shopee.co.id inspirasi + BigSeller | Primer terbatas + sekunder | 2026-11-13 |
| Moka / GoBiz | berita & blog | Sekunder | 2026-11-13 |
| BukaOlshop | — | **belum terverifikasi** (tidak ditemukan sumber resmi) | 2026-11-13 |
| Ginee | ginee.com, berita | Sekunder; fitur *webstore* **belum terverifikasi** | 2026-11-13 |
| WCAG 2.2, ATAG 2.0 | w3.org | Primer | 2026-11-13 |
| Core Web Vitals | web.dev | Primer | 2026-11-13 |
| SEO | developers.google.com/search, ogp.me | Primer | 2026-11-13 |
| UU PDP No. 27/2022 | peraturan.bpk.go.id | Primer | 2026-11-13 |

---

## 3. Matriks fitur per area

Kolom "Studio sekarang": **Ada** / **Sebagian** / **Belum**.

### 3.1 Onboarding & template

| Fitur | Studio | Pembanding (sitasi) | Catatan |
|---|---|---|---|
| Galeri template | Ada (6) | Canva [C1], Squarespace [Q4], Shopify tema [S4] | Perlu template per kategori UMKM (kuliner, fashion, jasa, kerajinan, kelontong) |
| Template terisi data toko | Ada | Squarespace Blueprint [Q4] | Sudah setara konsep |
| Generator situs AI dari tanya-jawab | Belum | Wix AI site [W9], Squarespace Blueprint [Q4], Framer Wireframer [F4] | Output JSON tertutup → aman |
| Ganti template tanpa kehilangan konten | Belum terverifikasi di prototipe | Squarespace Themes/site styles [Q3] | Perlu aturan pemetaan blok |
| Checklist "siap terbit" | Belum | Canva SEO guide [C5] (saran visibilitas) | Lihat G-PUB-05 |

### 3.2 Struktur halaman, multi-halaman & navigasi

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Banyak halaman | Belum | Canva ≤45 halaman [C4]; Shopify template per halaman [S4] | Renderer sudah menaut `/produk/{slug}` |
| Menu navigasi otomatis dari halaman | Belum | Canva [C4] | Header di luar grid sudah ada → titik pasang menu |
| Header/footer global (sinkron) | Sebagian (di luar grid, satu halaman) | Shopify section groups [S3]; WordPress template parts [P1] | Saat multi-halaman wajib global |
| Blok sinkron/dipakai ulang | Belum | WordPress synced patterns [P1][P2]; Webflow components [WF4] | P2 |
| Halaman 404, kebijakan privasi | Belum | Shopify template halaman [S4] | Kebijakan privasi diperlukan untuk UU PDP |
| Duplikasi halaman | Belum | Wix klik kanan/duplikat [W4] | |

### 3.3 Sistem layout

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Grid snap | Ada (12 kolom) | Squarespace Fluid Engine grid, tekan G untuk menampilkan [Q1] | |
| Tampilkan/sembunyikan garis grid | Belum terverifikasi | Squarespace (G) [Q1] | Murah |
| Layout ponsel terpisah | Belum (tumpuk otomatis) | Squarespace mobile layout terpisah [Q2]; Wix mobile editor [W1] | ADR-0017 sengaja menolak; perlu kompromi "urutan ponsel" |
| Sembunyikan per perangkat | Belum | Wix *Hidden on Mobile* [W1][W2]; Shopify sembunyikan section/blok (ikon mata) [S4] | |
| Spasi/padding per blok, jarak antarbaris | Belum | Squarespace site styles [Q3] | Enum ringan |
| Alignment guides / smart guides | Tidak relevan (grid) | Wix Smart Guide [W3] | Grid sudah menggantikan |
| Layers / daftar blok terstruktur | Sebagian (daftar terpasang) | Wix Layers panel [W3b]; WordPress List View [P3] | Perlu urutan baca/fokus |
| Grouping | Belum | Wix grouping [W3c] | P2; grid membuat ini kurang penting |
| Z-order / tumpuk | Tidak relevan | Wix [W3b] | Sengaja tidak (tanpa tumpang tindih) |
| Latar per section/baris (warna/foto) | Sebagian (gaya per plugin) | Squarespace section theme [Q3] | |

### 3.4 Tipografi & brand kit

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Palet + warna bebas | Ada | Squarespace colors [Q3]; Canva Brand Kit [C6] | Perlu cek kontras otomatis (WCAG 1.4.3) |
| Preset font | Ada (5) | WordPress global styles [P4]; Squarespace fonts [Q3] | Font harus self-host (CSP) |
| Ukuran teks global | Ada (S/M/L) | Squarespace [Q3] | |
| Logo, favicon, gambar berbagi | Sebagian (logo profil) | Canva favicon [C3] | |
| Brand kit lintas fitur (dipakai di poster/promo) | Belum | Canva Brand Kit + Magic Design [C6][C7] | P2 pembeda |
| Style book / pratinjau semua blok | Belum | WordPress Style Book [P4] | P2 |

### 3.5 Media

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Unggah & kompres di browser | Ada | — | |
| Seret foto ke plugin | Ada | — | |
| Pustaka media terpusat | Belum | Webflow Assets panel [WF3]; Shopee Galeri Media di Dekorasi Toko [T3 (sekunder)] | |
| Alt text | Belum (renderer `alt=""`) | Webflow alt di aset + opsi dekoratif [WF3]; Framer [F5] | WCAG 1.1.1 [A1] |
| Crop / focal point | Belum | Belum terverifikasi untuk Framer; umum di builder | Penting untuk banner di ponsel |
| Gambar responsif (`srcset`, WebP/AVIF) | Belum terverifikasi | web.dev [V3] | Dampak LCP |
| Video (YouTube/TikTok embed) | Belum | Umum; Carrd embed hanya Pro [K1] | Butuh CSP `frame-src` whitelist |
| Ikon | Belum terverifikasi | — | Set ikon resmi |
| Foto stok | Belum | Canva (pustaka bawaan) — belum terverifikasi tautan spesifik | Lisensi |

### 3.6 Komponen/plugin umum

| Plugin | Studio | Pembanding | Catatan |
|---|---|---|---|
| Formulir kontak/pesan | Belum | Carrd Contact (Pro Standard) & Custom form (Pro Plus) [K2][K3] | UU PDP |
| FAQ (akordeon) | Belum | Umum (WordPress core Details block) — belum terverifikasi tautan | Bisa jadi sumber chatbot |
| Countdown promo | Belum | Umum di Shopify apps — belum terverifikasi | |
| Peta | Sebagian (Lokasi: alamat & petunjuk arah) | — | Embed peta perlu `frame-src`; alternatif tautan Maps |
| Embed sosial (IG/TikTok) | Belum | — | CSP |
| Tombol WA melayang | Sebagian (Kontak) | — | WhatsApp-first |
| Tautan marketplace (Tokopedia/Shopee/TikTok Shop) | Belum | Linktree Shops [L3 (sekunder)] | Sangat relevan |
| Bilah pengumuman / pop-up | Belum | Squarespace (sekunder) [Q5] | |
| Menu restoran (kategori, harga, varian) | Sebagian (galeri produk) | Google LocalBusiness/Restaurant [G4] | |
| Booking/reservasi | Belum | Wix Bookings → halaman "Book Online" [W8] | Untuk jasa (salon, laundry) |
| Kupon/kode promo | Sebagian (Promo) | — | Terkait checkout |
| Ulasan terverifikasi | Sebagian (Testimoni manual) | Google Product review snippet [G5] | Jangan markup ulasan buatan sendiri |
| Link-in-bio / halaman tautan | Belum | Linktree [L2], Canva Link in Bio [C2] | Sangat dipakai UMKM IG/TikTok |
| QR code toko | Belum | Linktree QR (sekunder) [L3] | Cetak di meja/kemasan |
| Produk pilihan / etalase | Sebagian | Shopee Produk Pilihan Toko [T2]; Tokopedia Dekorasi Toko [T1] | |

### 3.7 E-commerce di builder

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Halaman detail produk | Sebagian (tautan ada di renderer) | Shopify product template [S4] | Perlu template detail yang dapat diatur |
| Keranjang & checkout QRIS manual | Di luar Studio (ADR-0004) | Shopify; Moka/GoStore (sekunder) [M2] | Studio perlu blok "Beli"/"Pesan" |
| Kategori/etalase | Belum terverifikasi | Shopee Kategori Toko (sekunder) [T3] | |
| Stok habis/label | Belum terverifikasi | — | |
| Product structured data | Belum | Google Product [G5] | |

### 3.8 Ergonomi editor

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Undo/Redo | Ada | Shopify toolbar (sekunder) | |
| Pintasan keyboard + daftar pintasan | Sebagian | WordPress Shift+Alt+H [P3] | Tambah layar bantuan |
| Salin/tempel & duplikat blok | Belum terverifikasi | Wix copy/paste/duplicate [W4] | |
| Salin gaya | Belum | Belum terverifikasi tautan primer | P2 |
| Klik kanan / menu konteks | Belum | Wix right-click menu [W4] | |
| Zoom kanvas | Belum | — | |
| Cari plugin | Ada | — | |
| Pratinjau ponsel | Ada | Canva "Resize on mobile" [C4] | |
| Pratinjau di HP sungguhan (QR/tautan) | Belum | Framer staging URL [F3] | |

### 3.9 Mobile editing

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Edit dari HP | Belum | Wix Owner app (Bookings dari app) [W8b]; Shopee/Tokopedia dekorasi di app — belum terverifikasi | Pemilik UMKM dominan HP |

### 3.10 Kolaborasi & peran

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Peran editor terpisah | Belum di Studio (platform punya peran) | Framer izin *Deploy* [F3]; Webflow publishing [WF2] | |
| Kunci edit bersamaan | Belum | — | Mencegah tabrakan antar staf |
| Komentar | Belum | — | P2 |

### 3.11 Versi, draf, jadwal, pratinjau

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Draf vs terbit | Belum | Shopify tema draf/publish; Framer staging [F3]; Webflow staging [WF2] | P0 |
| Riwayat versi & pulihkan | Belum (lokal saja) | Wix Site History [W5][W6]; Webflow backups [WF1] | P0 |
| Tautan pratinjau | Belum | Framer staging URL [F3] | |
| Jadwal terbit | Belum | Webflow item terjadwal [WF1]; Linktree scheduling [L2] | P1 |

### 3.12 Publikasi & domain

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Subdomain platform | Ada di platform (ADR-0001) | Canva domain gratis [C2] | |
| Domain sendiri | Belum (ADR-0008 menunda) | Canva Pro (≤5 domain) [C2] | Pembeda paket Pro |
| Proteksi kata sandi | Belum | Canva [C3] | Untuk "segera hadir" |
| Mode "segera hadir"/maintenance | Belum | — | |
| Tanda platform di footer | Belum terverifikasi | Canva footer "Designed with Canva" di Free [C2] | Model paket Starter |

### 3.13 SEO

| Fitur | Studio | Pembanding/standar | Catatan |
|---|---|---|---|
| Title & meta description per halaman | Belum | Google [G1][G2]; Canva [C3] | P0 |
| Open Graph (pratinjau WA/FB) | Belum | ogp.me [G7] | P0 untuk WA |
| Sitemap & robots | Belum | Google [G3][G9] | |
| Kanonik | Belum | Google [G8] | Subdomain + custom domain nanti |
| LocalBusiness JSON-LD | Belum | Google [G4] | Data jam buka/lokasi sudah ada |
| Product JSON-LD | Belum | Google [G5] | |
| Toggle "sembunyikan dari mesin pencari" | Belum | Canva [C5] | |
| Slug halaman | Belum | Canva URL per halaman [C3] | |
| AI SEO | Belum | Canva Magic SEO [C3]; Shopify Magic [S6] | |

### 3.14 Performa

| Fitur | Studio | Standar | Catatan |
|---|---|---|---|
| Target CWV (LCP ≤2,5 s, INP ≤200 ms, CLS ≤0,1 p75) | Belum diukur | web.dev [V1] | |
| Gambar responsif & lazy load | Sebagian (`loading="lazy"` di semua gambar, termasuk banner) | web.dev [V2][V3] | Banner (LCP) jangan lazy |
| Font self-host + `font-display` | Belum terverifikasi | web.dev [V4] | |
| Dimensi gambar untuk cegah CLS | Belum terverifikasi | web.dev [V5] | |

### 3.15 Aksesibilitas

| Fitur | Studio | Standar | Catatan |
|---|---|---|---|
| Alt text konten | Belum | WCAG 1.1.1 [A1] | |
| Cek kontras warna bebas | Belum | WCAG 1.4.3 (4,5:1) [A1] | |
| Struktur heading tunggal h1 | Sebagian (Banner = h1; bisa >1 Banner) | WCAG 1.3.1 [A1] | |
| Alternatif seret tanpa seret | Sebagian (keyboard saja) | WCAG 2.5.7 [A1] | |
| Target ≥24 px | Belum terverifikasi | WCAG 2.5.8 [A1] | |
| Fokus tak tertutup panel/chatbot melayang | Belum terverifikasi | WCAG 2.4.11 [A1] | Chatbot gelembung bisa menutupi fokus |
| Editor mendorong konten aksesibel | Belum | ATAG 2.0 B.2.1 [A2] | |
| Bahasa halaman (`lang="id"`) | Belum terverifikasi di renderer | WCAG 3.1.1 [A1] | |

### 3.16 Analitik

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Kunjungan, sumber, klik WA/produk | Belum | Canva + GA [C3]; Linktree analitik (sekunder) [L3] | First-party |
| Banner cookie | Belum | Canva [C3] | Hindari dengan analitik tanpa cookie |

### 3.17 i18n / multi-bahasa

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Situs dua bahasa (ID/EN) | Belum | Wix Multilingual (manual/otomatis) [W7][W7b]; Webflow localization [WF5] | P2 (turis/oleh-oleh) |
| `hreflang` | Belum | Google [G10] | |

### 3.18 Keamanan & moderasi

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| JSON tertutup + validator | Ada (rancangan) | Shopify validasi saat deploy [S1] | Validator v2 belum ada |
| Moderasi konten (judi, obat, penipuan) | Belum | — | Risiko hukum platform (UU ITE) — belum dikaji |
| Pelaporan situs oleh publik | Belum | — | |
| Anti-spam formulir | Belum | — | Saat form ada |

### 3.19 Ekosistem plugin

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Katalog + pasang | Ada | Shopify app blocks [S1][S7] | |
| Skema pengaturan per plugin | Sebagian (layout, style) | Shopify `settings` di schema blok [S1] | |
| Batasi plugin per jenis halaman | Belum | Shopify `enabled_on`/`disabled_on` [S1] | |
| Plugin "embed" (tanpa kotak grid) | Sebagian (chatbot floating) | Shopify app embed `head`/`body`, nonaktif bawaan [S1] | Pengumuman, WA melayang, pop-up |
| Kondisi tampil (fitur aktif/paket) | Belum | Shopify `available_if` [S1] | Starter vs Pro |
| Batas jumlah blok | Sebagian (ADR 0–40) | Shopify 30 blok/extension [S1] | |
| Versi plugin & migrasi data | Belum | Shopify deploy tervalidasi [S1] | |
| Harga per plugin | Belum | Wix Bookings gratis dipasang, berbayar untuk menerima booking [W8] | |
| Pedoman UX plugin | Belum | Shopify UX guidelines [S2] | |

### 3.20 AI di builder

| Fitur | Studio | Pembanding | Catatan |
|---|---|---|---|
| Chatbot AI untuk pengunjung | Ada | — | Pembeda |
| Generate situs dari prompt | Belum | Wix [W9], Squarespace [Q4], Framer [F4] | |
| Tulis/ubah teks blok dengan AI | Belum | Shopify Magic theme editor [S6]; Canva Magic Write [C6] | |
| AI SEO (judul/deskripsi) | Belum | Canva Magic SEO [C3] | |
| AI alt text | Belum | Plugin Framer (pihak ketiga) [F5] | |
| Brand dari URL/IG | Belum | Canva Brand Intelligence [C6] | |

---

## 4. Daftar celah lengkap

Format tiap butir: **ID — Judul**. Deskripsi · *Kenapa penting* · **Prioritas** (P0 wajib sebelum rilis publik / P1 segera setelah rilis / P2 pembeda) · **Kompleksitas** (S/M/L) · **Dampak arsitektur**.

### 4.1 Penyimpanan, publikasi, versi

- **G-PUB-01 — Simpan draf ke server.** Ganti localStorage dengan endpoint `PUT /website/draft` (optimistic concurrency via `version`/ETag). · Kerja hilang bila ganti perangkat/bersihkan browser; staf lain tidak melihat draf. · **P0** · M · `schema_version = 2` + validator Rust (bounds, overlap, whitelist kind, `deny_unknown_fields`); migrasi v1→v2 (ADR-0017).
- **G-PUB-02 — Terbit (publish) terpisah dari draf.** Tombol "Terbitkan" menyalin draf tervalidasi ke versi live; situs publik hanya membaca live. Pembanding: tema draf vs live Shopify [S4], staging Framer [F3]/Webflow [WF2]. · Pemilik bisa bereksperimen tanpa merusak toko yang sedang berjualan. · **P0** · M · Tabel versi; renderer publik dua versi selama transisi (ADR-0017).
- **G-PUB-03 — Riwayat versi terbit + pulihkan.** Simpan mis. 20 versi terbit terakhir dengan waktu & pengubah; "Pulihkan" membuat draf dari versi lama. Wix [W5][W6], Webflow (titik pulih otomatis, simpan versi saat ini sebelum restore) [WF1]. · Pengaman utama untuk pengguna awam. · **P0** · S–M · Retensi perlu masuk kebijakan ADR-0012; rollback lintas skema (open point #4 ADR-0017).
- **G-PUB-04 — Tautan pratinjau draf.** URL bertoken kedaluwarsa untuk dibuka di HP sendiri / dikirim ke rekan via WA. Framer staging URL [F3]. · Pemilik ingin melihat di HP Android sungguhan sebelum terbit. · **P1** · S · Token sekali pakai; `noindex`; CSP sama dengan publik.
- **G-PUB-05 — Pemeriksa "siap terbit".** Daftar periksa otomatis: alt text kosong, kontras rendah, judul SEO kosong, nomor WA tidak valid, blok terpotong, gambar terlalu besar. · Menerjemahkan standar (WCAG/SEO/CWV) jadi tindakan sederhana; ATAG B.2 [A2]. · **P0** (versi minimal) · M · Aturan dibagi frontend & validator Rust (peringatan vs. tolak).
- **G-PUB-06 — Jadwal terbit/jadwal tampil blok.** `visible_from`/`visible_until` per blok (promo Ramadan, Harbolnas 11.11/12.12). Linktree [L2]; Webflow item terjadwal [WF1]. · Promo musiman adalah pola utama UMKM. · **P1** · M · Karena static export, butuh rebuild terjadwal atau penyaringan waktu di klien (tanpa inline script—JS bundel biasa boleh); field baru di skema.
- **G-PUB-07 — Mode "segera hadir"/kata sandi situs.** Canva password protect [C3]. · Toko baru belum siap tapi ingin klaim alamat. · **P2** · S · Caddy basic-auth atau halaman statis; `noindex`.
- **G-PUB-08 — Kunci edit bersamaan / deteksi konflik.** Peringatan "Diedit oleh X 2 menit lalu" + penolakan simpan bila versi basi. · Toko dengan admin+staf. · **P1** · S · ETag/versi pada endpoint draf.
- **G-PUB-09 — Log aktivitas Studio.** Siapa menerbitkan/memulihkan apa. Webflow Site Activity [WF2]. · Audit & sengketa staf. · **P1** · S · Pakai audit log platform yang ada.

### 4.2 Struktur & navigasi

- **G-NAV-01 — Multi-halaman.** Halaman: Beranda, Katalog, Tentang, Kontak, Kebijakan, + halaman bebas (maks. N per paket). Canva ≤45 [C4]. · SEO per topik, struktur toko yang dikenal pembeli. · **P1** · L · Skema `pages[]` dengan `slug` whitelist regex; static export perlu rute dinamis per toko (cek ADR-0014); batas halaman per paket (ADR-0016).
- **G-NAV-02 — Menu navigasi otomatis + manual.** Dari judul halaman (Canva [C4]) dengan urutan & label dapat diubah; versi ponsel (hamburger) aksesibel. · **P1** · M · Header v1 diperluas; tautan internal divalidasi (hanya halaman sendiri/whitelist).
- **G-NAV-03 — Template halaman produk/detail.** Atur blok di halaman detail produk (galeri, harga, varian, tombol WA/pesan, ulasan). Shopify product template [S4]. · Halaman detail adalah halaman yang dibagikan ke WA. · **P1** · M · Jenis halaman `product` dengan blok yang dibatasi (`enabled_on`).
- **G-NAV-04 — Halaman kebijakan bawaan.** Privasi (UU PDP), pengembalian, syarat pemesanan—template teks yang bisa diedit. · Wajib bila ada formulir/pesanan; meningkatkan kepercayaan. · **P0** (bila form/checkout aktif) · S · Jenis halaman `policy` teks polos/markdown terbatas.
- **G-NAV-05 — Halaman 404 bergaya toko.** · **P2** · S · Static export + Caddy `handle_errors`.
- **G-NAV-06 — Blok/section global sinkron.** Satu blok (mis. "Info pengiriman") dipakai di banyak halaman; ubah sekali. WordPress synced patterns [P1][P2]; Shopify section groups [S3]. · **P2** · M · Referensi `ref_id` di skema; validator cegah siklus.
- **G-NAV-07 — Duplikasi halaman & blok.** Wix [W4]. · **P1** · S · Hanya frontend.
- **G-NAV-08 — Anchor/lompat ke bagian.** Tautan tombol ke blok di halaman (`#promo`). · Situs satu halaman tetap mudah dinavigasi. · **P1** · S · `anchor_id` slug whitelist.

### 4.3 Layout & perangkat

- **G-LAY-01 — Urutan ponsel dapat diatur.** Tetap satu kolom, tetapi pemilik bisa mengubah urutan tumpukan ponsel (bukan layout terpisah penuh). Squarespace/Wix punya layout ponsel terpisah [Q2][W1]. · Tumpukan y-lalu-x sering menaruh blok penting (WA, jam buka) terlalu bawah. · **P1** · M · Field `mobile_order` opsional; tetap sejalan dengan penolakan "dua layout" di ADR-0017.
- **G-LAY-02 — Sembunyikan per perangkat.** `hide_on: "mobile"|"desktop"`. Wix [W1][W2], Shopify sembunyikan section [S4]. · Galeri besar sering ingin disembunyikan di ponsel (hemat kuota). · **P1** · S · Enum di skema; render CSS `display:none` + idealnya tidak dirender sama sekali agar gambar tidak dimuat.
- **G-LAY-03 — Pratinjau tablet / lebar menengah.** Squarespace punya masalah spasi tablet 600–767 px (sekunder) [Q6]. · Banyak tablet Android murah. · **P1** · S · Hanya frontend.
- **G-LAY-04 — Spasi & padding per blok, celah antarbaris.** Enum `spacing: none|s|m|l`. · Tampilan "padat" vs "lega" adalah permintaan desain dasar. · **P1** · S · Enum skema.
- **G-LAY-05 — Latar baris/section penuh (full-bleed) dengan foto/warna.** Squarespace section theme [Q3]. · Banner dan promo butuh latar penuh lebar layar. · **P1** · M · Konsep "baris" atau `full_bleed: bool` pada blok w=12; aset whitelist.
- **G-LAY-06 — Perataan vertikal isi dalam kotak.** `align: top|center|bottom`. · Kotak berdampingan dengan tinggi berbeda terlihat rapi. · **P2** · S · Enum.
- **G-LAY-07 — Tampilkan grid & penanda kolom.** Squarespace tombol G [Q1]. · **P2** · S · Frontend.
- **G-LAY-08 — Panel struktur (layers/outline) dengan urutan baca.** Wix Layers [W3b]; WordPress List View [P3]. Memperlihatkan urutan baca yang dipakai pembaca layar & ponsel. · **P1** · S · Frontend.
- **G-LAY-09 — Grouping.** Wix [W3c]. · Kurang penting di grid; **P2** · M.

### 4.4 Tipografi, warna, brand

- **G-BRD-01 — Pemeriksa kontras otomatis.** Saat memilih warna bebas, hitung rasio vs latar; tolak/peringatkan <4,5:1 untuk teks normal (WCAG 1.4.3) [A1]; tawarkan warna terdekat yang lolos. · Warna bebas + latar gelap rawan teks tak terbaca di layar HP murah dan terik matahari. · **P0** · S · Validator Rust bisa menolak kombinasi ekstrem (opsional).
- **G-BRD-02 — Favicon & ikon situs, gambar berbagi default.** Canva favicon [C3]. · **P0** · S · Aset whitelist; meta di renderer.
- **G-BRD-03 — Brand kit toko lintas fitur.** Logo, warna, font tersimpan sekali dan dipakai di Studio, poster promo, katalog PDF, struk. Canva Brand Kit [C6]. · **P2** · M · Entitas brand di profil toko.
- **G-BRD-04 — Lebih banyak font + pasangan judul/isi.** WordPress global styles [P4]. · **P2** · S · Wajib self-host (CSP `font-src 'self'`), subset Latin untuk ukuran kecil.
- **G-BRD-05 — Gaya tombol global (bentuk, sudut).** Squarespace site styles (tombol) [Q3]. · **P2** · S · Enum.
- **G-BRD-06 — Ekstrak warna dari logo.** Mirip Canva Brand Intelligence [C6]. · **P2** · S · Klien saja.

### 4.5 Media

- **G-MED-01 — Alt text di aset + override per penggunaan + opsi "dekoratif".** Webflow [WF3]; Framer [F5]. Renderer saat ini menulis `alt=""` untuk Banner/Tentang/Promo/produk. · WCAG 1.1.1 [A1]; juga SEO gambar [G6]. · **P0** · S–M · Field `alt` (teks polos, ≤150 karakter) di aset & referensi gambar; validator; default alt = nama produk untuk foto produk.
- **G-MED-02 — Pustaka media.** Grid semua foto toko, cari, pakai ulang, hapus aman (cek pemakaian). Webflow Assets [WF3]; Shopee Galeri Media (sekunder) [T3]. · Mengurangi unggah ulang di koneksi lambat. · **P1** · M · ADR-0009 aset sudah ada; perlu indeks pemakaian.
- **G-MED-03 — Crop & focal point.** Titik fokus disimpan (`fx, fy` 0–1) agar banner tidak memotong wajah/produk di ponsel. Fitur Framer belum terverifikasi. · **P1** · M · Field angka terbatas; CSS `object-position`.
- **G-MED-04 — Varian ukuran gambar (srcset) & format modern.** Server membuat 3–4 lebar (mis. 480/960/1600) WebP/AVIF. web.dev [V3]. · LCP di 3G/4G lemah. · **P0** · M · Pipeline aset Rust (resize); renderer `srcset/sizes`.
- **G-MED-05 — Banner tidak `loading="lazy"` + `fetchpriority="high"`.** Saat ini semua gambar lazy. web.dev LCP [V2]. · **P0** · S · Renderer saja.
- **G-MED-06 — Video embed (YouTube) dengan fasad ringan.** Tampilkan thumbnail; iframe dimuat saat diklik (`youtube-nocookie`). · Konten demo produk; TikTok/Reels populer. · **P1** · M · CSP `frame-src` whitelist domain; ADR baru (perubahan CSP ADR-0014).
- **G-MED-07 — Set ikon resmi.** Untuk fitur/keunggulan, kontak. · **P2** · S · SVG bundel, bukan unggahan SVG (SVG unggahan = risiko XSS).
- **G-MED-08 — Foto stok/ilustrasi bebas lisensi untuk template.** · Toko baru belum punya foto. · **P2** · M · Lisensi; disimpan sebagai aset platform.
- **G-MED-09 — Hapus latar foto produk (AI).** · Foto produk UMKM sering berlatar berantakan. · **P2** · M · Proses server (AI platform), hasil = aset biasa.

### 4.6 Plugin/komponen baru

- **G-PLG-01 — Formulir (kontak, pre-order, RSVP, minta penawaran).** Field terbatas (nama, WA, teks, pilihan, tanggal, jumlah); kiriman masuk dasbor + notifikasi; opsi "lanjutkan ke WA". Carrd [K2][K3]. · Menangkap prospek saat toko offline. · **P1** · L · Endpoint publik baru, rate limit, anti-spam (honeypot/turnstile—perhatikan CSP), consent & retensi UU PDP [L1], ADR baru.
- **G-PLG-02 — WhatsApp: tombol melayang + "Pesan via WA" per produk** dengan template pesan berisi nama produk/harga/tautan (`wa.me/<62…>?text=`; sumber format sekunder [M1]). · Saluran transaksi utama UMKM. · **P0** · S · Plugin *embed* (lihat G-ECO-03); validasi nomor E.164 Indonesia di Rust.
- **G-PLG-03 — Tautan marketplace.** Tombol bermerek Tokopedia/Shopee/TikTok Shop/GoFood/GrabFood, per toko & per produk. · Banyak UMKM menjual di marketplace; website = etalase + kepercayaan. · **P1** · S · Whitelist domain URL di validator.
- **G-PLG-04 — Bilah pengumuman & pop-up promo** (maks. satu pop-up aktif, bisa ditutup, tidak menutup konten di ponsel). Squarespace (sekunder) [Q5]. · **P1** · M · Plugin *embed*; WCAG 2.4.11 & 2.2.x (dapat ditutup) [A1].
- **G-PLG-05 — FAQ akordeon.** Juga dipakai sebagai basis pengetahuan chatbot. · **P1** · S · `<details>` native tanpa JS; data teks polos.
- **G-PLG-06 — Countdown promo.** · **P2** · S · JS bundel (bukan inline); hormati `prefers-reduced-motion`.
- **G-PLG-07 — Menu restoran.** Kategori, item, harga, label (pedas/halal/terlaris), foto opsional, jam layanan. Google Restaurant/LocalBusiness `hasMenu` [G4]. · Segmen kuliner terbesar. · **P1** · M · Bisa varian layout "menu" dari produk.
- **G-PLG-08 — Booking/reservasi jasa.** Slot waktu, konfirmasi via WA, tanpa pembayaran online (atau DP QRIS manual). Wix Bookings menambah halaman "Book Online" [W8]. · Salon, laundry, servis, kursus. · **P2** · L · Domain baru (jadwal); ADR baru.
- **G-PLG-09 — Link-in-bio (halaman tautan).** Halaman ringkas ala Linktree/Canva Link in Bio [L2][C2] dengan tautan terjadwal. · Bio Instagram/TikTok adalah pintu masuk utama. · **P1** · S–M · Jenis halaman khusus dengan blok `link_list`.
- **G-PLG-10 — QR code toko/produk** untuk dicetak (meja, kemasan, banner). Linktree (sekunder) [L3]. · Jembatan offline→online, cocok dengan QRIS di kasir. · **P1** · S · Dibuat di klien/server sebagai gambar unduhan.
- **G-PLG-11 — Peta tersemat.** Saat ini Lokasi = alamat + petunjuk arah. Embed peta interaktif menambah beban & CSP. · **P2** · M · Rekomendasi: gambar peta statis + tautan Google Maps, bukan iframe.
- **G-PLG-12 — Embed sosial (IG/TikTok).** · **P2** · M · Script pihak ketiga → bertentangan dengan CSP ketat; alternatif: "kartu tautan" + thumbnail yang diambil server.
- **G-PLG-13 — Ulasan pelanggan terverifikasi** (dari pesanan selesai) vs. testimoni manual. Google: jangan markup ulasan yang ditulis sendiri sebagai rating [G5]. · **P2** · M · Butuh data pesanan; moderasi.
- **G-PLG-14 — Kupon/kode promo tampil + salin.** Terhubung ke checkout. · **P2** · M · Domain promo di backend.
- **G-PLG-15 — Pengumuman "Buka/Tutup sekarang" & libur.** Jam buka sudah ada; tambahkan pengecualian (libur Lebaran) dan status realtime. · **P1** · S · Field tanggal libur; status dihitung di klien (zona WIB/WITA/WIT).
- **G-PLG-16 — Info pengiriman & pembayaran (logo QRIS/bank/COD).** · Kepercayaan pembeli. · **P1** · S · Blok statis.
- **G-PLG-17 — Teks bebas & pemisah/spacer.** Blok teks sederhana (judul + paragraf, tebal/miring/tautan terbatas) dan pemisah. · Kebutuhan paling dasar yang belum ada di daftar plugin. · **P0** · S · Rich text terbatas = struktur JSON (bukan HTML), whitelist mark.
- **G-PLG-18 — Tombol/ajakan bertindak mandiri.** · **P1** · S · URL whitelist.

### 4.7 E-commerce di Studio

- **G-COM-01 — Blok "Beli"/"Pesan" yang terhubung checkout QRIS manual** (ADR-0004) atau WA. · **P0** bila website dijual sebagai toko online · M · Kontrak dengan modul pesanan.
- **G-COM-02 — Kategori/etalase & filter sederhana** di galeri produk. Shopee Kategori Toko (sekunder) [T3]. · **P1** · M · Query data produk saat build.
- **G-COM-03 — Produk pilihan/unggulan & label (Baru, Terlaris, Habis).** Shopee Produk Pilihan [T2]. · **P1** · S.
- **G-COM-04 — Sinkronisasi harga/stok setelah terbit.** Static export → perubahan harga butuh rebuild otomatis atau fetch data saat runtime. · Harga basi = komplain. · **P0** · M · Keputusan arsitektur: rebuild on change vs. JSON produk publik ber-cache; ADR.
- **G-COM-05 — Varian produk (ukuran/warna) di blok produk.** · Fashion. · **P1** · M · Tergantung model produk backend.

### 4.8 Ergonomi editor

- **G-EDT-01 — Salin/tempel/duplikat blok (Ctrl+C/V/D) & menu konteks.** Wix [W4]. · **P1** · S.
- **G-EDT-02 — Layar bantuan pintasan.** WordPress Shift+Alt+H [P3]. · **P1** · S.
- **G-EDT-03 — Salin gaya antar blok.** · **P2** · S.
- **G-EDT-04 — Zoom kanvas / "muat ke layar".** · Laptop 1366×768 umum di Indonesia. · **P2** · S.
- **G-EDT-05 — Status simpan yang jelas** ("Tersimpan di server 10.42", "Offline—akan disinkronkan"). · Koneksi putus-sambung. · **P0** (bersama G-PUB-01) · S.
- **G-EDT-06 — Edit offline & antrean sinkron.** · Koneksi lambat. · **P2** · M · Konflik versi.
- **G-EDT-07 — Pencarian di dalam halaman/konten.** · **P2** · S.
- **G-EDT-08 — Tur/onboarding interaktif singkat** (3–5 langkah) + bantuan kontekstual. WCAG 3.2.6 (bantuan konsisten) relevan untuk situs publik [A1]. · Pengguna pertama kali. · **P1** · S.
- **G-EDT-09 — Pratinjau "seperti pengunjung" penuh** (tanpa kontrol editor, dengan header/footer, chatbot aktif). · **P0** · S.

### 4.9 Mobile editing

- **G-MOB-01 — Mode edit di HP (versi sederhana).** Bukan kanvas grid; daftar blok vertikal: ubah teks, ganti foto, urutkan (tombol naik/turun), sembunyikan, terbitkan. Wix Owner app [W8b]; Tokopedia/Shopee dekorasi via app — belum terverifikasi. · Mayoritas pemilik UMKM mengelola dari HP Android. · **P1** · L · Memakai skema yang sama; tidak butuh drag.
- **G-MOB-02 — Ganti foto langsung dari kamera.** `<input capture>`. · **P1** · S.

### 4.10 Kolaborasi & peran

- **G-COL-01 — Izin Studio per peran:** edit draf vs. terbitkan. Framer izin Deploy [F3]. · Staf boleh mengubah promo tetapi pemilik yang menerbitkan. · **P1** · S · Peran platform yang ada + izin baru.
- **G-COL-02 — Komentar/catatan di blok.** · **P2** · M.

### 4.11 Publikasi & domain

- **G-DOM-01 — Domain sendiri (Pro).** Canva: domain sendiri hanya paket berbayar, maks. 5 [C2]. · Kredibilitas; pembeda paket. · **P2** (sesuai ADR-0008) · L · Verifikasi DNS, TLS on-demand Caddy, anti-takeover (R-04); ADR baru menggantikan ADR-0008.
- **G-DOM-02 — Atribusi platform di footer Starter.** Canva Free footer wajib [C2]. · Model akuisisi & pembeda paket. · **P1** · S · Bendera paket di renderer.
- **G-DOM-03 — Slug toko & ganti alamat dengan redirect.** · **P1** · M · Caddy redirect; mencegah takeover slug lama.

### 4.12 SEO

- **G-SEO-01 — `<title>` & meta description per halaman** dengan bawaan otomatis (nama toko + kota + kategori) dan batas panjang. Google [G1][G2]; Canva [C3]. · Muncul di Google & pratinjau WA. · **P0** · S · Field skema; renderer menulis di `<head>` saat build (static export mendukung metadata).
- **G-SEO-02 — Open Graph & Twitter card** (judul, deskripsi, gambar 1200×630). ogp.me [G7]. · Tautan yang dibagikan di WA/FB tampil kaya → klik lebih tinggi. · **P0** · S · Renderer; aset ukuran OG.
- **G-SEO-03 — LocalBusiness JSON-LD** dari profil (nama, alamat, telepon, jam buka, geo, `priceRange`). Google [G4]. · Pencarian lokal "warung X dekat saya". · **P0** · S · JSON-LD adalah `<script type="application/ld+json">`: **bukan** skrip yang dieksekusi, tetapi CSP `script-src` tanpa `unsafe-inline` umumnya tidak memblokir tipe data ini—**perlu diverifikasi** terhadap konfigurasi ADR-0014 (Next static export menulisnya inline); alternatif: microdata. Catat di ADR.
- **G-SEO-04 — Product JSON-LD** (nama, gambar, harga IDR, ketersediaan). Google [G5]. · **P1** · S · Sama dengan G-SEO-03.
- **G-SEO-05 — sitemap.xml & robots.txt per toko.** Google [G3][G9]. · **P0** · S · Dibuat saat terbit; dilayani Caddy.
- **G-SEO-06 — URL kanonik** (subdomain sekarang, domain sendiri kelak). Google [G8]. · Mencegah duplikat. · **P1** · S.
- **G-SEO-07 — Toggle "tampilkan di mesin pencari"** (`noindex`). Canva [C5]. · Toko uji coba/segera hadir. · **P1** · S.
- **G-SEO-08 — Slug halaman & produk yang bisa diedit** (huruf kecil, tanda hubung). · **P1** · S · Regex validator; redirect saat diubah.
- **G-SEO-09 — Satu `h1` per halaman & hierarki heading.** Saat ini setiap Banner = `h1`; dua Banner → dua `h1`. · SEO & WCAG 1.3.1. · **P0** · S · Renderer: h1 hanya untuk blok pertama/judul halaman.
- **G-SEO-10 — Pratinjau hasil Google & WA di editor.** · Edukasi pemilik. · **P2** · S.
- **G-SEO-11 — Saran SEO dengan AI.** Canva Magic SEO [C3]. · **P2** · S · AI platform (ADR-0011).

### 4.13 Performa

- **G-PRF-01 — Anggaran performa per halaman & pengukuran CWV lapangan.** Target LCP ≤2,5 s, INP ≤200 ms, CLS ≤0,1 pada persentil 75, mobile [V1]. · HP Android kelas bawah + jaringan lambat. · **P0** (anggaran + uji Lighthouse di CI) · M · RUM opsional via analitik first-party (G-ANL-01).
- **G-PRF-02 — Dimensi gambar & `aspect-ratio` untuk cegah CLS.** web.dev [V5]. · **P0** · S · Simpan lebar/tinggi aset.
- **G-PRF-03 — Font self-host, subset, `font-display: swap`.** web.dev [V4]. · **P0** · S.
- **G-PRF-04 — Chatbot AI dimuat malas** (setelah interaksi/idle) agar tidak memperberat LCP/INP. · **P1** · S.
- **G-PRF-05 — Batas berat halaman** (mis. total gambar di atas lipatan ≤ X KB; jumlah blok galeri). Peringatan di pemeriksa siap terbit. · **P1** · S.

### 4.14 Aksesibilitas

- **G-A11Y-01 — Alternatif tanpa seret untuk pindah/ubah ukuran.** Tombol di toolbar blok: Naik/Turun/Kiri/Kanan, Lebar −/+, Tinggi −/+. WCAG 2.5.7 [A1]. · Pengguna dengan tremor, trackpad laptop murah, dan **HP** (seret di layar kecil sulit). · **P0** · S · Frontend.
- **G-A11Y-02 — Target sentuh ≥24×24 px** untuk pegangan resize & tombol kecil. WCAG 2.5.8 [A1]. · **P0** · S.
- **G-A11Y-03 — Pegangan resize dapat difokus & diberi label** (saat ini `aria-hidden`). · **P1** · S.
- **G-A11Y-04 — Pengumuman `aria-live` untuk hasil aksi** ("Promo dipindah ke kolom 7"). · **P1** · S.
- **G-A11Y-05 — Fokus tidak tertutup chatbot melayang/panel.** WCAG 2.4.11 [A1]. · **P0** · S · Offset `scroll-padding`.
- **G-A11Y-06 — `lang="id"` & bahasa per halaman.** WCAG 3.1.1 [A1]. · **P0** · S.
- **G-A11Y-07 — Hormati `prefers-reduced-motion`** di carousel/animasi. WCAG 2.3.3/2.2.2 (carousel otomatis harus bisa dijeda) [A1]. · **P0** · S.
- **G-A11Y-08 — Label tautan bermakna** ("Pesan Nasi Goreng via WhatsApp", bukan "Klik di sini"). · **P1** · S.
- **G-A11Y-09 — Editor memandu konten aksesibel** (ATAG B.2.1 [A2]): prompt alt saat unggah, peringatan kontras, peringatan heading. · **P0** · bagian dari G-PUB-05.

### 4.15 Analitik

- **G-ANL-01 — Analitik first-party tanpa cookie:** kunjungan per hari, halaman populer, sumber (WA/IG/Google), klik WA, klik produk, klik marketplace. · Pemilik ingin tahu "promo saya dilihat berapa orang". · **P1** · M · Endpoint beacon same-origin (CSP `connect-src 'self'`), agregasi tanpa IP mentah; analisis UU PDP (data pribadi minimal).
- **G-ANL-02 — Integrasi Google Analytics/Meta Pixel.** Canva GA + banner cookie [C3]. · **P2 / sengaja ditunda** · M · Melonggarkan CSP + consent → lihat §6.
- **G-ANL-03 — UTM builder untuk tautan yang dibagikan.** · **P2** · S.

### 4.16 i18n

- **G-I18N-01 — Situs dua bahasa (ID + EN).** Wix Multilingual manual/otomatis [W7][W7b]; Webflow CMS localization [WF5]. · Oleh-oleh/turis (Bali, Yogyakarta), ekspor kerajinan. · **P2** · L · Skema teks menjadi map per locale; `hreflang` [G10].
- **G-I18N-02 — Terjemahan otomatis AI** dari ID ke EN. · **P2** · M · AI platform; tinjau manusia.
- **G-I18N-03 — Format lokal** (Rupiah, WIB/WITA/WIT, tanggal Indonesia). · **P0** · S · Sudah ada `rupiah()` di dasbor (lihat pedoman label).

### 4.17 Keamanan & moderasi

- **G-SEC-01 — Validator v2 server** (bounds, overlap, whitelist, panjang teks, URL scheme, jumlah blok, ukuran total JSON). · Tanpa ini Studio tidak boleh terbit. · **P0** · M · Rust; uji properti (seperti uji 300 langkah acak di frontend).
- **G-SEC-02 — Whitelist URL keluar** (wa.me, tokopedia.com, shopee.co.id, tiktok.com, maps.google.com, dll.) + `rel="noopener nofollow ugc"` pada tautan bebas. · Phishing/penipuan. · **P0** · S.
- **G-SEC-03 — Moderasi konten & pelaporan.** Filter kata kunci (judi, pinjol ilegal, obat keras), tombol "Laporkan situs" di footer, takedown di admin. · Risiko reputasi & hukum platform. · **P1** · M · Kebijakan konten; kajian hukum (UU ITE) — **belum terverifikasi** di riset ini.
- **G-SEC-04 — Anti-spam & rate limit form publik.** · **P1** (bersama G-PLG-01) · M.
- **G-SEC-05 — Pemindaian/normalisasi gambar** (strip EXIF lokasi rumah pemilik). · Privasi pemilik usaha rumahan. · **P0** · S · Pipeline aset.

### 4.18 Ekosistem plugin (arsitektur)

- **G-ECO-01 — Manifest plugin berversi.** `kind`, `version`, `settings_schema`, `min_w/min_h`, `max_per_page`, `allowed_pages`, `plans`. Rujukan: Shopify schema blok (`settings`, `enabled_on`/`disabled_on`, `available_if`) [S1]. · Satu sumber kebenaran untuk katalog, editor, validator Rust, renderer. · **P0** · M · Manifest dibagi frontend–Rust (dihasilkan dari satu file); ADR (menyempurnakan ADR-0017).
- **G-ECO-02 — Migrasi data plugin antarversi.** Fungsi migrasi `data v1→v2` per plugin di server. · Plugin pasti berubah setelah rilis. · **P1** · M · Validator menerima versi lama + migrasi saat simpan.
- **G-ECO-03 — Kategori plugin "embed"** (tidak menempati grid; satu per situs; posisi `head`/`body`-end) untuk chatbot melayang, WA melayang, pengumuman, pop-up. Shopify app embed: nonaktif bawaan setelah dipasang [S1]. · Merapikan model chatbot `floating` yang kini kasus khusus. · **P1** · S–M · Skema `embeds[]`.
- **G-ECO-04 — Gating paket (Starter/Pro) per plugin.** Wix: app gratis dipasang, fungsi utama perlu upgrade [W8]; Shopify `available_if` [S1]. · Model bisnis ADR-0016. · **P1** · S · Validator menolak plugin Pro di toko Starter; renderer menyembunyikan saat downgrade (perilaku downgrade perlu ADR).
- **G-ECO-05 — Pedoman UX plugin internal** (nama ≤25 karakter, pengaturan bawaan masuk akal, keadaan kosong). Shopify UX guideline [S2]. · Konsistensi saat plugin bertambah. · **P1** · S · Dokumen.
- **G-ECO-06 — Pengaturan plugin tingkat toko vs. tingkat instance** (mis. nomor WA, persona chatbot di toko; tata letak di instance). · **P1** · S.
- **G-ECO-07 — Status kesehatan plugin** (mis. "Chatbot: kuota AI habis", "Produk: 0 produk aktif"). · **P2** · S.

### 4.19 AI di builder

- **G-AI-01 — Buat situs dari tanya-jawab** (jenis usaha, kota, gaya, produk unggulan) → pilih template + isi teks + susun blok, menghasilkan **JSON v2** yang lolos validator. Wix [W9], Squarespace Blueprint [Q4], Framer Wireframer [F4]. · Mengurangi "halaman kosong" — hambatan terbesar UMKM. · **P1** · M · AI platform (ADR-0011); output hanya JSON → aman.
- **G-AI-02 — Tulis/ulang teks blok** ("lebih singkat", "gaya santai", "bahasa Jawa halus"). Shopify Magic theme editor [S6]; Canva Magic Write [C6]. · **P1** · S.
- **G-AI-03 — AI SEO & alt text otomatis** (disetujui pemilik). Canva Magic SEO [C3]. · **P1** · S.
- **G-AI-04 — Chatbot belajar dari isi situs** (FAQ, jam buka, produk) dengan sitasi blok. · **P1** · M · Sudah sebagian di ADR-0006 — belum terverifikasi.
- **G-AI-05 — Generate banner/poster promo dari brand kit.** Canva Magic Design [C7]. · **P2** · L.
- **G-AI-06 — Saran perbaikan desain** ("kontras rendah", "terlalu banyak blok di atas lipatan"). · **P2** · M.

### 4.20 Privasi & kepatuhan (UU PDP)

- **G-PDP-01 — Pemberitahuan privasi situs toko** yang dibuat otomatis (siapa pengendali data: toko; pemroses: platform; data apa; tujuan; retensi; kontak). UU PDP mengatur dasar pemrosesan, hak subjek, kewajiban pengendali & prosesor [L1]. · Wajib saat ada form, chatbot, pesanan. · **P0** · S–M · Halaman kebijakan (G-NAV-04); kajian hukum peran pengendali/prosesor.
- **G-PDP-02 — Persetujuan eksplisit pada formulir** (kotak centang tidak tercentang + tautan kebijakan) dan pencatatan bukti persetujuan. [L1] · **P1** (bersama form) · S.
- **G-PDP-03 — Pemberitahuan di chatbot** bahwa percakapan diproses AI & disimpan X hari (ADR-0012). [L1] · **P0** (chatbot sudah ada) · S.
- **G-PDP-04 — Tanpa cookie pelacak secara bawaan** → banner cookie tidak diperlukan; bila GA/Pixel diaktifkan kelak, consent wajib sebelum memuat. Canva menyediakan banner cookie bersama GA [C3]. · **P0** (sebagai prinsip) · S.
- **G-PDP-05 — Ekspor & hapus kiriman formulir per subjek** (hak akses/penghapusan). [L1] · **P1** · M.

---

## 5. Pertimbangan khusus Indonesia

1. **WhatsApp-first.** Konversi UMKM terjadi di WA, bukan keranjang. Konsekuensi: (a) tombol WA melayang & per produk (G-PLG-02) di atas keranjang; (b) templat pesan berisi nama produk, varian, jumlah, tautan; (c) analitik "klik WA" sebagai metrik utama (G-ANL-01); (d) pratinjau tautan (Open Graph, G-SEO-02) karena situs paling sering dibagikan *di dalam* WA. Format `wa.me` hanya dari sumber sekunder [M1]—verifikasi ke dokumentasi Meta sebelum implementasi.
2. **QRIS manual.** Blok "Cara bayar" menampilkan QRIS statis (ADR-0004) + instruksi unggah bukti; jangan tampilkan QRIS di halaman publik tanpa konteks pesanan (risiko penyalahgunaan/penipuan QR ganti). Logo metode bayar & COD (G-PLG-16).
3. **Marketplace sebagai saluran paralel.** Tokopedia & Shopee menyediakan dekorasi toko (banner, etalase, produk pilihan) [T1][T2][T3]; Tokopedia mengaitkan fitur dekorasi dengan tier Power Merchant/Power Shop (sebagian sekunder; skema 2024 [T4]). Studio sebaiknya **melengkapi**, bukan menggantikan: tautan marketplace per produk (G-PLG-03), "beli di sini atau di Shopee". Impor produk dari marketplace (seperti alat omnichannel Ginee, sekunder [M3]) adalah pembeda P2 — belum terverifikasi kelayakan API.
4. **Koneksi lambat & kuota mahal.** Kompres di browser sudah ada; tambahkan srcset/WebP (G-MED-04), banner non-lazy (G-MED-05), sembunyikan galeri berat di ponsel (G-LAY-02), chatbot dimuat malas (G-PRF-04), anggaran CWV (G-PRF-01). Editor: simpan otomatis tahan putus (G-EDT-05/06), unggah dapat dilanjutkan.
5. **HP Android kelas bawah.** Situs publik: JS minimal, tanpa animasi berat, uji pada perangkat 2–3 GB RAM. Editor: kanvas grid tidak cocok untuk HP → mode edit HP berbasis daftar (G-MOB-01).
6. **Bahasa.** UI editor Bahasa Indonesia baku (pedoman label `docs/ux/pedoman-label.md`); konten situs sering campuran (Indonesia + daerah + Inggris). AI tulis-ulang dengan pilihan nada lokal (G-AI-02). Multi-bahasa ID/EN untuk segmen wisata (G-I18N-01).
7. **Zona waktu & hari libur.** Jam buka per WIB/WITA/WIT, pengecualian libur nasional/cuti bersama dan Ramadan (jam buka berbeda) (G-PLG-15).
8. **Kepercayaan.** Testimoni, foto asli toko, alamat & peta, nomor WA bisnis, kebijakan pengembalian, tanda "toko terverifikasi" platform (P2) — penipuan online membuat pembeli waspada.
9. **Produk lokal pembanding.** BukaOlshop dan fitur *webstore* Ginee **belum terverifikasi** (tidak ditemukan dokumentasi resmi pada tanggal akses). Moka (diakuisisi Gojek 2020) disebut terintegrasi dengan GoStore untuk katalog online (sekunder [M2]); status GoStore 2026 **belum terverifikasi**.

---

## 6. Risiko & hal yang sengaja TIDAK disarankan

| Usulan | Alasan tidak disarankan | Alternatif |
|---|---|---|
| Blok HTML/JS/CSS bebas ("custom code") | Melanggar ADR-0007/0014 (CSP tanpa inline script, JSON tertutup); vektor XSS lintas toko di subdomain bersama; Carrd/Squarespace menyediakannya hanya untuk pengguna mahir [K1] | Tambah plugin resmi sesuai permintaan nyata |
| Plugin/app pihak ketiga (marketplace app) | Keputusan pemilik produk (ADR-0017); butuh sandbox, review, izin data, dukungan; Shopify menanggungnya dengan infrastruktur besar (validasi, batas ukuran) [S1] | Manifest internal yang meniru disiplin Shopify (G-ECO-01) |
| Kanvas bebas x/y per piksel & layout ponsel terpisah penuh | Ditolak di ADR-0017; Squarespace & Wix butuh perawatan dua layout dan sering rusak di ponsel [Q2][Q6] | Urutan ponsel + sembunyikan per perangkat (G-LAY-01/02) |
| Unggah SVG/font oleh pengguna | SVG dapat membawa skrip; font = lisensi & ukuran | Set ikon & font resmi (G-MED-07, G-BRD-04) |
| Embed iframe sembarang (peta, IG, TikTok, form eksternal) | Melonggarkan CSP `frame-src`, memuat pelacak pihak ketiga (UU PDP), berat di HP | Fasad + tautan; whitelist ketat bila sangat perlu (G-MED-06) |
| Google Analytics/Meta Pixel bawaan | Butuh skrip pihak ketiga & consent cookie; CSP; data pengunjung keluar negeri (transfer lintas batas UU PDP [L1]) | Analitik first-party (G-ANL-01) |
| Pembayaran online otomatis di builder | Di luar keputusan QRIS manual (ADR-0004) | Blok "Cara bayar" + konfirmasi pesanan |
| Grouping/z-order/overlap | Bertentangan dengan validator non-overlap; nilai kecil bagi UMKM | Varian layout per plugin |
| Generator AI yang mengeluarkan kode | Tidak dapat divalidasi; Shopify bahkan tidak mendukung troubleshoot blok kode hasil AI (sekunder) | AI hanya mengisi JSON v2 |

**Risiko lain:** (1) Static export + data dinamis (harga, stok, jadwal) → risiko konten basi (G-COM-04, G-PUB-06); butuh keputusan rebuild vs. data runtime. (2) JSON-LD inline vs. CSP perlu diverifikasi (G-SEO-03). (3) Batas paket & downgrade: apa yang terjadi pada plugin Pro saat toko turun ke Starter (G-ECO-04). (4) Retensi versi & kiriman form menambah data pribadi tersimpan (ADR-0012). (5) Fitur vendor berubah cepat; matriks ini perlu ditinjau ulang tiap 6 bulan.

---

## 7. Rekomendasi urutan pengerjaan (3 fase)

### Fase 1 — "Bisa terbit dengan aman" (sebelum rilis publik Studio, semua P0)
1. ADR-0017 → Accepted + ADR manifest plugin (G-ECO-01) dan kontrak v2.
2. Validator Rust v2 + migrasi v1→v2 (G-SEC-01, G-SEC-02); simpan draf server, status simpan (G-PUB-01, G-EDT-05).
3. Terbit + riwayat & pulihkan (G-PUB-02, G-PUB-03); pratinjau seperti pengunjung (G-EDT-09).
4. SEO dasar: title/meta, OG, LocalBusiness, sitemap/robots, satu h1, `lang` (G-SEO-01/02/03/05/09, G-A11Y-06).
5. Aksesibilitas: alt text di aset (G-MED-01), kontras (G-BRD-01), alternatif tanpa seret & target 24 px (G-A11Y-01/02/05/07).
6. Performa: srcset/WebP, banner prioritas, dimensi gambar, font (G-MED-04/05, G-PRF-01/02/03).
7. Plugin dasar yang hilang: Teks bebas (G-PLG-17), WA melayang & per produk (G-PLG-02), favicon (G-BRD-02).
8. Privasi: kebijakan privasi otomatis, pemberitahuan chatbot, tanpa cookie pelacak (G-PDP-01/03/04, G-NAV-04); strip EXIF (G-SEC-05).
9. Pemeriksa "siap terbit" versi minimal (G-PUB-05). Putuskan G-COM-04 (data basi).

### Fase 2 — "Toko yang lengkap" (P1, 1–3 bulan setelah rilis)
1. Multi-halaman + menu + template detail produk + anchor (G-NAV-01/02/03/07/08).
2. Kontrol perangkat: urutan ponsel, sembunyikan per perangkat, spasi, full-bleed, panel struktur (G-LAY-01/02/03/04/05/08).
3. Plugin: marketplace, pengumuman/pop-up (embed), FAQ, menu restoran, link-in-bio, QR, info bayar/kirim, libur (G-PLG-03/04/05/07/09/10/15/16/18); kategori embed & gating paket (G-ECO-03/04).
4. Media: pustaka, crop/focal point, video fasad (G-MED-02/03/06) — ADR CSP untuk `frame-src`.
5. Form + PDP + anti-spam (G-PLG-01, G-PDP-02/05, G-SEC-04).
6. Analitik first-party (G-ANL-01); jadwal tampil blok (G-PUB-06); tautan pratinjau (G-PUB-04).
7. Mode edit HP sederhana (G-MOB-01/02); peran edit vs terbit, kunci edit (G-COL-01, G-PUB-08).
8. AI: buat situs dari tanya-jawab, tulis ulang teks, SEO & alt otomatis (G-AI-01/02/03).
9. Atribusi footer Starter (G-DOM-02); moderasi & pelaporan (G-SEC-03).

### Fase 3 — "Pembeda" (P2)
Domain sendiri (Pro, ADR baru menggantikan ADR-0008), dua bahasa + AI terjemah, booking jasa, blok sinkron, brand kit lintas fitur + generator poster AI, ulasan terverifikasi, hapus latar foto, foto stok, edit offline, komentar kolaborasi, impor dari marketplace (bila API memungkinkan), saran desain AI.

---

## 8. Daftar pustaka

Semua diakses 2026-11-13.

**Standar & regulasi**
- [A1] W3C. *Web Content Accessibility Guidelines (WCAG) 2.2*, W3C Recommendation 12 Desember 2024 (SC 1.1.1, 1.3.1, 1.4.3, 2.2.2, 2.3.3, 2.4.11, 2.5.7, 2.5.8, 3.1.1, 3.2.6, 3.3.7). https://www.w3.org/TR/WCAG22/
- [A2] W3C. *Authoring Tool Accessibility Guidelines (ATAG) 2.0*, Recommendation 24 September 2015 (Part B, B.2.1). https://www.w3.org/TR/ATAG20/
- [V1] web.dev. *Web Vitals* (LCP ≤2,5 s, INP ≤200 ms, CLS ≤0,1, persentil 75). https://web.dev/articles/vitals
- [V2] web.dev. *Optimize Largest Contentful Paint*. https://web.dev/articles/optimize-lcp
- [V3] web.dev. *Responsive images* (Learn Responsive Design). https://web.dev/learn/design/responsive-images
- [V4] web.dev. *Ensure text remains visible during webfont load* (`font-display`). https://web.dev/articles/font-display
- [V5] web.dev. *Cumulative Layout Shift (CLS)*. https://web.dev/articles/cls
- [G1] Google Search Central. *Influencing your title links*. https://developers.google.com/search/docs/appearance/title-link
- [G2] Google Search Central. *Control your snippets in search results* (meta description). https://developers.google.com/search/docs/appearance/snippet
- [G3] Google Search Central. *Sitemaps overview*. https://developers.google.com/search/docs/crawling-indexing/sitemaps/overview
- [G4] Google Search Central. *Local business (LocalBusiness) structured data*. https://developers.google.com/search/docs/appearance/structured-data/local-business
- [G5] Google Search Central. *Product structured data* (termasuk aturan ulasan). https://developers.google.com/search/docs/appearance/structured-data/product
- [G6] Google Search Central. *Google Images SEO best practices*. https://developers.google.com/search/docs/appearance/google-images
- [G7] The Open Graph protocol. https://ogp.me/
- [G8] Google Search Central. *Consolidate duplicate URLs (canonical)*. https://developers.google.com/search/docs/crawling-indexing/consolidate-duplicate-urls
- [G9] Google Search Central. *Robots meta tag, data-nosnippet, and X-Robots-Tag*. https://developers.google.com/search/docs/crawling-indexing/robots-meta-tag
- [G10] Google Search Central. *Tell Google about localized versions of your page* (hreflang). https://developers.google.com/search/docs/specialty/international/localized-versions
- [L1] Republik Indonesia. *Undang-Undang Nomor 27 Tahun 2022 tentang Pelindungan Data Pribadi* (LN 2022/No. 196, TLN 6820; catatan Putusan MK 151/PUU-XXII/2024 atas Pasal 53 ayat (1) huruf b). https://peraturan.bpk.go.id/Details/229798/uu-no-27-tahun-2022

**Shopify**
- [S1] Shopify.dev. *Configure theme app extensions* (schema app block/app embed: `target` `head`/`body`/`compliance_head`, `enabled_on`/`disabled_on`, `available_if`, batas 30 blok, 10 MB, validasi saat deploy). https://shopify.dev/docs/apps/build/online-store/theme-app-extensions/configuration
- [S2] Shopify.dev. *UX guidelines for theme app extensions*. https://shopify.dev/docs/apps/build/online-store/theme-app-extensions/ux
- [S3] Shopify.dev. *Section groups*. https://shopify.dev/docs/storefronts/themes/architecture/section-groups
- [S4] Shopify.dev. *Integrate sections and blocks with the theme editor*; *Theme editor best practices*. https://shopify.dev/docs/themes/architecture/sections/integrate-sections-with-the-theme-editor ; https://shopify.dev/docs/storefronts/themes/best-practices/editor (pemilihan template, sembunyikan/urutkan section & blok, tema draf vs. publish — detail tombol *Publish* untuk tema draf berasal dari panduan sekunder)
- [S5] Shopify Help Center. *Extend your theme with apps*. https://help.shopify.com/manual/online-store/themes/theme-structure/extend/apps
- [S6] Shopify Help Center. *Automatically generating content in the online store editor* (Shopify Magic/Sidekick). https://help.shopify.com/manual/shopify-magic/online-store-editor
- [S7] Shopify.dev. *Theme app extensions framework*. https://shopify.dev/docs/apps/online-store/theme-app-extensions/extensions-framework

**Wix**
- [W1] Wix Help. *Wix Editor: Hidden Elements in Your Mobile Editor*. https://support.wix.com/en/article/wix-editor-hidden-elements-in-your-mobile-editor
- [W2] Wix Help. *Wix Harmony Editor: website aanpassen voor mobiel* (Layers → Hide di tampilan mobile). https://support.wix.com/article/wix-harmony-editor-je-website-aanpassen-voor-mobiel
- [W3] Wix Help. *Aligning, distributing and matching the size of multiple elements* (Smart Guide). https://support.wix.com/en/article/aligning-distributing-and-matching-the-size-of-multiple-elements
- [W3b] Wix Help. *Wix Editor: Using the Layers Panel*. https://support.wix.com/en/article/wix-editor-using-the-layers-panel
- [W3c] Wix Help. *Wix Editor: Grouping Elements*. https://support.wix.com/en/article/wix-editor-grouping-elements
- [W4] Wix Help. *Wix Editor: Using the Right-Click Menu*. https://support.wix.com/en/article/wix-editor-using-the-right-click-menu-7743975
- [W5] Wix Help. *Viewing and managing your site history*. https://support.wix.com/en/article/viewing-and-managing-your-site-history
- [W6] Wix Help. *Restoring a saved version of your site* (komponen yang tidak ikut dipulihkan). https://support.wix.com/en/article/restoring-a-saved-version-of-your-site
- [W7] Wix Help. *Wix Multilingual: An Overview*. https://support.wix.com/en/article/wix-multilingual-an-overview
- [W7b] Wix Help. *Wix Multilingual: Auto-Translating Your Site*. https://support.wix.com/en/article/wix-multilingual-auto-translating-your-site
- [W8] Wix Help. *About Wix Bookings*. https://support.wix.com/en/article/about-wix-bookings
- [W8b] Wix Help. *Wix Bookings: Adding Wix Bookings from the Wix Owner App*. https://support.wix.com/en/article/wix-bookings-adding-wix-bookings-from-the-wix-owner-app
- [W9] Wix Help. *Wix Editor: Creating an AI-Generated Site*. https://support.wix.com/en/article/wix-editor-creating-an-ai-generated-site

**Squarespace**
- [Q1] Squarespace Help. *Editing your site with Fluid Engine* (grid tampil saat menyeret; tekan G; batas 1000 baris). https://support.squarespace.com/hc/articles/6421525446541
- [Q2] Squarespace Help. *How will my site appear on mobile devices?* (layout mobile dapat diatur terpisah). https://support.squarespace.com/hc/en-us/articles/205815398-How-will-my-site-appear-on-mobile-devices
- [Q3] Squarespace Help. *Site styles* (font, warna, tema section). https://support.squarespace.com/hc/articles/205815788
- [Q4] Squarespace. *Guide to Squarespace AI tools* (Blueprint AI; blog resmi vendor). https://www.squarespace.com/blog/guide-to-squarespace-ai-tools
- [Q5] (sekunder) Floating Lotus Design. *Squarespace promotional pop-ups and announcement bar tutorial*. https://floatinglotusdesign.squarespace.com/blog/squarespace-promotional-popups-and-announcement-bar-tutorial
- [Q6] (sekunder) Will Myers. *Fixing the tablet spacing problem in Squarespace Fluid Engine*. https://will-myers.com/articles/fixing-the-tablet-spacing-problem-in-squarespace-fluid-engine

**Canva**
- [C1] Canva Help. *Canva Websites overview and getting started guide*. https://www.canva.com/help/canva-websites/
- [C2] Canva Help. *Publish Canva websites* (domain gratis/beli/sendiri; domain sendiri hanya Pro/Teams; ≤5 domain; footer "Designed with Canva" di Free; Link in Bio hanya canva.link). https://www.canva.com/help/publishing-canva-websites/
- [C3] Canva Help. *Website settings* (favicon, judul/deskripsi/URL per halaman, Magic SEO, kata sandi, Google Analytics, banner cookie). https://www.canva.com/help/website-settings/
- [C4] Canva Help. *Navigation for Canva websites* (menu otomatis dari judul halaman; ≤45 halaman; Resize on mobile). https://www.canva.com/help/navigation-canva-websites/
- [C5] Canva Help. *Optimize your Canva website* (SEO, visibilitas mesin pencari). https://www.canva.com/help/optimize-canva-website/
- [C6] Canva Help. *Create on-brand designs* / *Brand voice* (Brand Kit, Magic Write, Brand Intelligence). https://www.canva.com/help/create-on-brand-designs/ ; https://www.canva.com/help/brand-voice/
- [C7] Canva Help. *Use Magic Design*. https://www.canva.com/help/use-magic-design/

**Framer**
- [F1] Framer Help. *Setting up your Framer site for scale* (breakpoint, staging). https://www.framer.com/help/articles/setting-up-your-framer-site-for-scale/
- [F2] Framer Help. *Guide to web accessibility in Framer*. https://www.framer.com/help/articles/guide-to-web-accessibility-in-framer
- [F3] Framer Academy. *Manage Staging and Versions for your site* (catatan: dirilis sebelum pembaruan CMS 16 Juni 2026). https://www.framer.com/academy/lessons/staging-versions
- [F4] Framer. *Wireframer — AI website builder* (halaman produk). https://www.framer.com/wireframer/
- [F5] Framer Help. *How to add Alt Tags to images*. https://www.framer.com/help/articles/how-to-add-alt-tags-to-images

**Webflow**
- [WF1] Webflow Help. *Save and restore backups* (titik pulih tiap 50 simpan otomatis; versi saat ini disimpan sebelum restore; item terjadwal). https://help.webflow.com/hc/en-us/articles/33961244069395
- [WF2] Webflow. *Publishing* (staging subdomain, Site Activity). https://webflow.com/webflow-way/collaboration/publishing
- [WF3] Webflow Help. *Include alt text on images* (alt di aset, diwarisi; opsi dekoratif). https://help.webflow.com/hc/en-us/articles/33961330170643
- [WF4] Webflow Developers. *Localizing Components (Beta)*. https://developers.webflow.com/data-beta/docs/localizing-components-beta
- [WF5] Webflow Help. *Localize Collection content*. https://help.webflow.com/hc/en-us/articles/33961252209171

**WordPress**
- [P1] WordPress.org Documentation. *Comparing Patterns, Template Parts, and Synced Patterns*. https://wordpress.org/documentation/article/comparing-patterns-template-parts-and-reusable-blocks/
- [P2] WordPress Developer Resources. *Introduction to Patterns*. https://developer.wordpress.org/themes/patterns/introduction-to-patterns/
- [P3] WordPress.org Documentation. *Keyboard shortcuts (Block Editor)* (Shift+Alt+H; List View). https://wordpress.org/documentation/article/keyboard-shortcuts-block-editor/
- [P4] WordPress.org News. *Leap into 2024 with these Site Editor Tools* (Style Book, Global Styles). https://wordpress.org/news/2023/12/leap-into-2024-with-these-site-editor-tools/

**Carrd & Linktree**
- [K1] Carrd Docs. *Pro features*. https://carrd.co/docs/pro/features
- [K2] Carrd Docs. *Setting up a Contact Form* (Pro Standard+). https://carrd.co/docs/forms/setting-up-a-contact-form
- [K3] Carrd Docs. *Setting up a Custom Email Form* (Pro Plus+). https://carrd.co/docs/forms/setting-up-a-custom-email-form
- [L2] Linktree Help. *Scheduling links*. https://intercom.help/linktree-ff524ba1864c/en/articles/5434174-scheduling-links
- [L3] (sekunder) Influencer Marketing Hub. *Linktree review* (QR, analitik, Shops). https://influencermarketinghub.com/linktree/

**Indonesia**
- [T1] Tokopedia Help. *Informasi seputar Dekorasi Toko*. https://www.tokopedia.com/help/article/st-2145-informasi-seputar-dekorasi-toko
- [T2] Shopee Indonesia (Inspirasi Shopee, resmi). *Aktifkan fitur Produk Pilihan Toko*. https://shopee.co.id/inspirasi-shopee/buat-pembeli-borong-produk-tokomu-dengan-aktifkan-fitur-produk-pilihan-toko/
- [T3] (sekunder) BigSeller. *Peningkatan fitur Shopee Seller Centre update 2025* (Dekorasi Toko: panel komponen, kanvas, panel properti; Galeri Media & Kategori Toko). https://www.bigseller.com/blog/articleDetails/3995/fitur-seller-centre-update.htm
- [T4] Tokopedia Help. *Kebijakan Power Shop*. https://www.tokopedia.com/help/article/t-int79-kebijakan-power-shop
- [M1] (sekunder) Bird. *What is a WhatsApp click-to-chat link* (format `wa.me/<nomor>?text=`; faq.whatsapp.com tidak dapat diakses saat riset). https://bird.com/explained/whatsapp/what-is-a-whatsapp-click-to-chat-link
- [M2] (sekunder) Warta Ekonomi. *Gojek resmi akuisisi Moka*; Jawapos. *Solusi Moka dari Grup GoTo bantu UMKM* (integrasi GoStore). https://wartaekonomi.co.id/read283584/gojek-resmi-akuisisi-moka ; https://www.jawapos.com/ekonomi/2111090163/solusi-moka-dari-grup-goto-bantu-umkm-pulihkan-ekonomi
- [M3] (sekunder) DailySocial. *13 Ginee features* (omnichannel, sinkron stok). https://en.dailysocial.id/post/Gyne-features
- BukaOlshop — **belum terverifikasi**: tidak ditemukan dokumentasi resmi pada tanggal akses.

**Internal (repo)**
- `docs/decisions/0007-page-schema-dan-editor-section.md`, `0008-custom-domain-di-luar-v1.md`, `0012-retention-dan-purge.md`, `0014-kontrak-frontend-host-caddy-api.md`, `0016-langganan-starter-pro-pembayaran-manual.md`, `0017-studio-grid-dan-plugin.md`
- `frontend/src/renderer/Site.tsx` (temuan `alt=""`, `loading="lazy"` pada semua gambar, h1 di setiap Banner, tidak ada metadata SEO), `frontend/src/studio/Canvas.tsx` (pegangan resize `aria-hidden`, alternatif keyboard), `frontend/src/studio/plugins.ts`.
