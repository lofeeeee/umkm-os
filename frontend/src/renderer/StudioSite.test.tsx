import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { PageV2 } from "@/studio/doc";

import { StudioSite, mobileSorted, whatsappHref } from "./StudioSite";
import type { Business } from "./types";

const biz: Business = { profile: { name: "Kopi", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false, whatsapp: "0812-3456-789" }, addresses: [], hours: [], exceptions: [] };
const doc: PageV2 = {
  schema_version: 2, template: "t", theme: { primary: "#C2410C", font: "inter", surface: "cream", text_scale: 0.9 },
  blocks: [
    { id: "chat", kind: "chatbot", x: 8, y: 0, w: 4, h: 9, style: "card", layout: "bar", data: { title: "Tanya" } },
    { id: "teks", kind: "text", x: 0, y: 10, w: 6, h: 3, style: "plain", hide_on: "mobile", data: { title: "Halo <b>", body: "Baris 1\nBaris 2" } },
    { id: "hero", kind: "hero", x: 0, y: 0, w: 8, h: 8, style: "plain", data: { title: "Kopi Senja" } },
    { id: "wa", kind: "whatsapp", x: 6, y: 10, w: 6, h: 3, style: "plain", layout: "inline", data: { label: "Pesan", message: "Halo kak & tim" } },
  ],
};

describe("renderer publik Studio (v2)", () => {
  it("urutan ponsel atas→bawah lalu kiri→kanan", () => {
    expect(mobileSorted(doc.blocks).map((b) => b.id)).toEqual(["hero", "chat", "teks", "wa"]);
  });
  it("tautan WhatsApp dari nomor profil dengan pesan ter-encode", () => {
    expect(whatsappHref("0812-3456-789", "Halo kak & tim")).toMatch(/^https:\/\/wa\.me\/62?8123456789\?text=Halo%20kak%20%26%20tim$/);
    expect(whatsappHref(null, "x")).toBeNull();
  });
  it("blok grid diposisikan, hide_on & embed dihormati, teks tetap teks", () => {
    const html = renderToStaticMarkup(<StudioSite doc={doc} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain('data-surface="cream"');
    expect(html).toMatch(/id="hero"[^>]*style="grid-column:1 \/ span 8;grid-row:1 \/ span 8/);
    expect(html).toMatch(/id="teks"[^>]*data-hide="mobile"/);
    // chatbot bilah = embed: tidak dirender sebagai kotak grid
    expect(html).not.toContain('id="chat"');
    expect(html).toContain("Halo &lt;b&gt;");
    expect(html).not.toContain("<b>");
    expect(html).toContain("wa.me/");
    // P9-9: tautan privasi & syarat selalu ada.
    expect(html).toContain('href="/kebijakan-privasi"');
    expect(html).toContain('href="/syarat-transaksi"');
  });
  it("teks alternatif foto (WCAG 1.1.1): image_alt dirender & di-escape, galeri jatuh ke keterangan", () => {
    const withImg: PageV2 = { ...doc, blocks: [
      { id: "hero", kind: "hero", x: 0, y: 0, w: 12, h: 8, style: "plain", data: { title: "Kopi", image: "asset:1", image_alt: 'Kopi "susu" <b>' } },
      { id: "promo", kind: "promotion", x: 0, y: 8, w: 6, h: 4, style: "plain", data: { title: "Promo", image: "asset:2" } },
      { id: "gal", kind: "gallery", x: 0, y: 12, w: 12, h: 6, style: "plain", data: { images: [{ asset: "asset:3", caption: "Etalase" }, { asset: "asset:4", alt: "Barista", caption: "x" }] } },
    ] };
    const html = renderToStaticMarkup(<StudioSite doc={withImg} business={biz} assetUrl={(r) => `/f/${r}`} />);
    expect(html).toContain('alt="Kopi &quot;susu&quot; &lt;b&gt;"');
    expect(html).toMatch(/src="\/f\/asset:2" alt=""/);
    expect(html).toContain('alt="Etalase"');
    expect(html).toContain('alt="Barista"');
    // P9-7: banner (LCP) dimuat segera dengan prioritas tinggi; foto lain lazy; semua decoding async.
    expect(html).toMatch(/alt="Kopi &quot;susu&quot; &lt;b&gt;" decoding="async" loading="eager" fetch[pP]riority="high"/);
    // srcset hanya untuk URL file bertanda tangan (di sini /f/… → tidak ada srcset).
    expect(html).not.toContain("srcSet");
    expect(html).toMatch(/src="\/f\/asset:2"[^>]*alt=""[^>]*loading="lazy"/);
  });
  it("srcset varian server + sizes dari lebar blok (P9-7)", () => {
    const U = "https://api.x/files/0f8fad5b-d9cb-469f-a165-70867728950e?t=a&exp=1&sig=s";
    const d: PageV2 = { ...doc, blocks: [
      { id: "h8", kind: "hero", x: 0, y: 0, w: 8, h: 8, style: "plain", data: { title: "K", image: "asset:1" } },
      { id: "a4", kind: "about", x: 8, y: 0, w: 4, h: 8, style: "plain", data: { title: "A", body: "b", image: "asset:1" } },
    ] };
    const html = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => U} />).replace(/&amp;/g, "&");
    for (const w of [320, 640, 960, 1280]) expect(html).toContain(`${U}&w=${w} ${w}w`);
    // banner 8/12 × 1180 = 787 px (split 2 kolom → 393 px); about 4/12 = 393 px (< 768 → satu kolom penuh).
    expect(html).toContain('sizes="(min-width: 720px) 393px, 100vw"');
    expect(html.match(/sizes="\(min-width: 720px\) 393px, 100vw"/g)?.length).toBe(2);
  });
});
