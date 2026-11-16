import { describe, expect, it } from "vitest";

import type { Business } from "@/renderer/types";

import { readiness, type ReadyCell } from "./readiness";

const biz = (over: Partial<Business["profile"]> = {}, extra: Partial<Business> = {}): Business => ({
  profile: { name: "Kopi", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false, description: "Kedai kopi", whatsapp: "0812", ...over },
  addresses: [{ label: null, address: "Jl. Melati 3", latitude: null, longitude: null }],
  hours: [{ day_of_week: 1, open: "08:00", close: "17:00" }],
  exceptions: [],
  ...extra,
} as Business);
const cell = (id: string, kind: ReadyCell["kind"], data: ReadyCell["data"] = {}): ReadyCell => ({ id, kind, data });
const keys = (i: { key: string }[]) => i.map((x) => x.key);

describe("pemeriksa siap terbit", () => {
  it("situs lengkap: tidak ada catatan", () => {
    const cells = [cell("h", "hero", { title: "Kopi Senja", image: "asset:1", image_alt: "Kopi susu" }), cell("g", "product_grid"), cell("w", "whatsapp", { label: "WA" })];
    expect(readiness({ cells, business: biz(), productCount: 3 })).toEqual([]);
  });

  it("menemukan yang terlewat dan menunjuk bloknya", () => {
    const cells = [
      cell("h", "hero", { title: "  " }),
      cell("a", "about", { image: "asset:2" }),
      cell("g", "product_grid"),
      cell("l", "location"),
      cell("j", "business_hours"),
      cell("k", "contact", { show_whatsapp: true }),
    ];
    const r = readiness({ cells, business: biz({ whatsapp: null, phone: null, description: null }, { addresses: [], hours: [] }), productCount: 0 });
    expect(keys(r)).toEqual(["title", "alt:a", "whatsapp", "products", "address", "hours", "description"]);
    expect(r.find((x) => x.key === "alt:a")?.cellId).toBe("a");
    expect(r.find((x) => x.key === "whatsapp")).toMatchObject({ cellId: "k", area: "business" });
  });

  it("deskripsi SEO Studio menggantikan deskripsi profil; foto tanpa gambar tidak butuh alt", () => {
    const cells = [cell("h", "hero", { title: "Kopi" }), cell("p", "promotion", { title: "Promo" })];
    expect(readiness({ cells, business: biz({ description: "" }), productCount: 0, seo: { description: "Kopi rumahan" } })).toEqual([]);
  });

  it("tanpa banner sama sekali: catatan judul tanpa blok", () => {
    const r = readiness({ cells: [cell("t", "text", { body: "x" })], business: biz(), productCount: 0 });
    expect(r).toEqual([{ key: "title", message: expect.any(String), cellId: undefined }]);
  });

  // Regresi: plugin Chatbot terpasang tetapi chatbot belum diaktifkan di Asisten AI → plugin
  // tidak tampil di situs terbit. Harus diperingatkan sebelum terbit, bukan setelah.
  it("plugin Chatbot dengan chatbot nonaktif jadi catatan siap terbit", () => {
    const cells = [cell("h", "hero", { title: "Kopi" }), cell("c", "chatbot", { title: "Tanya" })];
    const r = readiness({ cells, business: biz(), productCount: 0, chatReady: false });
    expect(r.find((x) => x.key === "chatbot")).toMatchObject({ cellId: "c", area: "ai" });
  });

  it("chatbot aktif atau belum diketahui tidak memunculkan catatan", () => {
    const cells = [cell("h", "hero", { title: "Kopi" }), cell("c", "chatbot", { title: "Tanya" })];
    expect(keys(readiness({ cells, business: biz(), productCount: 0, chatReady: true }))).not.toContain("chatbot");
    expect(keys(readiness({ cells, business: biz(), productCount: 0, chatReady: null }))).not.toContain("chatbot");
  });

  it("tanpa plugin Chatbot, status chatbot tidak relevan", () => {
    const cells = [cell("h", "hero", { title: "Kopi" })];
    expect(keys(readiness({ cells, business: biz(), productCount: 0, chatReady: false }))).not.toContain("chatbot");
  });
});
