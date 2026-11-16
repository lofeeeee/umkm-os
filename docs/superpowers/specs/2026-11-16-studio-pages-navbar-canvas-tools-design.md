# Desain: Multi-Page Studio, Navbar Top/Side, Background In-Through, Brush/Eraser, dan Pengelompokan Tools Kompak

**Tanggal:** 2026-11-16  
**Status:** Disetujui  
**Terkait:** ADR-0014, ADR-0017, ADR-0019

---

## 1. Latar Belakang & Masalah

Studio website UMKM OS saat ini mengusung konsep kanvas grid 12-kolom dengan katalog plugin resmi (ADR-0017, ADR-0019). Namun, terdapat beberapa keterbatasan operasional dan ekspresi visual yang dilaporkan oleh pemilik toko:
1. **Hanya Single-Page (Beranda saja):** Studio hanya menyunting halaman `home` draf. Toko membutuhkan struktur multi-halaman standar (Beranda, Tentang Kami, Kontak) untuk paket Starter, serta kebebasan membuat halaman kustom untuk paket Pro.
2. **Keterbatasan Navigasi:** Hanya tersedia header atas default tanpa opsi navigasi samping (*side navbar*) yang populer untuk toko modern/katalog luas.
3. **Latar Belakang Monoton & Kaku:** Kanvas hanya mendukung warna polos tanpa kemampuan mengunggah foto latar, mengatur ukuran/posisi, atau memberi efek tembus pandang (*in-through*) antar-blok.
4. **Manipulasi Gambar Kurang Fleksibel:** Foto dalam plugin sering kali terpotong secara otomatis tanpa kendala mode penuh (*contain*), reposisi titik fokus (*focal point*), atau pemotongan (*crop*).
5. **Ketiadaan Alat Kreatif Bebas:** Tidak ada fitur kuas (*brush*) untuk memberikan coretan aksen, stabilo, panah, atau dekorasi bebas di latar maupun di atas plugin.
6. **Bilah Alat (Tools/Plugins) Panjang & Menumpuk:** Seluruh plugin, palet warna, font, dan SEO berada di satu panel vertikal panjang tanpa pengelompokan yang ringkas.

---

## 2. Tujuan & Ruang Lingkup

### Tujuan
- Menghadirkan bilah kerja kompak (*Modular Studio Suite*) berbasis *icon rail* (56px) dan panel laci (*flyout drawer*) yang dikelompokkan berdasarkan kategori.
- Menyediakan manajemen Multi-Halaman (*Pages*) dengan diferensiasi paket Starter (3 halaman standar) vs Pro (halaman kustom tak terbatas).
- Menyediakan opsi Navigasi: **Bilah Atas (Top Bar)** dan **Bilah Samping (Side Bar)** responsif.
- Mengintegrasikan latar belakang kustom (*custom background image*) dengan pengaturan skala, posisi, dan efek tembus pandang (*in-through opacity/blur*).
- Menghadirkan kontrol gambar lengkap pada plugin: pilihan *Cover* vs *Contain*, reposisi titik fokus (*object-position*), dan alat potong (*crop modal*).
- Menyediakan alat coretan bebas (*Brush* & *Eraser*) dengan pilihan target lapisan (*Background-only* atau menempel pada plugin tertentu).

### Batasan (Non-Tujuan)
- Tidak mengizinkan eksekusi JavaScript/HTML sembarangan dari pihak ketiga (tetap patuh pada whitelist keamanan ADR-0014 dan CSP).
- Coretan disimpan sebagai kurva vektor SVG terstruktur, bukan bitmap raster biner berat.
- Tidak mengubah logika keranjang belanja atau checkout toko.

---

## 3. Arsitektur Workspace Kompak (*Modular Studio Suite*)

### 3.1. Icon Rail (Sisi Paling Kiri, 56px)
Bilah navigasi vertikal ramping tetap di sisi kiri Studio dengan 4 tombol ikon utama:
1. 📄 **Halaman (`pages`):** Menampilkan dan mengelola halaman toko.
2. 🧩 **Elemen & Plugin (`elements`):** Menampilkan katalog plugin resmi yang dikelompokkan rapi.
3. 🎨 **Desain & Latar (`theme`):** Warna utama, permukaan latar (*surface*), font, background gambar & efek in-through, serta gaya navbar.
4. ✏️ **Coretan (`draw`):** Kuas gambar, ukuran, warna, penghapus, dan target lapisan.

