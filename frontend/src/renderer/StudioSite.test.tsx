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

describe("footer & kartu lokasi", () => {
  const full: Business = {
    ...biz,
    profile: { ...biz.profile, name: "Kopi Senja", description: "Kopi susu gula aren", phone: "0812", socials: [{ platform: "instagram", url: "https://instagram.com/kopi" }, { platform: "x", url: "javascript:alert(1)" }] },
    addresses: [{ address: "Jl. Mawar 1, Bandung" }],
    hours: [{ day_of_week: 1, open: "08:00", close: "17:00" }],
  };
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#C2410C", font: "inter" }, blocks: [
    { id: "loc", kind: "location", x: 0, y: 0, w: 6, h: 6, style: "card", data: { title: "Lokasi", show_map: true } },
    { id: "jam", kind: "business_hours", x: 6, y: 0, w: 6, h: 9, style: "card", data: { title: "Jam" } },
  ] };
  const html = renderToStaticMarkup(<StudioSite doc={page} business={full} status={{ status: "open", timezone: "Asia/Jakarta", local_time: "09:00" }} assetUrl={() => undefined} pages={[{ slug: "home", title: "Beranda", current: true }, { slug: "tentang", title: "Tentang" }]} />);
  it("footer memuat identitas, status, WhatsApp, kolom tautan, kontak, dan sosial yang aman", () => {
    expect(html).toContain('class="sf-name">Kopi Senja');
    expect(html).toContain("Kopi susu gula aren");
    expect(html).toContain("sf-status-open");
    expect(html).toMatch(/class="sf-wa" href="https:\/\/wa\.me\//);
    expect(html).toContain('href="/tentang"');
    expect(html).toContain('href="/keranjang"');
    expect(html).toContain("Jl. Mawar 1, Bandung");
    expect(html).toContain('href="https://instagram.com/kopi"');
    expect(html).not.toContain("javascript:");
  });
  it("kartu bersebelahan disamakan tingginya; peta jadi tautan petunjuk arah", () => {
    expect(html).toMatch(/id="loc"[^>]*style="grid-column:1 \/ span 6;grid-row:1 \/ span 9/);
    expect(html).toContain('class="s-map" href="https://www.google.com/maps/dir/?api=1&amp;destination=Jl.%20Mawar%201%2C%20Bandung"');
  });
  it("lencana \"Dibuat dengan UMKM OS\" tampil bawaan, bertaut ke platform; hide_badge menyembunyikannya", () => {
    const linked = renderToStaticMarkup(<StudioSite doc={page} business={full} assetUrl={() => undefined} platformUrl="https://umkm.example/" />);
    expect(linked).toMatch(/<a href="https:\/\/umkm\.example\/" target="_blank" rel="noopener nofollow" data-testid="made-with">/);
    expect(html).toContain('data-testid="made-with"');
    const hidden = renderToStaticMarkup(<StudioSite doc={{ ...page, theme: { ...page.theme, hide_badge: true } }} business={full} assetUrl={() => undefined} />);
    expect(hidden).not.toContain("made-with");
  });
  it("tanpa data kontak, kolom kontak tidak dirender", () => {
    const bare = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={() => undefined} />);
    expect(bare).not.toContain("sf-addr");
    expect(bare).not.toContain("sf-status");
  });
});

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
  it("anchor template (#kontak, #menu, …) menunjuk blok pertama jenis itu", () => {
    const d: PageV2 = { ...doc, blocks: [
      { id: "c1", kind: "contact", x: 0, y: 0, w: 4, h: 4, style: "plain", data: { title: "Kontak", show_whatsapp: true } },
      { id: "c2", kind: "contact", x: 4, y: 0, w: 4, h: 4, style: "plain", data: { title: "Kontak 2", show_whatsapp: true } },
      { id: "p1", kind: "product_grid", x: 0, y: 4, w: 12, h: 8, style: "plain", data: { title: "Menu" } },
    ] };
    const html = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => undefined} />);
    expect(html.match(/id="kontak"/g)?.length).toBe(1);
    expect(html).toMatch(/id="c1"[^>]*>(<span[^>]*><\/span>)*<span id="kontak"/);
    for (const a of ["produk", "menu", "koleksi", "karya"]) expect(html).toContain(`id="${a}"`);
  });

  // Regresi: navigasi terlihat di Studio tetapi hilang setelah diterbitkan. `navbar_style` default
  // adalah "top" dan sebelumnya TIDAK dirender sama sekali oleh renderer publik.
  it("navbar_style top dirender sebagai bilah atas (bukan hanya side)", () => {
    const top: PageV2 = { ...doc, theme: { ...doc.theme, navbar_style: "top", navbar_sticky: true } };
    const html = renderToStaticMarkup(<StudioSite doc={top} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain("s-headbar");
    expect(html).toContain("s-sticky");
    expect(html).toContain("Kopi"); // nama toko sebagai brand
    expect(html).not.toContain("s-nav-side");
    expect(html).not.toContain("s-has-side-nav");
  });

  it("tema tanpa navbar_style tetap menampilkan navigasi atas", () => {
    const { navbar_style: _omitted, ...rest } = doc.theme;
    const html = renderToStaticMarkup(<StudioSite doc={{ ...doc, theme: rest }} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain("s-headbar");
    expect(html).not.toContain("s-sticky");
  });

  it("navbar_style side merender bilah samping di dalam shell", () => {
    const side: PageV2 = { ...doc, theme: { ...doc.theme, navbar_style: "side" } };
    const html = renderToStaticMarkup(<StudioSite doc={side} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain("s-has-side-nav");
    expect(html).toContain("sv2-shell");
    expect(html).toContain("s-nav-side");
    expect(html).not.toContain("s-headbar");
  });

  it("tautan navigasi memakai halaman terbit, bukan anchor, saat daftar halaman tersedia", () => {
    const html = renderToStaticMarkup(
      <StudioSite
        doc={doc}
        business={biz}
        assetUrl={() => undefined}
        pages={[
          { slug: "home", title: "Beranda", current: true },
          { slug: "menu", title: "Menu" },
          { slug: "tentang-kami", title: "Tentang kami" },
        ]}
      />,
    );
    expect(html).toContain('href="/menu"');
    expect(html).toContain('href="/tentang-kami"');
    expect(html).toContain('href="/"');
    expect(html).toContain(">Beranda<");
  });

  it("tanpa daftar halaman, navigasi jatuh ke anchor bagian di halaman ini", () => {
    const d: PageV2 = { ...doc, blocks: [
      { id: "k", kind: "contact", x: 0, y: 0, w: 12, h: 4, style: "plain", data: { title: "Kontak", show_whatsapp: true } },
    ] };
    const html = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain('href="#kontak"');
    expect(html).toContain(">Kontak<");
  });

  // ---------------------------------------------------------------- paritas Studio ↔ publik
  // Kriteria lulus: isi tiap blok di kanvas Studio harus sama dengan situs terbit, KECUALI node yang
  // memang harus berbeda agar bisa disunting:
  //   - teks: publik `<p>`/`<h2>` berisi teks; editor membungkusnya `<span contentEditable>`;
  //   - tautan aksi: publik `<a href>`; editor `<span>` agar klik tidak meninggalkan halaman.
  // Yang diuji: pembungkus grid (kelas + posisi), urutan blok, dan footer identik; perbedaan terbatas
  // pada dua kategori itu.

  /** Isi (innerHTML) elemen `div#id.sv2-block`, dihitung dengan mencocokkan kedalaman `<div>`. */
  function blockInner(html: string, id: string): string | null {
    const start = html.indexOf(`<div id="${id}" class="sv2-block"`);
    if (start < 0) return null;
    const open = html.indexOf(">", start) + 1;
    let depth = 1;
    let i = open;
    while (i < html.length && depth > 0) {
      const nextOpen = html.indexOf("<div", i);
      const nextClose = html.indexOf("</div>", i);
      if (nextClose < 0) break;
      if (nextOpen >= 0 && nextOpen < nextClose) { depth += 1; i = nextOpen + 4; }
      else { depth -= 1; i = nextClose + 6; }
    }
    return html.slice(open, depth === 0 ? i - 6 : i);
  }

  /**
   * Urutan tag + kelas, tanpa teks dan tanpa penanda editor. Ini yang harus identik antara kanvas
   * Studio dan situs terbit: struktur, bukan kosmetik.
   *
   * Perbedaan yang MEMANG disengaja (dan karena itu dinormalkan) hanya empat:
   *   1. pembungkus seleksi `.sf-block` + labelnya,
   *   2. `<span contentEditable>` di sekitar teks yang bisa diketik,
   *   3. `<span>` vs `<a href>` untuk tautan aksi (klik di editor tidak boleh menavigasi),
   *   4. paragraf kosong `<p class="s-lead">` agar subtitle kosong tetap bisa diklik.
   */
  function tags(html: string): string[] {
    const cleaned = html
      .replace(/<div class="sf-block[^"]*"[^>]*>/g, "")
      .replace(/<span class="sf-block-tag"[^>]*>[^<]*<\/span>/g, "")
      .replace(/<span class="sf-block-lock"[^>]*><\/span>/g, "")
      // Buang pembungkus teks yang bisa disunting beserta isinya (teks polos, tanpa span bersarang).
      .replace(/<span class="s-edit"[^>]*>[\s\S]*?<\/span>/g, "")
      // Paragraf kosong yang hanya ada agar bisa diklik di editor.
      .replace(/<p class="s-lead">\s*<\/p>/g, "")
      // Tautan aksi: editor memakai <a> tanpa href (tag sama, klik tidak menavigasi).
      .replace(/ href="[^"]*" target="_blank" rel="noopener noreferrer"/g, "");

    // Telusuri tag dengan stack agar penutup berlebih (sisa wrapper `.sf-block`) terbuang otomatis,
    // bukan ditebak lewat regex posisi.
    const out: string[] = [];
    const stack: string[] = [];
    for (const m of cleaned.matchAll(/<(\/?)([a-z0-9]+)([^>]*?)(\/?)>/g)) {
      const close = m[1] === "/";
      const tag = m[2]!;
      const attrs = m[3] ?? "";
      const selfClosing = m[4] === "/";
      if (["span", "path", "svg", "circle"].includes(tag)) continue;
      if (selfClosing) { out.push(`${tag}!`); continue; }
      if (close) {
        // Hanya terima penutup yang cocok dengan stack; penutup yatim dibuang.
        if (stack[stack.length - 1] === tag) { stack.pop(); out.push(`/${tag}`); }
        continue;
      }
      stack.push(tag);
      const cls = /class="([^"]*)"/.exec(attrs)?.[1];
      out.push(cls ? `${tag}.${cls.trim().split(/\s+/).join(".")}` : tag);
    }
    return out;
  }

  it("paritas DOM: isi blok di mode studio-edit sama dengan mode customer", () => {
    const d: PageV2 = {
      ...doc,
      theme: { ...doc.theme, navbar_style: "top" },
      blocks: [
        { id: "hero", kind: "hero", x: 0, y: 0, w: 8, h: 8, style: "plain", data: { title: "Kopi Senja" } },
        { id: "teks", kind: "text", x: 0, y: 8, w: 6, h: 3, style: "card", data: { title: "Tentang", body: "Baris" } },
        { id: "wa", kind: "whatsapp", x: 6, y: 8, w: 6, h: 3, style: "plain", layout: "inline", data: { label: "Pesan" } },
      ],
    };
    const customer = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => undefined} mode="customer" />);
    const editor = renderToStaticMarkup(
      <StudioSite doc={d} business={biz} assetUrl={() => undefined} mode="studio-edit" selectedId="hero" onSelect={() => {}} onEditBlock={() => {}} />,
    );

    for (const id of ["hero", "teks", "wa"]) {
      const a = blockInner(customer, id);
      const b = blockInner(editor, id);
      expect(a, `blok ${id} ada di situs terbit`).not.toBeNull();
      expect(b, `blok ${id} ada di kanvas`).not.toBeNull();
      // Pembungkus grid (kelas + posisi) identik.
      const boxOf = (html: string) => html.slice(html.indexOf(`<div id="${id}" class="sv2-block"`), html.indexOf(">", html.indexOf(`<div id="${id}" class="sv2-block"`)) + 1);
      expect(boxOf(editor)).toBe(boxOf(customer));
      // Struktur (tag + kelas) identik setelah penanda editor dinormalkan.
      expect(tags(b!)).toEqual(tags(a!));
    }

    // Mode edit benar-benar menambahkan pelapis (kalau tidak, tes ini hampa).
    expect(editor).toContain("sf-block-tag");
    expect(editor.toLowerCase()).toContain("contenteditable");
    expect(customer).not.toContain("sf-block-tag");
    expect(customer.toLowerCase()).not.toContain("contenteditable");
    // Footer & navigasi sama.
    expect(editor).toContain('class="sv2-foot"');
    expect(customer).toContain('class="sv2-foot"');
    expect(editor).toContain("s-headbar");
    expect(customer).toContain("s-headbar");
  });

  it("mode customer (default) tidak memasang pelapis editor", () => {
    const html = renderToStaticMarkup(<StudioSite doc={doc} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain('data-mode="customer"');
    expect(html).not.toContain("sf-block-tag");
    expect(html).not.toContain("sf-editing");
  });

  it("mode studio-edit menandai blok terpilih dan meneruskan onSelect", () => {
    // Dipanggil lewat pointer handler tidak bisa diuji tanpa DOM; yang diuji adalah penanda terpilih.
    const html = renderToStaticMarkup(
      <StudioSite doc={doc} business={biz} assetUrl={() => undefined} mode="studio-edit" selectedId="hero" onSelect={() => {}} />,
    );
    expect(html).toContain('data-mode="studio-edit"');
    expect(html).toContain("sf-editing");
    expect(html).toMatch(/class="sf-block sf-block-selected"[^>]*data-block-id="hero"/);
    expect(html).toContain('data-block-label="Banner"');
  });

  it("mode studio-edit tidak memasang widget chat asli (tidak memanggil API saat pratinjau)", () => {
    const d: PageV2 = { ...doc, blocks: [
      { id: "chat", kind: "chatbot", x: 8, y: 0, w: 4, h: 9, style: "card", layout: "floating", data: { title: "Tanya" } },
    ] };
    const editor = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => undefined} mode="studio-edit" />);
    // Markup chatbot tetap ada (bentuk sama), tetapi tanpa pemicu fetch: penanda khusus editor.
    expect(editor).toContain('data-editor-preview=""');
    expect(editor).toContain("chat-fab");
    // Widget asli (mode customer) memakai komponen React yang memanggil API; di SSR ia belum merender fab.
    const customer = renderToStaticMarkup(<StudioSite doc={d} business={biz} assetUrl={() => undefined} mode="customer" />);
    expect(customer).not.toContain('data-editor-preview=""');
  });
});

