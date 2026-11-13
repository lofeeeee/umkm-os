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
  });
});
