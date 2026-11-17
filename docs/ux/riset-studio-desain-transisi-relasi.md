# Riset lanjutan Studio, situs subdomain & plugin — template, desain, transisi, relasi

- Tanggal riset & akses sumber: **2026-11-17**. Melanjutkan `docs/ux/riset-studio-website-builder.md` (2026-11-13, fokus celah fitur); dokumen ini fokus **arah desain**: template, sistem gaya situs toko, gerak/transisi, dan relasi antarbagian (Studio ↔ subdomain ↔ plugin ↔ data toko).
- Dokumen ini **riset & usulan**, bukan keputusan. Butir bertanda **[ADR]** menyentuh kontrak `content_json` v2 / validator Rust / manifest plugin (ADR-0017, ADR-0019) dan perlu keputusan pemilik produk.
- Sitasi `[n]` → Daftar pustaka (§9). `(sekunder)` = sumber pihak ketiga. `belum terverifikasi` = tidak dapat dipastikan dari sumber primer.

---

## 0. Pembaruan terbaru (titik awal riset)

- `origin/main` berisi PR #3 dari kontributor (6d56103, digabung 0449b56): kartu gaya "Kartu" di permukaan gelap bingkai Studio + footer menempel di dasar halaman pendek (`.sv2` jadi kolom flex). Sudah ditarik ke lokal.
- Dibanding riset 13 Nov, celah P0 berikut **sudah tertutup**: draf/terbit/versi/rollback (`/website/pages/{slug}/publish|versions|rollback`), multi-halaman + navbar atas/samping, SEO head per Host + sitemap + JSON-LD (`apps/api/src/site_head.rs`, ADR-0018), plugin WhatsApp, `hideOn` per perangkat, pemeriksa siap terbit (`studio/readiness.ts`), font toko self-hosted, data tertanam di dokumen (`__site`), kontrak plugin (ADR-0019 + `plugin-manifest.json`).
- Artinya fondasi sudah ada; pembeda berikutnya bukan "fitur dasar" lagi, melainkan **kualitas hasil jadi (template & gaya), rasa (gerak), dan keterhubungan data (relasi)**.

## 1. Ringkasan rekomendasi (urut dampak/usaha)

| # | Rekomendasi | Area | Usaha | Butuh keputusan? |
|---|---|---|---|---|
| 1 | **Sumber data eksplisit di plugin**: Galeri produk bisa memilih *kategori / produk pilihan / terbaru / terlaris*, bukan hanya `limit` | Relasi | M | **[ADR]** field baru `source` di `product_grid` |
| 2 | **Paket Gaya (Style preset)** = palet + pasangan font + bentuk (radius) + tombol + gerak; template = *kerangka* + *gaya* terpisah | Desain/Template | M | **[ADR]** `theme.style`, `theme.shape` |
| 3 | **Transisi antarhalaman situs tanpa JS** via `@view-transition { navigation: auto }` + morf foto produk kartu → halaman produk | Transisi | S–M | Tidak (peningkatan progresif) |
| 4 | **Palet turunan dari 1 warna merek** (OKLCH): tint/latar/garis/hover otomatis + jaminan kontras | Desain | S | Tidak (renderer saja) |
| 5 | **Mode "Link-in-bio"** (template satu kolom untuk bio Instagram/TikTok) + **QR toko** siap cetak | Template/Subdomain | S | Tidak |
| 6 | **Animasi muncul saat gulir tanpa JS** (`animation-timeline: view()`), pengaturan gerak per situs: *Tenang / Hidup / Mati* | Transisi | S | **[ADR]** kecil: `theme.motion` |
| 7 | **Bagian global** (bilah pengumuman/promo yang tampil di semua halaman, terjadwal) | Relasi/Plugin | M | **[ADR]** |
| 8 | **Template halaman produk** (opsi terbatas: tata letak foto, tombol WA, produk terkait) | Relasi | M | **[ADR]** |
| 9 | Plugin baru prioritas: Pengumuman, FAQ (juga makan chatbot), Kategori, Cara bayar, Hitung mundur promo, Reservasi | Plugin | M–L | **[ADR]** per plugin (manifest) |
| 10 | Template vertikal baru: Kecantikan/salon, Katering/pre-order, Kartu nama digital | Template | S | Tidak |

---