// Kriteria lulus "isolasi gaya": tema dasbor (gelap) tidak boleh mewarnai kanvas toko. Warna kanvas
// sepenuhnya berasal dari `theme` dokumen (--s-primary, data-surface), bukan dari ancestor.
describe("isolasi gaya kanvas", () => {
  it("warna blok berasal dari tema dokumen, bukan dari ancestor", () => {
    const d: PageV2 = { ...doc, theme: { ...doc.theme, primary: "#C2410C", surface: "cream" } };
    const html = renderToStaticMarkup(
      // Simulasi kanvas dipasang di dalam dashboard bertema gelap.
      <div data-theme="dark" className="dashboard-dark">
        <StudioSite doc={d} business={biz} assetUrl={() => undefined} mode="studio-edit" />
      </div>,
    );
    // Token tema toko dipasang inline pada wadah kanvas...
    expect(html).toContain("--s-primary:#C2410C");
    expect(html).toContain('data-surface="cream"');
    expect(html).toContain('data-mode="studio-edit"');
    // ...dan tidak ada warna latar dashboard yang bocor ke kanvas (tidak ada style latar gelap).
    expect(html).not.toContain("--bg-surface");
  });

  it("permukaan gelap toko tetap memakai token toko, bukan token dasbor", () => {
    const d: PageV2 = { ...doc, theme: { ...doc.theme, surface: "dark" } };
    const html = renderToStaticMarkup(
      <div data-theme="light">
        <StudioSite doc={d} business={biz} assetUrl={() => undefined} />
      </div>,
    );
    expect(html).toContain('data-surface="dark"');
  });
});

