// Kelengkapan kamus EN: setiap teks antarmuka di kode punya terjemahan, placeholder-nya sama,
// tidak ada terjemahan yatim, dan kunci ganda di beberapa berkas kamus tidak saling bertentangan.
import { resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { extractKeys, placeholders } from "./catalog";
import { EN, EN_PARTS } from "./en";
import { getLocale, loadEnglish, setLocale, t, tmap } from "./index";

// Kamus EN dimuat malas (chunk terpisah); tes memuatnya dulu agar setLocale("en") langsung berlaku.
beforeAll(() => loadEnglish());

const keys = extractKeys(resolve(__dirname, ".."));
/** Kunci tanpa huruf (mis. "{0} – {1}") sama di semua bahasa — tidak wajib di kamus. */
const needsText = (k: string) => /[A-Za-zÀ-ÿ]/.test(k);

describe("kamus English", () => {
  it("setiap teks di kode punya terjemahan EN", () => {
    const missing = [...keys.entries()].filter(([k]) => needsText(k) && !(k in EN)).map(([k, at]) => `${JSON.stringify(k)}  ← ${at.replace(/^.*\/src\//, "src/")}`);
    expect(missing, `Tambahkan ke src/i18n/en/*.ts:\n${missing.join("\n")}`).toEqual([]);
  });

  it("placeholder {0}, {1}, … sama di kedua bahasa", () => {
    const bad = Object.entries(EN).filter(([id, en]) => placeholders(id).join() !== placeholders(en).join()).map(([id]) => id);
    expect(bad).toEqual([]);
  });

  it("tidak ada terjemahan yatim (teksnya sudah tidak dipakai di kode)", () => {
    const orphan = Object.keys(EN).filter((k) => !keys.has(k));
    expect(orphan, `Hapus dari kamus:\n${orphan.join("\n")}`).toEqual([]);
  });

  it("kunci yang muncul di beberapa berkas kamus memakai terjemahan yang sama", () => {
    const seen = new Map<string, string>();
    const conflict: string[] = [];
    for (const part of Object.values(EN_PARTS)) {
      for (const [k, v] of Object.entries(part)) {
        if (seen.has(k) && seen.get(k) !== v) conflict.push(`${k}: "${seen.get(k)}" vs "${v}"`);
        seen.set(k, v);
      }
    }
    expect(conflict).toEqual([]);
  });

  it("terjemahan tidak kosong dan tidak berisi entitas HTML", () => {
    const bad = Object.entries(EN).filter(([, v]) => !v.trim() || /&(amp|quot|lt|gt|nbsp);/.test(v)).map(([k]) => k);
    expect(bad).toEqual([]);
  });
});

describe("t() & tmap()", () => {
  it("bawaan Indonesia; EN menerjemahkan dan mengisi placeholder", () => {
    const before = getLocale();
    try {
      setLocale("id", false);
      expect(t("Halaman {0} dibuat.", "Menu")).toBe("Halaman Menu dibuat.");
      setLocale("en", false);
      expect(t("Halaman {0} dibuat.", "Menu")).toBe("Page Menu created.");
      expect(t("teks yang tidak ada di kamus")).toBe("teks yang tidak ada di kamus");
      const map = tmap({ a: "Simpan" });
      expect(map.a).toBe("Save");
      setLocale("id", false);
      expect(map.a).toBe("Simpan");
    } finally {
      setLocale(before, false);
    }
  });
});

describe("judul tab", () => {
  it("hanya judul halaman terdaftar yang diterjemahkan, nama produk/toko tidak", async () => {
    const { translateTitle } = await import("./titles");
    const before = getLocale();
    try {
      setLocale("en", false);
      expect(translateTitle("Pesanan · UMKM OS")).toBe("Orders · UMKM OS");
      expect(translateTitle("Pembayaran & pengiriman · UMKM OS")).toBe("Payments & delivery · UMKM OS");
      // "Kopi" ada di kamus (nama palet), tetapi sebagai judul produk tidak boleh berubah.
      expect(translateTitle("Kopi")).toBe("Kopi");
      setLocale("id", false);
      expect(translateTitle("Pesanan · UMKM OS")).toBe("Pesanan · UMKM OS");
    } finally {
      setLocale(before, false);
    }
  });
});
