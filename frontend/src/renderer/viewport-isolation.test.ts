// Kriteria lulus "responsif kanvas": lebar BINGKAI (bukan jendela) harus menentukan breakpoint toko.
// jsdom tidak menghitung layout, jadi yang diuji adalah KONTAK CSS-nya:
//   - wadah kanvas bertipe container bernama "storefront";
//   - breakpoint tata letak toko memakai @container, bukan @media lebar viewport.
// Ini yang membuat mengubah Ponsel → Tablet → Desktop mengubah jumlah kolom tanpa mengecilkan jendela.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { DEVICE_WIDTH } from "@/studio/DeviceFrame";

const css = (f: string) => readFileSync(resolve(__dirname, f), "utf8");

describe("isolasi viewport kanvas", () => {
  it("lebar bingkai per perangkat sesuai spesifikasi", () => {
    expect(DEVICE_WIDTH.mobile).toBe(390);
    expect(DEVICE_WIDTH.tablet).toBe(768);
    expect(DEVICE_WIDTH.desktop).toBe(1280); // layar laptop: grid situs penuh 1200 px
  });

  it(".st-frame adalah container bernama storefront dan mengunci elemen fixed", () => {
    const s = css("site.css");
    const block = /\.st-frame\s*\{([^}]*)\}/.exec(s)?.[1] ?? "";
    expect(block).toContain("container-type: inline-size");
    expect(block).toContain("container-name: storefront");
    // Tanpa ini, chatbot/WhatsApp melayang menempel ke jendela browser, bukan ke bingkai.
    expect(block).toContain("transform: translateZ(0)");
  });

  it("tata letak toko memakai @container, bukan @media lebar layar", () => {
    const s = css("site.css");
    // Semua @media berbasis lebar (min-width/max-width) di site.css harus nol.
    const widthMedia = [...s.matchAll(/@media[^{]*(?:min|max)-width[^{]*\{/g)].map((m) => m[0]);
    expect(widthMedia).toEqual([]);
    // Dan setidaknya ada satu @container yang mengatur tumpukan ponsel.
    expect(s).toMatch(/@container[^{]*max-width:\s*719px/);
  });

  it("shop.css tidak memakai container-type pada .shop (akan mengunci .shop-fab)", () => {
    const s = css("../shop/shop.css");
    // `.shop-fab` memakai position: fixed di situs publik. Menjadikan `.shop` container akan
    // menjadikannya containing block dan merusak posisi tombol di situs pelanggan.
    expect(s).not.toMatch(/\.shop\s*\{[^}]*container-type/);
    expect(s).toContain("position: fixed");
  });

  it("chat.css tidak memakai container-type pada .chat (akan mengunci .chat-fab)", () => {
    const s = css("../chat/chat.css");
    expect(s).not.toMatch(/\.chat\s*\{[^}]*container-type/);
    expect(s).toContain("position: fixed");
  });

  it("aturan @container untuk chat hanya aktif di dalam bingkai Studio", () => {
    const s = css("../chat/chat.css");
    // Breakpoint viewport tetap ada sebagai default situs publik...
    expect(s).toContain("@media (min-width: 640px)");
    // ...dan override khusus bingkai Studio memakai nama container.
    expect(s).toContain("@container storefront");
  });
});

// Regresi: kanvas Studio tampak kosong karena `.st-frame` berlebar 0.
// `container-type: inline-size` menyiratkan inline-size containment — lebar elemen TIDAK bisa
// dihitung dari isinya. Di dalam induk flex tanpa lebar eksplisit, bingkai menyusut ke 0 dan seluruh
// situs (yang masih ter-render di DOM) jadi tak terlihat. Ini sempat terjadi di produksi.
describe("regresi: bingkai kanvas tidak boleh berlebar nol", () => {
  it(".st-frame punya lebar eksplisit, bukan auto", () => {
    const s = css("site.css");
    // Komentar CSS dibuang dulu: komentar memuat ";" yang mengacaukan pencocokan deklarasi.
    const noComments = s.replace(/\/\*[\s\S]*?\*\//g, "");
    const block = /\.st-frame\s*\{([^}]*)\}/.exec(noComments)?.[1] ?? "";
    expect(block).toContain("container-type: inline-size");
    // Lebar tidak boleh bergantung pada isi (containment mematikan itu).
    expect(block).toMatch(/(^|;)\s*width:\s*100%/);
    expect(block).toContain("max-width: 100%");
    // `flex: none` — kalau boleh tumbuh (1 1 auto), lebar 390px dari DeviceFrame ditumbuhkan
    // induk flex sampai selebar panggung dan semua perangkat tampak sama.
    expect(block).toContain("flex: none");
    expect(block).toContain("min-width: 0");
  });

  it("semua perangkat memakai lebar layar asli (desktop 1280 = grid penuh 1200 seperti situs terbit)", () => {
    expect(DEVICE_WIDTH).toEqual({ mobile: 390, tablet: 768, desktop: 1280 });
    const src = readFileSync(resolve(__dirname, "../studio/DeviceFrame.tsx"), "utf8");
    // Diperkecil dengan scale(), bukan dengan mengubah lebar (lebar menentukan tata letak).
    expect(src).toContain("transform: `scale(${zoom})`");
  });
});
