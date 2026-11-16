import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  PagesPanel,
  confirmDeletePage,
  formatSlugPath,
  type PageItem,
} from "./PagesPanel";

describe("PagesPanel component (Studio Enhancement Task 5)", () => {
  const samplePages: PageItem[] = [
    { slug: "home", title: "Beranda", draft_revision: 3, published_version: 2 },
    { slug: "about", title: "Tentang Kami", draft_revision: 1, published_version: 1 },
    { slug: "promo", title: "Promo Spesial", draft_revision: 2, published_version: null },
  ];

  it("merender header dengan judul 'Halaman Website' dan deskripsi multi-halaman", () => {
    const html = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    expect(html).toContain("Halaman Website");
    expect(html).toMatch(/multi-halaman|navigasi/i);
    expect(html).toContain("Tambah Halaman");
  });

  it("merender daftar halaman dengan judul, path slug, dan badge 'Utama' untuk home", () => {
    const html = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    // Judul halaman
    expect(html).toContain("Beranda");
    expect(html).toContain("Tentang Kami");
    expect(html).toContain("Promo Spesial");

    // Path slug
    expect(html).toContain("/");
    expect(html).toContain("/about");
    expect(html).toContain("/promo");

    // Badge "Utama" hanya pada home
    expect(html).toContain("Utama");
  });

  it("menampilkan indikator aktif pada halaman yang sedang dipilih (activeSlug)", () => {
    const htmlHome = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );
    expect(htmlHome).toContain('data-active="true"');
    expect(htmlHome).toContain('aria-current="page"');

    const htmlAbout = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="about"
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );
    expect(htmlAbout).toContain('data-page-slug="about"');
  });

  it("tombol hapus dinonaktifkan (disabled) untuk halaman 'home'", () => {
    const html = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    // Tombol hapus untuk home harus disabled
    expect(html).toMatch(/data-delete-slug="home"[^>]*\sdisabled(=|[\s>])/);
    // Tombol hapus untuk about tidak boleh disabled
    expect(html).not.toMatch(/data-delete-slug="about"[^>]*\sdisabled(=|[\s>])/);
  });

  it("memformat slug path dengan benar melalui formatSlugPath helper", () => {
    expect(formatSlugPath("home")).toBe("/");
    expect(formatSlugPath("")).toBe("/");
    expect(formatSlugPath("about")).toBe("/about");
    expect(formatSlugPath("/kontak")).toBe("/kontak");
    expect(formatSlugPath("promo-spesial")).toBe("/promo-spesial");
  });

  it("confirmDeletePage mencegah penghapusan halaman 'home'", async () => {
    const onDeleteMock = vi.fn().mockResolvedValue(true);
    const confirmMock = vi.fn().mockResolvedValue(true);

    const homeItem: PageItem = { slug: "home", title: "Beranda" };
    const result = await confirmDeletePage(homeItem, onDeleteMock, confirmMock);

    expect(result).toBe(false);
    expect(confirmMock).not.toHaveBeenCalled();
    expect(onDeleteMock).not.toHaveBeenCalled();
  });

  it("confirmDeletePage memanggil confirm dialog dan menghapus halaman jika dikonfirmasi", async () => {
    const onDeleteMock = vi.fn().mockResolvedValue(true);
    const confirmMock = vi.fn().mockResolvedValue(true);

    const aboutItem: PageItem = { slug: "about", title: "Tentang Kami" };
    const result = await confirmDeletePage(aboutItem, onDeleteMock, confirmMock);

    expect(result).toBe(true);
    expect(confirmMock).toHaveBeenCalledTimes(1);
    expect(onDeleteMock).toHaveBeenCalledWith("about");
  });

  it("confirmDeletePage membatalkan penghapusan jika pengguna menolak di confirm dialog", async () => {
    const onDeleteMock = vi.fn().mockResolvedValue(true);
    const confirmMock = vi.fn().mockResolvedValue(false);

    const aboutItem: PageItem = { slug: "about", title: "Tentang Kami" };
    const result = await confirmDeletePage(aboutItem, onDeleteMock, confirmMock);

    expect(result).toBe(false);
    expect(confirmMock).toHaveBeenCalledTimes(1);
    expect(onDeleteMock).not.toHaveBeenCalled();
  });

  it("menampilkan preset halaman standar (Tentang Kami & Kontak) jika belum dibuat", () => {
    const pagesOnlyHome: PageItem[] = [{ slug: "home", title: "Beranda" }];

    const html = renderToStaticMarkup(
      <PagesPanel
        pages={pagesOnlyHome}
        activeSlug="home"
        defaultOpenAdd={true}
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    // Karena belum ada 'about' dan 'contact', preset keduanya tampil
    expect(html).toContain("Tentang Kami");
    expect(html).toContain("Kontak");
    expect(html).toContain("data-preset-slug=\"about\"");
    expect(html).toContain("data-preset-slug=\"contact\"");
  });

  it("menyembunyikan preset jika halaman standar sudah ada di daftar", () => {
    const pagesWithAbout: PageItem[] = [
      { slug: "home", title: "Beranda" },
      { slug: "about", title: "Tentang Kami" },
    ];

    const html = renderToStaticMarkup(
      <PagesPanel
        pages={pagesWithAbout}
        activeSlug="home"
        defaultOpenAdd={true}
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    // Preset 'about' sudah ada, jadi tidak muncul di preset quick-add
    expect(html).not.toContain("data-preset-slug=\"about\"");
    // Preset 'contact' belum ada, jadi tetap muncul
    expect(html).toContain("data-preset-slug=\"contact\"");
  });

  it("menampilkan pembatasan Starter (Paket Pro badge, pesan bantuan, upgrade CTA, input disabled)", () => {
    const html = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        defaultOpenAdd={true}
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={false}
        onUpgradeClick={() => {}}
      />
    );

    expect(html).toContain("Paket Pro");
    expect(html).toContain(
      "Halaman kustom memerlukan paket Pro. Starter mendapatkan Beranda, Tentang Kami, dan Kontak."
    );
    expect(html).toContain("Upgrade ke Pro");

    // Input custom page dinonaktifkan (disabled) pada paket Starter
    expect(html).toMatch(/data-testid="input-custom-title"[^>]*\sdisabled(=|[\s>])/);
    expect(html).toMatch(/data-testid="input-custom-slug"[^>]*\sdisabled(=|[\s>])/);
  });

  it("mengaktifkan input kustom secara penuh saat pengguna adalah Pro (isPro=true)", () => {
    const html = renderToStaticMarkup(
      <PagesPanel
        pages={samplePages}
        activeSlug="home"
        defaultOpenAdd={true}
        onSelectPage={() => {}}
        onCreatePage={async () => true}
        onDeletePage={async () => true}
        isPro={true}
        onUpgradeClick={() => {}}
      />
    );

    // Pada Pro, tidak ada pemblokiran upgrade CTA
    expect(html).not.toContain("Upgrade ke Pro");
    expect(html).not.toContain(
      "Halaman kustom memerlukan paket Pro. Starter mendapatkan Beranda, Tentang Kami, dan Kontak."
    );

    // Input kustom aktif (tidak disabled)
    expect(html).toContain('data-testid="input-custom-title"');
    expect(html).toContain('data-testid="input-custom-slug"');
    expect(html).not.toMatch(/data-testid="input-custom-title"[^>]*\sdisabled(=|[\s>])/);
    expect(html).not.toMatch(/data-testid="input-custom-slug"[^>]*\sdisabled(=|[\s>])/);

    // Tombol submit kustom aktif
    expect(html).toContain("Buat Halaman");
  });
});