describe("plugin Latar (lapisan tembus)", () => {
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#C2410C", font: "inter" }, blocks: [
    { id: "a", kind: "text", x: 0, y: 0, w: 6, h: 4, style: "plain", data: { body: "A" } },
    { id: "b", kind: "text", x: 6, y: 0, w: 6, h: 4, style: "plain", data: { body: "B" } },
    { id: "c", kind: "text", x: 0, y: 8, w: 12, h: 4, style: "plain", data: { body: "C" } },
    { id: "latar", kind: "background", x: 0, y: 0, w: 12, h: 6, style: "card", data: { image: "asset:11111111-1111-1111-1111-111111111111", fit: "stretch", position: "top", opacity: 70 } },
    { id: "kosong", kind: "background", x: 0, y: 20, w: 6, h: 4, style: "plain", data: {} },
  ] };
  const asset = (r: string) => (r.startsWith("asset:") ? "/files/bg.webp" : undefined);
  const html = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={asset} />);
  it("dirender paling awal di grid (di belakang blok), dengan gaya isi yang dipilih", () => {
    const grid = html.slice(html.indexOf('class="sv2-grid"'));
    expect(grid.indexOf('id="latar"')).toBeLessThan(grid.indexOf('id="a"'));
    expect(html).toMatch(/id="latar"[^>]*data-layer="cover"/);
    expect(html).toContain('background-size:100% 100%');
    expect(html).toContain('background-position:top');
    expect(html).toContain("opacity:0.7");
    expect(html).toContain("data-round");
  });
  it("situs publik: latar tanpa gambar tidak dirender; baris ponsel membentang di belakang blok yang ditumpuk", () => {
    expect(html).not.toContain('id="kosong"');
    expect(html).toMatch(/id="latar"[^>]*style="[^"]*--mrow:1 \/ 3/);
    expect(html).toMatch(/id="c"[^>]*style="[^"]*--mrow:3/);
  });
});

