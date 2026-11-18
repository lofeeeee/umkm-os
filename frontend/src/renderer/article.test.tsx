import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ArticleBody, BlogList, parseArticle } from "./article";

describe("isi artikel (penanda ringan yang aman)", () => {
  it("mengurai judul, daftar, kutipan, paragraf; id judul unik", () => {
    const n = parseArticle("## Alat\nSiapkan **V60**.\n\n- kertas\n- gelas\n\n1. bilas\n2. seduh\n\n> panas 92°C\n\n## Alat");
    expect(n.map((x) => x.t)).toEqual(["h2", "p", "ul", "ol", "quote", "h2"]);
    expect(n.filter((x) => x.t === "h2").map((x) => (x as { id: string }).id)).toEqual(["alat", "alat-2"]);
  });
  it("tidak pernah merender HTML mentah; tautan berbahaya jadi teks", () => {
    const html = renderToStaticMarkup(<ArticleBody body={'<script>alert(1)</script> [klik](javascript:alert(1)) [ok](/produk)'} toc={false} />);
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain("javascript:");
    expect(html).toContain('<a href="/produk">ok</a>');
  });
  it("kepala artikel: tag, judul, ringkasan, penulis, waktu baca, daftar isi", () => {
    const html = renderToStaticMarkup(<ArticleBody body={"## Satu\nx\n## Dua\ny"} toc title="Cara seduh" meta={{ excerpt: "Ringkas", author: "Rina", published_at: "2026-11-01", tags: ["kopi"] }} />);
    expect(html).toContain("<h1>Cara seduh</h1>");
    expect(html).toContain('<time dateTime="2026-11-01">');
    expect(html).toContain("1 menit baca");
    expect(html).toContain('<a href="#satu">Satu</a>');
  });
  it("daftar artikel urut terbaru, contoh hanya di editor", () => {
    const entries = [
      { slug: "lama", title: "Lama", article: { published_at: "2026-01-01" } },
      { slug: "baru", title: "Baru", article: { published_at: "2026-11-01", excerpt: "Hai" } },
    ];
    const html = renderToStaticMarkup(<BlogList entries={entries} limit={6} layout="grid" assetUrl={() => undefined} />);
    expect(html.indexOf("Baru")).toBeLessThan(html.indexOf("Lama"));
    expect(html).toContain('href="/baru"');
    expect(renderToStaticMarkup(<BlogList entries={[]} limit={3} layout="list" assetUrl={() => undefined} />)).toContain("Belum ada artikel.");
    expect(renderToStaticMarkup(<BlogList entries={[]} limit={3} layout="list" assetUrl={() => undefined} editing />)).toContain("Judul artikel contoh 1");
  });
});