## 2. Peta relasi saat ini (Studio ↔ subdomain ↔ plugin ↔ data)

```
                    ┌────────────── Dasbor ───────────────┐
 Profil bisnis ─────┤ nama, logo, WA, alamat, jam buka     │
 Katalog ───────────┤ produk, varian, kategori (category_id)│
 Asisten AI ────────┤ chatbot aktif, kuota (paket)          │
 Paket (ADR-0016) ──┤ starter / pro → plan di manifest      │
                    └──────────────┬──────────────────────┘
                                   │ dibaca oleh
   Studio (draf) ── theme ─┬── halaman[] ── blocks[] (plugin) ──► Terbitkan (versi)
                           │                                          │
                           └── nav (header/footer global)             ▼
                                                     {slug}.domain  (Caddy → shell statis)
                                                     ├─ /, /tentang, /kontak …  (Studio)
                                                     ├─ /produk/{slug}           (tetap, bermerek)
                                                     └─ /keranjang /pesanan /akun (tetap, shop/)
```

Celah relasi yang ditemukan di kode:

1. **`product_grid` hanya punya `title` + `limit`** (`renderer/sections.ts:67`, contoh di `schema_v2.rs`), padahal katalog sudah punya `category_id` (`crates/domain/src/catalog.rs:191`). Toko tidak bisa membuat "Menu minuman" dan "Menu makanan" sebagai dua blok terpisah — padahal ini pola paling umum di kuliner/kelontong.
2. **Halaman produk, keranjang, pesanan berada di luar Studio** (`shop/route.ts`). Konsisten secara merek (tema ikut), tetapi pemilik tidak punya kendali sama sekali (mis. menampilkan tombol "Pesan via WA" per produk, produk terkait).
3. **Relasi data plugin tersirat**: Jam buka/Lokasi/WhatsApp mengambil dari Profil bisnis, Chatbot dari Asisten AI. `readiness.ts` sudah memperingatkan bila kosong — bagus — tetapi Inspector belum menampilkan *"Data dari: Profil bisnis · Ubah →"* di blok itu sendiri.
4. **Tidak ada bagian lintas halaman** selain header/footer. Promo/pengumuman harus ditaruh ulang di tiap halaman.

## 3. Template

### 3.1 Diagnosis 6 template saat ini (`studio/templates.ts`)

Kekuatan: langsung terisi data toko (nama, deskripsi), per vertikal (kuliner, butik, jasa, kerajinan, kafe malam, kelontong), sudah memakai variasi layout plugin (carousel/list/floating).

Kelemahan:
- **Gaya dan kerangka melekat jadi satu**: memilih "Butik" = otomatis magenta + Fraunces + hero gelap. Pemilik yang suka kerangka Butik tapi berwarna hijau harus menyetel ulang manual.
- **Hanya halaman Beranda**. Template belum membawa halaman Tentang/Kontak yang sudah didukung multi-halaman.
- Gaya hanya berbeda di warna/font/surface; **bentuk (radius), tombol, kepadatan, dan gerak sama** di semua template → hasil terasa "satu keluarga" dan kurang khas.
- Belum ada template untuk kebiasaan UMKM paling umum: **link-in-bio** (satu kolom, tombol besar) dan **pre-order/katering**.

### 3.2 Rujukan

