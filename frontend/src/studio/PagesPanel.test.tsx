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

    expect(html).toContain("Halaman");
    expect(html).toMatch(/multi-halaman|navigasi/i);
    expect(html).toContain("Tambah halaman");
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

  const panel = (pages: PageItem[], isPro: boolean) => renderToStaticMarkup(
    <PagesPanel pages={pages} activeSlug="home" defaultOpenAdd={true} onSelectPage={() => {}}
      onCreatePage={async () => true} onDeletePage={async () => true} isPro={isPro} onUpgradeClick={() => {}} />,
  );

  it("standar: Halaman 1 = Beranda, halaman berikutnya bernama bebas (Starter juga)", () => {
    const html = panel([{ slug: "promo", title: "Promo Spesial" }, { slug: "home", title: "Home" }], false);
    expect(html.indexOf("Halaman 1")).toBeLessThan(html.indexOf("Beranda"));
    expect(html.indexOf("Beranda")).toBeLessThan(html.indexOf("Promo Spesial"));
    expect(html).toContain("Halaman 3");
    expect(html).toContain("2 dari 5 halaman");
    expect(html).not.toMatch(/data-testid="input-custom-title"[^>]*\sdisabled(=|[\s>])/);
    expect(html).toContain("Tentang Kami"); // saran nama
    expect(html).not.toContain("Tingkatkan ke Pro");
  });

  it("Starter penuh di 5 halaman → ajakan naik paket; Pro sampai 30", () => {
    const five: PageItem[] = ["home", "a", "b", "c", "d"].map((slug) => ({ slug, title: slug }));
    const starter = panel(five, false);
    expect(starter).toContain("5 dari 5 halaman");
    expect(starter).toContain("Paket Starter maksimal 5 halaman");
    expect(starter).toContain("Tingkatkan ke Pro");
    expect(starter).not.toContain("input-custom-title");
    const pro = panel(five, true);
    expect(pro).toContain("5 dari 30 halaman");
    expect(pro).toContain("input-custom-title");
  });
});
