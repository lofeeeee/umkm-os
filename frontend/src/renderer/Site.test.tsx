import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { pageSlug } from "@/lib/slug";

import { safeColor, safeHref, waLink } from "./safe";
import { FIELDS, defaultData, newSectionId } from "./sections";
import { Site } from "./Site";
import { SECTION_TYPES, type Business, type PageContent, type Section, type SectionType } from "./types";

const business: Business = {
  profile: {
    name: "Toko <b>A</b>", socials: [{ platform: "instagram", url: "https://instagram.com/a" }],
    timezone: "Asia/Jakarta", pickup_available: true, delivery_available: false,
    phone: "+6281234567890", whatsapp: "081234567890", email: "a@toko.id",
  },
  addresses: [{ address: "Jl. Merdeka 1", latitude: -6.2, longitude: 106.8 }],
  hours: [{ day_of_week: 1, open: "08:00", close: "17:00" }],
  exceptions: [],
};

function page(sections: Section[], theme = { primary: "#2563EB", font: "inter" }): PageContent {
  return { schema_version: 1, template: "modern_store", theme, sections };
}

const ASSET = "asset:6f9619ff-8b86-d011-b42d-00c04fc964ff";
const urls = (r: string) => (r === ASSET ? "https://api.example/files/x?sig=1" : undefined);

function render(sections: Section[], preview = false, theme?: PageContent["theme"]) {
  return renderToStaticMarkup(<Site content={page(sections, theme)} business={business} assetUrl={urls} preview={preview} />);
}

/** Data default editor + satu foto galeri (galeri kosong tidak dirender di situs publik). */
const withPublicData = (t: SectionType) => (t === "gallery" ? { ...defaultData(t), images: [{ asset: ASSET, caption: "Foto" }] } : defaultData(t));