describe("plugin Gambar (bebas, PNG transparan)", () => {
  const img = "asset:22222222-2222-2222-2222-222222222222";
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#C2410C", font: "inter" }, blocks: [
    { id: "a", kind: "text", x: 0, y: 0, w: 12, h: 6, style: "plain", data: { body: "A" } },
    { id: "depan", kind: "image", layout: "above", x: 8, y: 1, w: 3, h: 4, style: "plain", data: { image: img, alt: "Stiker", href: "/produk", fit: "contain" } },
    { id: "belakang", kind: "image", layout: "below", x: 0, y: 0, w: 4, h: 4, style: "plain", data: { image: img } },
    { id: "gantung", kind: "image", layout: "sticky", x: 0, y: 6, w: 3, h: 12, style: "card", data: { image: img, fit: "crop" } },
    { id: "pop", kind: "image", layout: "popup", x: 0, y: 0, w: 3, h: 3, style: "plain", data: { image: img, corner: "top-left" } },
  ] };
  const html = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={() => "/files/x.png"} />);
  const grid = html.slice(html.indexOf('class="sv2-grid"'), html.indexOf("sv2-foot"));
  it("di belakang sebelum blok, di depan sesudahnya; menggantung & tautan & alt dirender", () => {
    expect(grid.indexOf('id="belakang"')).toBeLessThan(grid.indexOf('id="a"'));
    expect(grid.indexOf('id="depan"')).toBeGreaterThan(grid.indexOf('id="a"'));
    expect(html).toMatch(/id="depan"[^>]*data-layer="front"/);
    expect(html).toMatch(/id="belakang"[^>]*data-layer="cover"/);
    expect(html).toContain('<a href="/produk" class="sv2-img-link" aria-label="Stiker"><img src="/files/x.png" alt="Stiker"');
    expect(html).toContain('data-mode="sticky"');
    expect(html).toContain("object-fit:cover");
  });
  it("popup menempel di pojok layar di luar grid, dengan tombol tutup", () => {
    expect(grid).not.toContain('id="pop"');
    expect(html).toContain('class="sv2-popup" data-corner="top-left"');
    expect(html).toContain('class="sv2-popup-close"');
  });
});