- **Shopify Horizon** (Summer '25): satu basis kode, **10 preset** (Fabric, Dwell, Heritage, Pitch, …) yang berbeda lewat pengaturan, bukan kode berbeda; blok bertingkat hingga 8 level, *group block*, salin-tempel blok, pratinjau saat hover di pemilih blok [1][2]. Ulasan pedagang campuran (sekunder) [3] — pelajaran: preset menarik, tapi perubahan perilaku blok antar versi membuat pengguna kecewa → jaga kompatibilitas (kita sudah punya `version` per plugin).
- **Squarespace Site Styles**: *Themes* berupa paket yang mengubah font+warna+tombol sekaligus; palet 5 warna → **10 tema warna** yang diterapkan per section; **60+ font pack** berpasangan (heading + isi); pengaturan spasi, tombol, dan **animasi tingkat situs** [4][5 (sekunder)].
- **Take App** (pembuat toko WhatsApp, ASEAN): template per industri (F&B, retail, grocery, salon, fitness, fashion, tour, meal prep), pesanan dikirim lewat WhatsApp, template *link-in-bio* [6][7].
- **Lynk.id**: microsite link-in-bio lokal, >5 juta pengguna menurut pemberitaan (sekunder) [8] — bukti bahwa UMKM/kreator Indonesia terbiasa dengan pola satu halaman bertombol.

### 3.3 Rekomendasi template

1. **Pisahkan *Kerangka* dan *Gaya*** (pola Horizon/Squarespace). Galeri template = kerangka (susunan blok + halaman); setelah memilih, tampil 4–6 kartu **Gaya** yang bisa ditukar satu klik tanpa mengubah konten. Implementasi: `TplResult` dipecah menjadi `layout()` dan `style: StyleKey`; gaya disimpan di `theme.style` **[ADR]**.
2. **Template membawa halaman**: Beranda + Tentang + Kontak (Starter = 3 halaman sesuai spek 16 Nov) dengan isi awal dari profil bisnis.
3. **Template baru** (urut relevansi UMKM):
   - *Kartu nama digital / Link-in-bio* — satu kolom, avatar/logo, 4–6 tombol besar (WA, katalog, marketplace, maps), produk pilihan. Dipakai di bio Instagram/TikTok.
   - *Pre-order & katering* — hitung mundur batas pesan, menu per hari, cara bayar QRIS.
   - *Kecantikan & salon* — layanan + harga (product_grid layout `list`), reservasi via chatbot/WA, galeri sebelum–sesudah.
4. **Pratinjau template dengan data toko sendiri** (sudah ada endpoint `templates/{key}/preview`) — pertahankan; tambahkan toggle ponsel/desktop di kartu galeri.
5. **Isi contoh harus terlihat contoh**: testimoni bawaan ("Rina", "Budi") berisiko tampil sebagai ulasan asli. Tandai di editor ("Contoh — ganti dengan ulasan asli") dan masukkan ke `readiness.ts`; pertimbangkan tidak menerbitkan testimoni yang belum diubah (isu kepercayaan & perlindungan konsumen).

## 4. Desain — sistem gaya situs toko

### 4.1 Kondisi
`theme` = `primary` (bebas), `surface` (light/cream/dark), `font` (7 preset, satu font untuk semua), `text_scale`, navbar, latar gambar; per blok `style` = plain/card/tint/dark. Warna turunan hanya `--s-on-primary` & `--s-shade` (`renderer/safe.ts:35`).

### 4.2 Rekomendasi

| Token | Usulan | Alasan / rujukan |
|---|---|---|
| **Palet turunan** | Dari `primary`, hitung di renderer (OKLCH): `primary-soft` (latar tint), `primary-line`, `primary-hover`, `primary-ink` (teks berwarna yang lolos 4.5:1 di surface aktif). Hanya ubah *L* (lightness), pertahankan hue/chroma | OKLCH: lightness sejalan dengan persepsi, kontras bisa dikejar dengan menggeser L saja [9][10 (sekunder)]. Mencegah kasus "kuning di putih tak terbaca" tanpa membatasi pilihan warna pemilik |
| **Warna aksen** (opsional) | `theme.accent` untuk badge promo/harga coret | Squarespace memakai palet 5 warna [4]; 2 warna cukup untuk UMKM |
| **Pasangan font** | Ganti `font` tunggal menjadi `font_pair`: judul + isi (mis. Fraunces/Inter, Poppins/Nunito, Merriweather/Jakarta). Tetap whitelist, tetap self-hosted | Font pack Squarespace (2 font berpasangan) [5] |
| **Bentuk** | `shape`: *tegas* (4px) / *lembut* (12px) / *bulat* (24px + tombol pil) | Pembeda karakter termurah; satu variabel CSS |
| **Gerak** | `motion`: *tenang* (default) / *hidup* / *mati* | Squarespace: animasi tingkat situs [4]; lihat §5 |
| **Gaya blok** | Pertahankan plain/card/tint/dark, tetapi **turunkan warnanya dari palet** (tint = `primary-soft`, dark = surface gelap bermerek) | Setara "section themes" Squarespace [4] |

**Paket Gaya** = kombinasi token di atas dengan nama manusiawi (mis. *Hangat*, *Elegan*, *Segar*, *Malam*, *Tegas*, *Lembut*). Pemilik memilih paket, lalu boleh mengubah satu token.

### 4.3 Relevansi desain per vertikal

| Vertikal | Palet | Font | Bentuk | Gerak | Catatan |
|---|---|---|---|---|---|
| Kuliner/kafe | hangat (oranye/merah bata), cream | sans geometris ramah | lembut | hidup | Foto makanan = bintang; harga besar |
| Fashion/butik | netral + 1 aksen, kontras tinggi | serif judul + sans isi | tegas | tenang | Carousel/lookbook, banyak ruang kosong |
| Jasa/servis | biru/teal (kepercayaan) | sans netral | lembut | tenang | Kontak & testimoni di atas lipatan |
| Kelontong/sembako | hijau/kuning, latar terang | sans bulat, ukuran besar | bulat | mati/tenang | HP kelas bawah: ringan, terbaca |
| Kerajinan/seni | tanah (cokelat, terakota) | serif | tegas/lembut | hidup | Galeri besar |
| Kecantikan/salon | pastel/rose | serif elegan + sans | bulat | tenang | Sebelum–sesudah |

### 4.4 Tren yang relevan vs. yang dihindari

- **Relevan: bento grid** — kartu asimetris dengan isi berbeda; disebut tren 2026 di banyak sumber (sekunder) [11]. Grid 12 kolom + gaya blok *card* kita **sudah** bento secara alami → jadikan bahasa pemasaran ("susun seperti bento") dan pastikan template punya minimal satu baris bento (mis. Jam buka | Lokasi | Kontak). Saran sumber yang sama: satu animasi, satu momen — teks & CTA tetap statis.
- **Hindari**: tipografi kinetik besar, parallax berat, video latar otomatis, carousel berputar sendiri — berat untuk HP 2–3 GB & kuota, dan mengganggu (WCAG 2.2.2 Pause/Stop/Hide).

## 5. Transisi & gerak — relevansi per lapisan

Prinsip: gerak harus **menjelaskan hubungan** (dari mana ke mana), bukan hiasan. Situs publik = pembeli dengan HP murah → gerak tanpa JS, di thread compositor, mati otomatis saat `prefers-reduced-motion` (sudah ada global di `site.css:218`).

### 5.1 Antarhalaman situs toko (MPA) — **paling relevan**
Situs toko memakai navigasi dokumen penuh (`<a href>` di `shop/views.tsx`, shell statis ADR-0014). Ini cocok persis dengan **cross-document view transitions**:

- Aktifkan dengan `@view-transition { navigation: auto; }` di kedua halaman (asal sama — semua halaman satu subdomain memenuhi) [12].
- Dukungan: Chrome/Edge 126+, Safari 18.2+; Firefox belum/parsial (sumber berbeda pendapat) [12][13][14] → tanpa dukungan, navigasi biasa (aman).
- **Morf foto produk**: kartu di galeri → foto besar di `/produk/{slug}`. Pasang `view-transition-name` *tepat waktu* di event `pageswap` (halaman lama) dan `pagereveal` (halaman baru) berdasarkan URL tujuan, lalu hapus setelah transisi agar BFCache aman [12].
- **Risiko yang harus diprototipekan**: konten situs kita dirender React di klien. Snapshot halaman baru diambil saat render pertama; bila foto produk belum ada di DOM, morf gagal (jatuh ke cross-fade). Opsi: `<link rel="expect" blocking="render" href="#…">` hanya bekerja untuk elemen dari parser dan menunda render pertama — **ukur LCP dulu** [12]. Chrome melewati transisi bila navigasi >4 detik.
- Bonus (Chromium): Speculation Rules `prerender` dengan eagerness `moderate` untuk tautan produk → navigasi terasa instan. Catatan: prerender memicu permintaan API/analitik; jangan hitung kunjungan saat prerender.

### 5.2 Dalam halaman
- **Muncul saat gulir tanpa JS**: `animation-timeline: view()` kini didukung Chrome, Safari 26, dan Firefox 159 menurut web.dev (Okt 2026) [15][16]. Bungkus `@supports (animation-timeline: view())`; efek kecil saja (opacity + translateY 8–12px). Dikendalikan `theme.motion`.
- Hover kartu produk: angkat 2px + bayangan — sudah ada; pastikan hanya di `@media (hover: hover)` (konvensi repo).
- Tambah ke keranjang: ikon keranjang "pop" + hitungan — sudah ada `shop-pop`; cukup.

### 5.3 Studio (editor)
- Gerak editor bersifat **fungsional**: blok baru masuk (scale 0.96→1, ≤200 ms, ease-out), geser/ubah ukuran mengikuti pointer tanpa easing, blok lain bergeser memberi tempat (FLIP, 150–200 ms).
- **Ganti perangkat (desktop ↔ ponsel)** dan **ganti Paket Gaya**: pakai same-document `document.startViewTransition()` agar pemilik *melihat* apa yang berubah (dasbor sudah memakai pola `d-route`).
- **Momen terbit**: setelah "Terbitkan" sukses, kartu kecil berisi tautan subdomain + QR + tombol "Bagikan ke WhatsApp" — titik paling emosional di alur, saat ini hanya toast.
- Durasi mengikuti token repo (`--dur-fast` 120 / `--dur` 180 / `--dur-slow` 240 ms). Material 3 Expressive memindahkan gerak ke tingkat tema ("motion scheme") dengan pegas (sekunder) [17] — selaras dengan usulan `theme.motion`, tetapi untuk situs publik tetap pakai CSS easing (pegas butuh JS).

### 5.4 Matriks relevansi

| Gerak | Situs publik | Studio | Alasan |
|---|---|---|---|
| Transisi antarhalaman (view transition) | **Ya** | Ya (same-document) | Menjaga orientasi, tanpa JS di MPA |
| Morf kartu → detail produk | **Ya** | — | Menghubungkan pilihan dengan hasil |
| Muncul saat gulir | Opsional (`motion`) | Tidak | Hiasan; mati untuk "mati"/reduced-motion |
| Parallax/latar bergerak | Tidak | Tidak | Berat, mual (vestibular) |
| Carousel berputar otomatis | Tidak | — | WCAG 2.2.2, menurunkan klik |
| Pegas/spring | Tidak | Ya (drag) | Butuh JS; editor sudah interaktif |

## 6. Relasi — rekomendasi rinci

1. **Sumber data per plugin ("dynamic sources")** — rujukan Shopify: pengaturan blok bisa dihubungkan ke resource/metafield, dengan batas jumlah demi performa [18]. Untuk kita: `product_grid.source = { kind: "all" | "category" | "featured" | "newest" | "bestseller", category_id?, product_ids? }` **[ADR]**. Validator memastikan `category_id` milik tenant; renderer publik memfilter di API (bukan di klien) agar data tertanam (`__site`) tetap kecil.
2. **Label asal data di Inspector**: "Jam buka diambil dari Profil bisnis · Ubah" dengan tautan dalam; peringatan langsung di blok bila kosong (memakai `readiness.ts` yang sama).
3. **Bagian global** (setara section groups Shopify): blok yang dipasang sekali tampil di semua halaman — kandidat pertama *Bilah pengumuman* dengan jadwal mulai/selesai **[ADR]**.
4. **Template halaman produk** (setara JSON template per jenis resource Shopify): opsi terbatas, bukan kanvas bebas — tata letak foto (galeri/geser), tombol "Pesan via WA" berisi nama produk & varian, blok produk terkait (kategori sama), testimoni. Disimpan sebagai `pages["_product"]` **[ADR]**.
5. **Relasi antarplugin**: Promo → tautan ke produk/kategori tertentu (pemilih, bukan URL manual); FAQ → dipakai juga sebagai basis jawaban chatbot (satu sumber kebenaran); Testimoni → kelak dari ulasan pesanan terverifikasi.
6. **Relasi paket** (ADR-0019): saat plugin `pro` pertama hadir, tampilkan lencana "Pro" di katalog plugin dan perilaku turun-paket yang sudah diputuskan (tidak dihapus, tidak dirender).

## 7. Subdomain

- **Identitas yang dibagikan**: halaman/kartu "Bagikan toko" — tautan, **QR siap cetak** (stiker meja/etalase — UMKM banyak berjualan luring), gambar OG otomatis (logo + nama + warna merek; `site_head.rs` sudah menulis `og:image`).
- **Ganti subdomain** dengan pengalihan 301 dari nama lama selama masa tenggang; daftar nama terlarang (lihat butir terbuka "reserve ryasai.my.id subdomains" di catatan proyek) — perlu dicek apakah aturan TEN-004..006 (`crates/domain/src/tenant.rs`) sudah mencakup **belum terverifikasi**.
- **Halaman 404 bermerek** dan halaman "toko sedang tutup/libur" (relasi dengan Jam buka).
- Domain sendiri tetap di luar V1 (ADR-0008); kandidat fitur Pro (ADR-0019).

## 8. Ide plugin baru (diurutkan relevansi UMKM)

| Plugin | Placement | Relasi data | Paket usulan | Catatan |
|---|---|---|---|---|
| Bilah pengumuman (terjadwal) | global/embed | — | starter | Ramadan, Harbolnas, libur |
| FAQ | grid | dipakai chatbot | starter | Mengurangi chat berulang |
| Kategori produk | grid | katalog `category_id` | starter | Butuh rekomendasi #1 |
| Produk pilihan (1 produk besar) | grid | katalog | starter | "Produk unggulan minggu ini" |
| Cara bayar | grid | QRIS/GoPay (ADR-0004/0020) | starter | Jangan tampilkan QR tanpa konteks pesanan |
| Hitung mundur promo | grid | promo | pro | Pre-order/flash sale |
| Reservasi/janji temu | grid | jam buka, chatbot | pro | Jasa & salon; bisa lewat WA dulu |
| Video (YouTube/TikTok whitelist) | grid | — | pro | Perlu ubah CSP `frame-src` **[ADR]** |
| Formulir/pre-order | grid | CRM | pro | Memicu kewajiban UU PDP |

## 9. Daftar pustaka (diakses 2026-11-17)

1. Shopify Changelog — Horizon: 10 new free themes. https://changelog.shopify.com/posts/horizon-10-new-free-themes-by-shopify
2. Codrops — Shopify Summer '25 Edition Introduces Horizon (sekunder). https://tympanus.net/codrops/2025/05/21/shopify-summer-25-edition-introduces-horizon-a-new-standard-for-creative-control/
3. GemPages — Shopify Horizon Themes: Features, Limitations (sekunder). https://gempages.net/blogs/shopify/shopify-horizon-themes
4. Squarespace Help — Site styles. https://support.squarespace.com/hc/articles/205815788
5. Paige Brunton — Squarespace site styles (sekunder). https://www.paigebrunton.com/blog/squarespace-site-styles
6. Take App — WhatsApp store. https://www.take.app/whatsapp-store
7. Take App Changelog — More store templates and apps. https://docs.take.app/changelog/more-store-templates-and-apps
8. Kontan/MomsMoney — Lynk.id bidik babak baru ekonomi kreator (sekunder). https://momsmoney.kontan.co.id/news/platfrom-lynkid-bidik-babak-baru-ekonomi-kreator
9. Atmos — How to build a color system for UI design (sekunder). https://atmos.style/blog/how-to-build-a-color-system-for-ui-design
10. hexdocs `color` — A Guide to Color Palettes (tonal ≠ kontras merata). https://hexdocs.pm/color/palettes.html
11. B12 — Bento grids and kinetic typography (sekunder). https://orchestra.b12.io/resource-center/website-design/web-design-guide-bento-grids-and-kinetic-typography/
12. Chrome for Developers — Cross-document view transitions for MPAs. https://developer.chrome.com/docs/web-platform/view-transitions/cross-document
13. Can I use — View Transitions (cross-document). https://caniuse.com/cross-document-view-transitions
14. Web Features Explorer — Cross-document view transitions. https://web-platform-dx.github.io/web-features-explorer/features/cross-document-view-transitions
15. web.dev — New to the web platform in October 2026. https://web.dev/blog/web-platform-10-2026
16. WebKit — A guide to scroll-driven animations with just CSS. https://www.webkit.org/blog/17101/a-guide-to-scroll-driven-animations-with-just-css/
17. "Why Google's Material 3 update turns motion into a theme" (sekunder). https://www.besthub.dev/articles/why-google-s-material-3-update-turns-motion-into-a-theme-it-s-not-about-the-bounce-9436fce4a345
18. Shopify.dev — Dynamic sources. https://shopify.dev/docs/storefronts/themes/architecture/settings/dynamic-sources