describe("renderer keamanan (AC-WEB-018, ADR-0007)", () => {
  it("teks berisi HTML/skrip dirender sebagai teks literal", () => {
    const html = render([
      { id: "hero-1", type: "hero", enabled: true, data: { title: "<script>alert(1)</script>", subtitle: "<img src=x onerror=alert(1)>" } },
      { id: "about-1", type: "about", enabled: true, data: { body: "<iframe src=//evil></iframe>" } },
      { id: "footer-1", type: "footer", enabled: true, data: {} },
    ]);
    expect(html).not.toMatch(/<script|<iframe|<img src=x/i);
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("&lt;iframe");
    expect(html).toContain("© Toko &lt;b&gt;A&lt;/b&gt;"); // nama bisnis juga di-escape
    expect(html).not.toContain("<b>A</b>");
  });

  it("tautan berbahaya tidak pernah menjadi href", () => {
    const html = render([
      { id: "hero-1", type: "hero", enabled: true, data: { title: "x", cta: { label: "Klik", href: "javascript:alert(1)" } } },
      { id: "header-1", type: "header", enabled: true, data: { title: "x", nav: [{ label: "a", href: "data:text/html,x" }, { label: "b", href: "//evil.example" }] } },
    ]);
    expect(html).not.toMatch(/href="(javascript|data):/i);
    expect(html).not.toContain('href="//evil');
    expect(html).toContain("<span class=\"s-btn\">Klik</span>");
  });

  it("warna tema tidak valid diganti default (tidak bisa menyuntik CSS)", () => {
    const html = render([], false, { primary: "red;background:url(//evil)", font: "x" });
    expect(html).not.toContain("evil");
    expect(html).toContain("--s-primary:#2563EB");
  });

  it("tautan eksternal memakai rel noopener", () => {
    const html = render([{ id: "contact-1", type: "contact", enabled: true, data: {} }]);
    expect(html).toContain('href="https://instagram.com/a" target="_blank" rel="noopener noreferrer nofollow"');
  });

  it("sumber tidak memakai dangerouslySetInnerHTML", () => {
    const root = join(__dirname, "..");
    const walk = (d: string): string[] => readdirSync(d).flatMap((f) => {
      const p = join(d, f);
      return statSync(p).isDirectory() ? walk(p) : [p];
    });
    const offenders = walk(root).filter((f) => /\.(t|j)sx?$/.test(f) && !f.endsWith(".test.tsx"))
      .filter((f) => /dangerouslySetInnerHTML\s*[=:]/.test(readFileSync(f, "utf8")));
    expect(offenders).toEqual([]);
  });
});

describe("renderer perilaku", () => {
  it("section nonaktif tidak dirender di publik, tampil berpenanda di preview (AC-WEB-008/011)", () => {
    const s: Section[] = [
      { id: "about-1", type: "about", enabled: false, data: { body: "Rahasia draft" } },
      { id: "footer-1", type: "footer", enabled: true, data: { text: "Kaki" } },
    ];
    expect(render(s)).not.toContain("Rahasia draft");
    const pv = render(s, true);
    expect(pv).toContain("Rahasia draft");
    expect(pv).toContain("Nonaktif — tidak tampil di publik");
  });

  it("kelas section tidak bertabrakan dengan blok di dalamnya (regresi layout desktop)", () => {
    const html = render(SECTION_TYPES.map((t, i) => ({ id: `s-${i}`, type: t, enabled: true, data: withPublicData(t) })));
    for (const t of SECTION_TYPES) expect(html).toContain(`class="s s--${t}"`);
    expect(html).not.toMatch(/class="s s-[a-z]/);
    // Tanpa gambar: hero satu kolom; dengan gambar: has-media
    expect(html).toContain('class="s-hero"');
    expect(render([{ id: "h", type: "hero", enabled: true, data: { title: "x", image: ASSET } }])).toContain('class="s-hero has-media"');
    expect(render([{ id: "c", type: "contact", enabled: true, data: {} }].map((x) => x as Section))).toContain("s-social");
  });

  it("urutan section mengikuti array (AC-WEB-007)", () => {
    const html = render([
      { id: "footer-1", type: "footer", enabled: true, data: { text: "PERTAMA" } },
      { id: "hero-1", type: "hero", enabled: true, data: { title: "KEDUA" } },
    ]);
    expect(html.indexOf("PERTAMA")).toBeLessThan(html.indexOf("KEDUA"));
  });

  it("gambar hanya dari peta aset bertanda tangan", () => {
    const html = render([
      { id: "hero-1", type: "hero", enabled: true, data: { title: "x", image: ASSET } },
      { id: "about-1", type: "about", enabled: true, data: { body: "y", image: "asset:00000000-0000-0000-0000-000000000000" } },
    ]);
    expect(html.match(/<img /g)?.length).toBe(1);
    expect(html).toContain('src="https://api.example/files/x?sig=1"');
  });

  it("11 tipe section dirender tanpa error dengan data default editor", () => {
    const sections = SECTION_TYPES.map((t, i) => ({ id: `s-${i}`, type: t, enabled: true, data: withPublicData(t) }));
    const html = render(sections);
    for (const t of SECTION_TYPES) expect(html).toContain(`data-section-type="${t}"`);
    expect(html).toContain("Senin");
    expect(html).toContain("Jl. Merdeka 1");
    expect(html).toContain("https://wa.me/6281234567890");
  });

  // Renderer v1 (warisan) selalu memakai bilah atas: gaya `side` di v1 tidak pernah punya CSS/markup
  // yang berfungsi. Dokumen baru (Studio, schema_version 2) memakai StudioSite yang mendukung keduanya.
  it("header v1 selalu jadi bilah atas berisi tautan halaman", () => {
    const html = render(
      [{ id: "h-1", type: "header", enabled: true, data: { title: "Toko Side", nav: [{ label: "Menu", href: "#menu" }] } }],
      false,
      { primary: "#2563EB", font: "inter", navbar_style: "side" } as any,
    );
    expect(html).toContain("s-headbar");
    expect(html).toContain('href="#menu"');
    expect(html).toContain("Toko Side");
  });

  it("navbar_style top dengan navbar_sticky me-render s-sticky", () => {
    const html = render(
      [{ id: "h-1", type: "header", enabled: true, data: { title: "Toko Top" } }],
      false,
      { primary: "#2563EB", font: "inter", navbar_style: "top", navbar_sticky: true } as any,
    );
    expect(html).toContain("s-sticky");
  });

  it("mendukung background CSS variables dan overlay pada .site", () => {
    const html = render([], false, {
      primary: "#2563EB",
      font: "inter",
      background_image: ASSET,
      background_size: "contain",
      background_position: "top center",
      background_opacity: 75,
    } as any);
    expect(html).toContain("--s-bg-img:url(&quot;https://api.example/files/x?sig=1&quot;)");
    expect(html).toContain("--s-bg-size:contain");
    expect(html).toContain("--s-bg-pos:top center");
    expect(html).toContain("--s-bg-opacity:0.75");
    expect(html).toContain("s-bg-overlay");
  });

  it("mendukung image_fit dan image_position pada Img", () => {
    const html = render([
      {
        id: "hero-1",
        type: "hero",
        enabled: true,
        data: {
          title: "Hero Fit",
          image: ASSET,
          image_fit: "contain",
          image_position: "top left",
        },
      },
    ]);
    expect(html).toContain("object-fit:contain");
    expect(html).toContain("object-position:top left");
  });
});

describe("helper", () => {
  it("safeHref sama dengan aturan Rust", () => {
    for (const ok of ["https://x.id/a?b=1", "http://x.id", "/produk", "#hero-1", "mailto:a@b.id", "tel:+62-812"]) expect(safeHref(ok)).toBe(ok);
    for (const bad of ["javascript:alert(1)", "JavaScript:x", "data:x", "//evil", "https://user@evil", " https://x", "https://", "java\tscript:x", "#Bad"])
      expect(safeHref(bad)).toBeUndefined();
  });
  it("safeColor & waLink", () => {
    expect(safeColor("#abcdef")).toBe("#abcdef");
    expect(safeColor("red")).toBe("#2563EB");
    expect(waLink("0812-345-678")).toBe("https://wa.me/62812345678");
    expect(waLink("12")).toBeUndefined();
  });
  it("id section baru unik dan berpola valid", () => {
    const ex: Section[] = [{ id: "product-grid-1", type: "product_grid", enabled: true, data: {} }];
    expect(newSectionId("product_grid", ex)).toBe("product-grid-2");
    expect(newSectionId("hero", ex)).toMatch(/^[a-z0-9-]{1,40}$/);
  });
  it("spesifikasi field mencakup semua tipe", () => {
    expect(Object.keys(FIELDS).sort()).toEqual([...SECTION_TYPES].sort());
  });
  it("slug halaman publik", () => {
    expect(pageSlug("/")).toBe("home");
    expect(pageSlug("/promo")).toBe("promo");
    expect(pageSlug("/../etc")).toBeNull();
    expect(pageSlug("/A")).toBeNull();
  });

  it("situs publik menyembunyikan bagian tanpa data toko; pratinjau editor tetap menampilkannya", () => {
    const empty: Business = { ...business, profile: { ...business.profile, phone: null, whatsapp: null, email: null, socials: [] }, addresses: [], hours: [] };
    const sections: Section[] = (["business_hours", "location", "contact", "gallery", "about"] as const).map((t, i) => ({ id: `e-${i}`, type: t, enabled: true, data: defaultData(t) }));
    const pub = renderToStaticMarkup(<Site content={page(sections)} business={empty} assetUrl={() => undefined} />);
    for (const t of ["business_hours", "location", "contact", "gallery"]) expect(pub).not.toContain(`data-section-type="${t}"`);
    expect(pub).not.toContain("Tutup sekarang");
    expect(pub).toContain('data-section-type="about"');
    const pv = renderToStaticMarkup(<Site content={page(sections)} business={empty} assetUrl={() => undefined} preview />);
    expect(pv).toContain('data-section-type="business_hours"');
  });
});