describe("brand kit di renderer", () => {
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#0F766E", font: "inter", heading_font: "fraunces", heading_weight: 600, accent: "#F59E0B", gradient: true, radius: "round", motion: "lively", texture: "paper", page_transition: true }, blocks: [] };
  it("variabel & atribut gaya situs, transisi halaman hanya di situs publik", () => {
    const html = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={() => undefined} />);
    expect(html).toContain("--s-head-font:&#x27;Fraunces Variable&#x27;");
    expect(html).toContain("--s-head-weight:600");
    expect(html).toContain("--s-accent:#F59E0B");
    expect(html).toContain("--s-radius-kit:24px");
    expect(html).toMatch(/data-motion="lively" data-texture="paper" data-gradient="" data-radius="round"/);
    expect(html).toContain("@view-transition");
    const edit = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={() => undefined} mode="studio-edit" />);
    expect(edit).not.toContain("@view-transition");
  });
});

describe("Latar selebar layar & pemisah", () => {
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#0F766E", font: "inter", accent: "#F59E0B" }, blocks: [
    { id: "pita", kind: "background", x: 0, y: 0, w: 12, h: 8, style: "card", data: { color: "accent", bleed: true, edge_top: "wave", edge_bottom: "zigzag" } },
    { id: "setengah", kind: "background", x: 6, y: 10, w: 6, h: 4, style: "plain", data: { color: "#112233", bleed: true } },
  ] };
  const html = renderToStaticMarkup(<StudioSite doc={page} business={biz} assetUrl={() => undefined} />);
  it("pita warna tampil tanpa gambar, memanjang ke tepi yang menyentuh grid, dengan topeng tepi", () => {
    expect(html).toMatch(/id="pita"[\s\S]*?data-bleed="lr"/);
    expect(html).toMatch(/id="setengah"[\s\S]*?data-bleed="r"/);
    expect(html).toContain("background-color:var(--s-accent, var(--s-primary))");
    expect(html).toContain("mask-image:linear-gradient(#000,#000), url(");
    expect(html).not.toMatch(/id="pita"[\s\S]*?data-round/);
  });
});

