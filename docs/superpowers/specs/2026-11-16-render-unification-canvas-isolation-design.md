# Desain: Unifikasi Mesin Render Studio↔Publik dan Isolasi Kanvas Preview

**Tanggal:** 2026-11-16
**Status:** Disetujui
**Terkait:** ADR-0014, ADR-0017, ADR-0019

---

## 1. Latar Belakang & Masalah

Situs yang diterbitkan tidak selalu sama dengan yang terlihat di Studio. Sepanjang sesi 2026-11-15/16 ditemukan dua
kelas bug yang berasal dari akar yang sama: **Studio dan situs publik merender dokumen yang sama lewat dua jalur
kode berbeda**.

1. **Navigasi hilang setelah terbit.** `StudioSite` hanya merender bilah samping ketika `navbar_style = "side"`;
   gaya `"top"` (default) tidak dirender sama sekali. Markup `.s-nav-side` juga tidak punya CSS. Pratinjau Studio
   menampilkan `<StudioNavbar>` yang selalu ada, sehingga pratinjau menjanjikan sesuatu yang tidak pernah muncul.
2. **Elemen melayang bocor ke luar bingkai.** `ChatWidget`, tombol WhatsApp melayang, dan `CartFab` memakai
   `position: fixed` terhadap jendela browser. Di dalam kanvas Studio itu berarti elemen menempel ke layar, bukan
   ke bingkai pratinjau.
3. **Duplikasi renderer.** `studio/page.tsx` punya `renderCell`, `ChatPreview`, `FloatingChat`, dan `FloatingWa`
   sendiri, yang menyusun ulang tampilan dari `Site` per-sel. Setiap perubahan desain harus dikerjakan dua kali,
   dan keduanya perlahan menyimpang.

Akibatnya tidak ada jaminan struktural bahwa "preview = terbit". Perbaikannya bukan menambal tiap gejala,
melainkan **satu sumber render**.

---

## 2. Tujuan & Ruang Lingkup

### Tujuan

- Satu pohon komponen render untuk dokumen `schema_version = 2`, dipakai oleh situs publik **dan** kanvas Studio.
- Kanvas Studio akurat untuk lebar ponsel, tablet, dan desktop tanpa perlu mengecilkan jendela browser.
- Elemen melayang terkunci di dalam bingkai kanvas saat di Studio, tetapi tetap menempel ke jendela di situs publik.
- Semua data dari server maupun `localStorage` melewati satu fungsi normalisasi sebelum masuk renderer.

### Batasan (Non-Tujuan)

- **Tidak** membuat skema data baru. Wire format tetap `PageV2`; `crates/domain/src/website/schema_v2.rs` tetap
  sumber kebenaran dan tidak diubah.
- **Tidak** mengubah `site.css` untuk `position: fixed` — perilaku situs pelanggan tidak berubah sama sekali.
- **Tidak** mengonversi `@media (hover: hover)`, `@media (prefers-reduced-motion: reduce)`, dan
  `@media (prefers-color-scheme: dark)` pada chrome dasbor. Ketiganya tidak punya padanan `@container`, dan
  mengubahnya merusak aksesibilitas atau mewarnai ulang dasbor.
- **Tidak** mengerjakan Fase 4 (tiga panel 260/320 px, mode interaksi penuh, kontrol semantik) dan Fase 5
  (preset template baru). Keduanya sub-proyek terpisah.
- **Tidak** menghapus `Site.tsx`. Renderer v1 tetap dipakai untuk dokumen legacy `schema_version = 1`.

---

## 3. Keputusan Arsitektur

### 3.1. `StudioSite` diperluas dengan `mode`, bukan renderer baru

Alternatif yang ditolak:

- **`StorefrontRenderer` + `BlockRegistry` baru.** Sesuai struktur direktori usulan, tetapi menambah satu lapisan
  di atas renderer yang sudah teruji (grid 12 kolom, `hide_on`, anchor template, pemfilteran embed) dan berisiko
  regresi pada perilaku yang sudah punya tes.
- **Ekstrak helper saja.** Perubahan paling kecil, tetapi justru mempertahankan duplikasi yang menjadi akar bug.

Keputusan: `StudioSite` menjadi satu-satunya renderer v2, dengan prop `mode`. Paritas DOM menjadi konsekuensi
struktural, bukan hasil kerja manual yang harus dijaga.