Tombol aktif memiliki indikator kontras tinggi. Mengklik ikon aktif akan menciutkan (*collapse*) laci panel untuk memaksimalkan area pandang kanvas.

### 3.2. Grouping Plugin Ringkas (Kategori Kartu 2-Kolom)
Pada tab **Elemen & Plugin**, plugin disajikan dalam kartu mini kompak dengan ikon tematik, judul tebal, deskripsi singkat, dan tombol pasang/tambah. Dikelompokkan ke dalam 4 kategori terpisah:
- **Konten:** Banner Hero, Tentang Kami, Galeri Foto, Testimoni, Teks Bebas.
- **Jualan:** Galeri Produk, Kotak Promo, Tombol WhatsApp.
- **Layanan:** Chatbot AI.
- **Info Toko:** Jam Buka, Lokasi Alamat, Kontak & Media Sosial.

---

## 4. Spesifikasi Fitur Detail

### 4.1. Manajemen Halaman (*Multi-Page*) & Pembatasan Paket
- **Tampilan Daftar Halaman:**
  - Menampilkan daftar halaman toko: nama judul, slug URL, dan status terbit.
  - Memilih halaman akan otomatis menyimpan draf halaman aktif, lalu memuat kanvas untuk halaman yang dipilih.
  - Setiap halaman memiliki pengaturan mandiri: Judul Halaman, Slug URL, Meta Title Google (maks 70 karakter), dan Meta Description Google (maks 160 karakter).
- **Aturan Pembatasan Paket (Tiering):**
  - **Paket Starter:**
    - Mendapatkan **3 halaman standar**: `Beranda` (`/`), `Tentang Kami` (`/tentang`), dan `Kontak` (`/kontak`).
    - Tombol `+ Tambah Halaman` menampilkan lencana *Pro*. Jika ditekan, muncul dialog edukasi upgrade ke paket Pro.
  - **Paket Pro:**
    - Dapat menambah halaman kustom baru tanpa batas (slug bebas seperti `/katalog-khusus`, `/galeri-karya`, `/faq`).
    - Opsi menduplikasi halaman yang sudah ada.
  - **Invarian Downgrade:** Halaman kustom tidak pernah dihapus jika toko turun ke paket Starter; halaman tersebut hanya dinonaktifkan dari publikasi website publik sampai toko kembali mengaktifkan Pro.

### 4.2. Navigasi Website (Top Bar vs Side Bar)
Pengaturan terletak di tab **Desain & Latar**:
- **Pilihan Tipe Navbar:**
  - `top` (Default): Bilah navigasi horizontal klasik di bagian paling atas. Dilengkapi toggle `sticky` (tetap menempel di atas saat layar digulir).
  - `side`: Bilah navigasi vertikal samping di sisi kiri layar desktop toko, memuat logo, nama toko, navigasi menu, dan tombol kontak. Pada layar ponsel (< 768px), otomatis bertransformasi menjadi header ramping dengan ikon hamburger menu / drawer.
- **Tautan Navigasi Otomatis:**
  - Otomatis mencantumkan halaman-halaman aktif toko.
  - Opsi menambahkan tautan eksternal khusus (misal tautan marketplace atau nomor telepon).

### 4.3. Background Laman & Efek "In-Through"
- **Unggah Gambar Latar Laman:**
  - Mendukung PNG, JPEG, dan WebP lewat komponen `useUploads`.
- **Mode Ukuran & Posisi:**
  - `size`: `cover` (penuh menutup kanvas), `contain` (tampil proporsional utuh), atau `repeat` (motif pola berulang).
  - `position`: `center`, `top`, `bottom`, `left`, `right`.
- **Efek Tembus Pandang (*In-Through*):**
  - Slider `overlay_opacity` (0% hingga 90%) untuk memberi lapisan warna semi-transparan di atas foto latar.
  - Gaya blok *card* dan *tint* di kanvas mendapatkan latar semi-transparan dengan efek *backdrop-filter: blur(8px)*, sehingga motif atau gambar latar belakang terlihat menembus sela-sela konten website.

### 4.4. Kontrol Gambar pada Plugin (Contain, Reposisi, dan Crop)
Pada setiap plugin yang memiliki aset foto utama (Banner Hero, Tentang Kami, Promo, Galeri):
1. **Mode Tampilan Foto:**
   - `cover`: Memenuhi kotak foto secara penuh dengan pemotongan estetis.
   - `contain`: Menampilkan keseluruhan foto tanpa terpotong (menjaga rasio asli).
