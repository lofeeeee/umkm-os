# feat(studio): 10 palet tema + token WCAG onPrimary, terintegrasi dengan fondasi multi-page/navbar

## Ringkasan

Merge request ini menggabungkan dua jalur kerja menjadi satu:

1. **Fondasi upstream** (`ryasrk/umkm-os` main) — studio multi-page, navbar top/side,
   theme panel, latar halaman, canvas draw layers, dan crop modal (~6.400 baris,
   26 file, termasuk schema_v2 multi-page API di sisi Rust).
2. **Pekerjaan palet kami** — sistem tema dibangun ulang di atas token kontras aman
   (WCAG AA), 10 palet baru, dan dokumentasi audit UI lengkap (`step_1.md`).

Semua konflik merge telah diselesaikan; cabang ini sudah berisi merge upstream
sehingga siap digabung tanpa konflik.

## Perubahan utama

### 1. Palet tema (18 total, dari 8)

- `Palette` kini membawa **`onPrimary`** — warna teks di atas warna utama, tidak lagi
  diasumsikan putih. Emas memakai teks gelap (putih di atas `#CA8A04` hanya ~2.2:1).
- **10 palet baru:** Sawi, Terong, Bata, Sian, Kuningan, Lumut, Abu, Merah, Toska, Koral.
- **Re-tune palet lama** yang bentrok rona/madu kontras — semua diverifikasi numerik:
  Kopi `#5C2A0D`, Laut `#075C9E`, Mawar `#A61D6D`, Malam `#7C3AED` (dulu lavender
  pastel yang bobot visualnya beda dari tema lain).

### 2. Pengujian permanen (mencegah regresi)

- `contrast.test.ts` — setiap pasangan `primary`/`onPrimary` wajib ≥ 4.5:1 (WCAG AA).
- `plugins.test.ts` — dua palet kromatik dilarang terlalu mirip
  (Δhue < 12° **sekaligus** ΔL < 15; palet netral dikecualikan).
- Kedua test ini menangkap dan memaksa perbaikan **9 pelanggaran nyata** selama
  pengerjaan — bukan test kosmetik.

### 3. UI swatch

- Chip **"Aa"** pada swatch menampilkan warna teks palet sesungguhnya.
- Cincin seleksi **netral** (`--ink`) menggantikan `--brand` — memperbaiki cincin biru
  di atas palet hijau Daun.

### 4. Kompatibilitas dokumen

- `onPrimary` **tidak** disimpan ke server (skema belum mengenal field itu).
  `doc.ts` memakai **derive-on-read**: cocokkan `PALETTES`, selain itu `onColor()`.
  Dokumen lama tetap aman tanpa migrasi.

### 5. Integrasi dengan ThemePanel baru upstream

- `ThemePanel.tsx` meneruskan `--s-on`, sehingga chip "Aa" + cincin netral otomatis
  aktif di panel desain baru tanpa CSS tambahan.

## Resolusi konflik

| File | Keputusan |
|---|---|
| `studio/doc.ts` | **Digabung**: field multi-page upstream (`navbarStyle`, `background*`, `drawings`) + fallback `onPrimary` kami |
| `studio/page.tsx` | **Upstream dipakai**: galeri plugin terkategori; UI warna pindah ke `ThemePanel` |

## Verifikasi

- `tsc --noEmit` — bersih
- Test suite — **111/112** lolos
  - 1 kegagalan: `templates.contract` (mismatch CRLF fixture) — **gagal juga di
    upstream/main murni**, bukan efek merge ini
- Manual QA di 360px/768px/1440px direkomendasikan setelah merge (lihat `step_1.md` §1)

## Dokumentasi

`step_1.md` (baru, ~650 baris) berisi audit lengkap: responsivitas mobile (drawer,
toolbar, FAB overlap), audit animasi (pelanggaran kontrak motion di jalur drag),
audit palet, rencana perbaikan dengan CSS siap pakai, dan log implementasi —
peta langkah berikutnya (bottom-sheet mobile, kolaps toolbar, navbar 4-posisi).

## Catatan untuk reviewer

- `#seo-description` focus target di `page.tsx:991` kini menggantung (upstream
  menghapus seksi SEO dari drawer) — perbaikan kecil menyusul, bukan blocker.
- Skema server belum memvalidasi `onPrimary` — derive-on-read disengaja sampai
  ADR disetujui; lihat header `page.tsx` untuk konteks status prototipe.