### 3.2. Dua sub-mode kanvas

Drag/resize per-blok butuh matematika koordinat grid dan penanganan pointer (`Canvas.tsx`) yang tidak mungkin
ditumpangkan ke renderer asli tanpa membungkus tiap blok dalam elemen grid. Karena itu kanvas punya dua sub-mode
yang merender dokumen yang sama:

| Sub-mode | Renderer | Kegunaan |
|---|---|---|
| **Isi** (default) | `<StudioSite mode="studio-edit">` | Klik teks → ketik; klik blok → inspector |
| **Tata letak** | `<Canvas>` (grid drag/resize) | Geser & ubah ukuran blok |

Keduanya membaca `TplCell[]` yang sama. Tidak ada dokumen kedua.

### 3.3. Kontainmen elemen melayang lewat containing block

`position: sticky` **bukan** solusi: sticky tidak mengunci elemen ke bingkai pratinjau, dan mengubahnya akan
merusak situs pelanggan tempat `fixed` adalah perilaku yang benar.

Wrapper khusus Studio:

```css
.st-frame {
  container-type: inline-size;
  container-name: storefront;
  position: relative;
  isolation: isolate;
  transform: translateZ(0); /* menjadikan containing block untuk position:fixed */
}
```

Menurut spesifikasi CSS, `transform` (selain `none`) membuat elemen menjadi containing block bagi keturunan
`position: fixed`. Jadi elemen melayang terkunci ke bingkai kanvas **tanpa satu baris pun perubahan pada
`site.css`**. Di situs publik wrapper ini tidak dipasang, sehingga `fixed` kembali relatif ke jendela.

Catatan: `Canvas.tsx:127` sudah memakai `transform: scale(…)`, sehingga mode Tata letak sudah memiliki containing
block secara tidak sengaja. Wrapper `.st-frame` membuat perilaku itu eksplisit dan berlaku juga di mode Isi.

---

## 4. Rancangan Teknis

### 4.1. Normalizer (Fase 1, diperkecil)

Berkas baru `frontend/src/renderer/normalize.ts`:

```ts
/** Validasi + isi default PageV2 dari server, localStorage, atau state Studio. */
export function normalizePageV2(raw: unknown): PageV2;
```

Perilaku:

- `schema_version` dipaksa `2`; `template` di-`safeId` (maks 40 karakter).
- `blocks` difilter: `kind` harus ada di `PLUGIN_KINDS` (12 nilai), `style` di `STYLES` (4), `layout` di
  `layouts(kind)`, `hide_on` di `HIDE_ON` (2). Yang tidak valid dibuang, bukan melempar error.
- `x/y/w/h` di-clamp: `x >= 0`, `y >= 0`, `w >= 1`, `h >= 1`, `x + w <= 12`, `h <= 64`.
- `id` di-`safeId()`; id duplikat diberi sufiks.
- `theme.surface` default `"light"`, `text_scale` default `1`, `primary`/`font` default dari `safe.ts`.
  **Tidak ada key tema baru** — validator Rust memakai daftar key tertutup (`schema_v2.rs:190`) dan akan menolak
  `unknown_key` saat publish.
- `data` dipakai apa adanya (bentuknya sudah dijaga validator per plugin).

Titik masuk yang wajib memakai normalizer:

1. `frontend/src/app/site/page.tsx` — data dari `/public/site`.
2. `frontend/src/app/(studio)/studio/page.tsx` — hasil `fromDoc()` atas draf server.
3. `frontend/src/app/(studio)/studio/page.tsx` — dokumen dari `localStorage`.

Ini memenuhi guardrail "setiap data wajib lewat satu fungsi normalisasi sebelum masuk komponen render".

**Kenapa bukan `StorefrontSchema` terpisah:** wire format tetap `PageV2`. `crates/domain/tests/frontend_contract.rs`
sudah menjaga kontrak frontend↔Rust lewat `plugin-manifest.json`. Menambah lapisan tipe kedua berarti menambah
konversi bolak-balik yang harus dijaga tes paritasnya sendiri, tanpa manfaat yang sepadan.

### 4.2. `StudioSite` dengan mode