2. **Titik Fokus Reposisi (`focal_point`):**
   - Pemilih titik 9 kuadran (*top-left, top-center, top-right, center-left, center, center-right, bottom-left, bottom-center, bottom-right*) yang mengontrol properti CSS `object-position`.
3. **Alat Potong (*Crop Modal*):**
   - Tombol "Potong Foto" di toolbar/inspector plugin membuka modal pemotong foto interaktif.
   - Pilihan preset rasio: Asli, 1:1 (Kotak), 16:9 (Banner), 4:3, atau Bebas (*Free*).
   - Menghasilkan foto baru yang diunggah ke storage toko dan otomatis dipasangkan ke plugin terkait.

### 4.5. Fitur Coretan Bebas (*Brush & Eraser Tooling*)
- **Alat Kuas (*Brush*):**
  - Mengubah kursor menjadi kuas gambar saat berada di atas kanvas.
  - Pilihan ketebalan: Halus (`2px`), Standar (`4px`), Tebal (`8px`), Penanda/Stabilo (`18px`).
  - Pilihan warna: Warna utama tema toko, warna hitam/putih, atau warna kustom hex.
  - Pilihan opasitas: 100% (Solid) atau 40% (Highlighter transparan).
- **Alat Penghapus (*Eraser*):**
  - Mode penghapus per-goresan (*stroke eraser*) atau tombol cepat "Hapus Semua Coretan".
- **Target Lapisan Coretan (*Target Layer*):**
  - **Latar Belakang Saja (`background`):** Coretan dirender pada lapisan SVG di bawah blok grid konten, menempel pada background kanvas.
  - **Terikat ke Plugin Tertentu (`cell:<cell_id>`):** Coretan menempel pada koordinat lokal plugin terpilih (misal melingkari tombol CTA pada Banner Hero). Jika plugin dipindah atau diubah ukurannya di grid, coretan ikut bergerak secara proporsional.
- **Representasi Data:**
  - Disimpan sebagai array kurva vektor SVG pada skema halaman:
    ```typescript
    interface DrawingStroke {
      id: string;
      target: "background" | string; // 'background' atau cell.id
      color: string;
      size: number;
      opacity: number;
      points: Array<[number, number]>; // koordinat titik relatif
    }
    ```
  - Dirender menggunakan elemen `<svg>` vektor murni dengan atribut `stroke-linecap="round"` dan `stroke-linejoin="round"`.

---

## 5. Kontrak Data & Skema Backend

Dokumen `PageV2` (pada `crates/domain/src/website/schema_v2.rs` dan `frontend/src/studio/doc.ts`) diperluas secara non-breaking:

```typescript
export interface PageV2 {
  schema_version: 2;
  template: string;
  theme: {
    primary: string;
    font: string;
    surface?: "light" | "cream" | "dark";
    text_scale?: number;
    navbar_style?: "top" | "side";
    navbar_sticky?: boolean;
    background_image?: string; // asset:<uuid>
    background_size?: "cover" | "contain" | "repeat";
    background_position?: string;
    background_opacity?: number; // 0..100
  };
  seo?: StudioSeo;
  blocks: BlockV2[];
  drawings?: DrawingStroke[];
}
```

Batas validasi backend:
- `drawings`: Maksimal 200 goresan per halaman untuk menjaga ukuran dokumen di bawah batas aman (256 KiB).
- `points`: Koordinat titik dibatasi integer positif relatif terhadap lebar desain (1080px).
- `navbar_style`: Whitelist `"top" | "side"`.
- `background_size`: Whitelist `"cover" | "contain" | "repeat"`.

---

## 6. Verifikasi & Pengujian

1. **Uji Frontend:**
   - Unit test vitest untuk manipulasi goresan kuas, pengelompokan plugin, dan pembatasan halaman Starter vs Pro.
   - Linting `npm run lint` dan typecheck `npm run typecheck` bebas dari error.
   - Build statis Next.js `npm run build` berhasil tanpa pelanggaran CSP.
2. **Uji Kontrak Backend:**
   - Unit test Rust `cargo test -p umkm-domain` untuk memvalidasi penambahan field opsional di `schema_v2.rs`.
   - Menjamin bahwa dokumen lama (tanpa `drawings` atau `background_image`) tetap valid dan kompatibel 100%.
3. **Uji Interaksi Visual:**
   - Memastikan goresan kuas lapisan background tetap berada di balik blok konten.
   - Memastikan coretan pada plugin ikut bergeser saat plugin di-drag di kanvas grid.
   - Memastikan navbar side berubah menjadi mobile drawer responsif pada layar sempit.