describe("urutan ponsel (m_order)", () => {
  it("m_order didahulukan; blok tanpa m_order menyusul urut atas→bawah", () => {
    const blocks = [
      { id: "a", kind: "text", x: 0, y: 0, w: 6, h: 4, style: "plain", data: {} },
      { id: "b", kind: "text", x: 6, y: 0, w: 6, h: 4, style: "plain", data: {}, m_order: 1 },
      { id: "c", kind: "text", x: 0, y: 4, w: 12, h: 4, style: "plain", data: {}, m_order: 0 },
    ] as PageV2["blocks"];
    expect(mobileSorted(blocks).map((b) => b.id)).toEqual(["c", "b", "a"]);
  });
});

describe("plugin Bentuk & Logo", () => {
  const withLogo: Business = { ...biz, profile: { ...biz.profile, name: "Kopi Senja", logo: "asset:55555555-5555-5555-5555-555555555555" } };
  const page: PageV2 = { schema_version: 2, template: "t", theme: { primary: "#0F766E", font: "inter", accent: "#F59E0B" }, blocks: [
    { id: "a", kind: "text", x: 0, y: 0, w: 12, h: 4, style: "plain", data: { body: "A" } },
    { id: "bintang", kind: "shape", layout: "below", x: 0, y: 0, w: 2, h: 3, style: "plain", data: { shape: "star", color: "accent", rotate: -15 } },
    { id: "logo", kind: "logo", x: 9, y: 0, w: 3, h: 3, style: "plain", data: { show_name: true } },
  ] };
  const html = renderToStaticMarkup(<StudioSite doc={page} business={withLogo} assetUrl={() => "/files/logo.png"} />);
  it("bentuk di belakang blok dengan warna aksen & putaran; logo bebas bertaut ke beranda", () => {
    const grid = html.slice(html.indexOf('class="sv2-grid"'));
    expect(grid.indexOf('id="bintang"')).toBeLessThan(grid.indexOf('id="a"'));
    expect(html).toContain('fill="var(--s-accent, var(--s-primary))"');
    expect(html).toContain("transform:rotate(-15deg)");
    expect(html).toMatch(/<a class="sv2-logo" href="\/" aria-label="Kopi Senja"><img src="\/files\/logo.png"/);
  });
  it("logo tidak dobel: navigasi tidak menampilkan logo/nama bila plugin Logo ada", () => {
    const nav = html.slice(html.indexOf("s-brand"), html.indexOf("sv2-main"));
    expect(nav).not.toContain('class="s-logo"');
    expect(nav).toContain('<span class="s-sr-only">Kopi Senja</span>');
    const without = renderToStaticMarkup(<StudioSite doc={{ ...page, blocks: page.blocks.filter((b) => b.kind !== "logo") }} business={withLogo} assetUrl={() => "/files/logo.png"} />);
    expect(without).toContain('class="s-logo"');
  });
});