```ts
export interface StudioSiteProps {
  doc: PageV2;
  business: Business;
  status?: OpenStatus | null;
  products?: GridProduct[];
  assetUrl: (ref: string) => string | undefined;
  pages?: SitePageLink[];
  /** "customer" (default) = situs publik; "studio-edit" = kanvas editor. */
  mode?: "customer" | "studio-edit";
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
}
```

Perubahan internal:

- **Mode `studio-edit`**: setiap blok dibungkus `EditableBlockWrapper` (overlay seleksi, label jenis plugin,
  `data-interactive`) dan dipasangi `EditContext` sehingga `<T>` menjadi `contentEditable`.
- **Mode `customer`**: tanpa overlay, tanpa `EditContext` — perilaku hari ini tidak berubah.
- Grid, `hide_on`, anchor template, pemfilteran embed, footer, chat/WA melayang **tidak berubah**; hanya dibungkus.

Dihapus dari `frontend/src/app/(studio)/studio/page.tsx`: `renderCell`, `ChatPreview`, `FloatingChat`,
`FloatingWa`, dan pemakaian `Site` per-sel. Studio memanggil satu `<StudioSite mode="studio-edit">`.

`Site.tsx` tetap ada untuk dokumen v1 legacy (`schema_version: 1`), masih dipakai preview `app/(app)/website` dan
masih dicakup `frontend_contract.rs`.

### 4.3. Berkas baru

```
frontend/src/renderer/
├── normalize.ts               (BARU) normalizer PageV2
├── normalize.test.ts          (BARU)
├── StudioSite.tsx             (DIPERLUAS: mode, wrapper blok)
└── site.css                   (DIPERLUAS: .st-frame, container queries tersisa)

frontend/src/studio/
├── EditableBlockWrapper.tsx   (BARU) overlay seleksi khusus mode edit
└── DeviceFrame.tsx            (BARU) bingkai 390 / 768 / 100%
```

### 4.4. Viewport perangkat

`DeviceViewport = "mobile" | "tablet" | "desktop"` → lebar bingkai `390px`, `768px`, `100%`.

Karena `site.css` sudah memakai `@container` (7 aturan) dan `.sv2` sudah `container-type: inline-size`,
mengubah lebar bingkai otomatis mengubah jumlah kolom katalog **tanpa** mengubah ukuran jendela browser. Ini
kriteria lulus "verifikasi responsif kanvas".

### 4.5. Konversi `@media` yang tersisa

Sebelum implementasi ditemukan satu jebakan penting: **`.shop` dan `.chat` tidak boleh dijadikan container.**
`container-type: inline-size` menyiratkan `contain: layout`, yang menjadikan elemen tersebut containing block
bagi keturunan `position: fixed`. Karena `.shop-fab`, `.chat-fab`, `.chat-panel`, dan `.chat-bar` justru memakai
`fixed` di situs publik, menamai `.shop`/`.chat` sebagai container akan **merusak situs pelanggan**.

Karena itu pendekatannya dibalik: breakpoint default tetap `@media`, dan override khusus Studio ditambahkan
sebagai `@container storefront` yang hanya cocok di dalam `.st-frame`:

| Berkas | Perubahan |
|---|---|
| `chat.css` | `@media (min-width: 640px)` / `(max-width: 480px)` **dipertahankan** sebagai default situs publik, ditambah blok `@container storefront (...)` yang menimpanya di dalam bingkai Studio |
| `shop.css` | tidak berubah — `.shop` bukan container, sehingga `@container` tidak akan pernah cocok |

Batas 720 px dipilih karena sudah menjadi ambang mobile/desktop di `site.css`, sehingga grid dan chat tidak
berpindah breakpoint pada lebar yang berbeda.

**Dipertahankan tanpa perubahan:** `@media (hover: hover)` (site.css:117, 170), `@media (prefers-reduced-motion:
reduce)` (site.css:185, shop.css:113, chat.css:28), dan `@media (prefers-color-scheme: dark)` pada
`.st-rail`/`.st-group-card` (chrome dasbor).

---

## 5. Alur Data

```
API /public/site ─┐
localStorage ─────┼─> normalizePageV2() ─> PageV2 ─> <StudioSite mode> ─> DOM
draf server ──────┘                                      ▲
                                                         │
                                        EditableBlockWrapper (hanya studio-edit)
```

`StudioSite` menerima `PageV2` yang **sudah** ternormalisasi. Tidak ada komponen di bawahnya yang melakukan
validasi ulang atau mengisi default.

---

## 6. Penanganan Kesalahan

| Kondisi | Perilaku |
|---|---|
| `blocks` bukan array | Daftar kosong; kanvas menampilkan keadaan kosong, situs menampilkan nav + footer |
| `kind` tidak dikenal | Blok dibuang (fail-closed, sejalan dengan `plugin_plan()` di Rust) |
| `x/w` keluar grid | Di-clamp, bukan dibuang — pengguna tidak kehilangan blok |
| `theme` hilang | Default terang penuh |
| `id` duplikat | Sufiks `-2`, `-3`, … agar kunci React unik |
| `assetUrl` mengembalikan `undefined` | Gambar tidak dirender (perilaku `Img` saat ini) |

---

## 7. Rencana Pengujian

| Kriteria lulus plan | Cara verifikasi |
|---|---|
| Paritas DOM `studio-preview` vs `customer` | Render kedua mode dari dokumen yang sama, buang node overlay/`contentEditable`, bandingkan `tagName` + `className` + teks tiap blok |
| Responsif kanvas | Render tiga lebar bingkai, periksa `grid-template-columns` yang dihasilkan berubah tanpa mengubah viewport |
| Isolasi gaya | Render kanvas di dalam wrapper `data-theme="dark"`, pastikan warna latar kanvas tetap dari tema toko |
| Ganti template tidak hilang data | Perluas `templates.contract.test.ts`: produk + profil tetap ada setelah `applyTemplate` |
| `tsc --noEmit` tanpa `any` | Normalizer memakai `unknown` + type guard, bukan `any` |

Tes baru: `normalize.test.ts` (setiap aturan di §6), paritas DOM, dan tiga kasus viewport.

Tes regresi yang sudah ada dan wajib tetap lulus: `StudioSite.test.tsx` (11 kasus), `Site.test.tsx`,
`readiness.test.ts`, `templates.contract.test.ts`, serta `crates/domain/tests/frontend_contract.rs`.

---

## 8. Risiko & Mitigasi

| Risiko | Mitigasi |
|---|---|
| Menghapus `renderCell` menghilangkan perilaku kecil (mis. `st-style-*`, `st-layout-*`) | Pemetaan style/layout dipindah ke `StudioSite` dan dicakup tes paritas; dicek manual di ketiga viewport |
| `transform: translateZ(0)` memengaruhi posisi `sticky` di dalam kanvas | `.st-frame` hanya membungkus kanvas Studio; `sticky` header tetap bekerja karena containing block-nya juga berada di dalam wrapper |
| Konversi `@container` mengubah tampilan chat di situs publik | Ambang 640→720 hanya memindahkan titik pindah 80 px; diverifikasi di 390/768/1440 |
| `safeId` mengubah id blok sehingga anchor `#kontak` berubah | `ANCHORS` dihitung dari blok hasil normalisasi, jadi tetap konsisten |
| Blok `pro` hilang dari situs publik (sudah ada `hide_unentitled_blocks`) | Normalizer berjalan **setelah** filter server; tidak menambah/mengurangi blok |

---

## 9. Urutan Implementasi

1. `normalize.ts` + tes.
2. `EditableBlockWrapper.tsx` + `DeviceFrame.tsx`.
3. `StudioSite` mode + wrapper blok; tes paritas.
4. Hapus `renderCell`/`ChatPreview`/`FloatingChat`/`FloatingWa` dari `studio/page.tsx`; pasang dua sub-mode.
5. `.st-frame` + konversi `@container` di `shop.css`/`chat.css`.
6. Verifikasi: `tsc --noEmit`, `eslint`, `vitest`, `next build`, `cargo test -p umkm-domain`.

---

## 10. Yang Sengaja Ditunda

- `radiusScale`/`density` pada tema — butuh perubahan `schema_v2.rs` dan migrasi dokumen terbit.
- Tiga panel 260/320 px dan mode interaksi penuh — Fase 4.
- Preset template baru — Fase 5; `templates.ts` sudah punya 6 preset dan `applyTemplate` sudah mempertahankan data.
