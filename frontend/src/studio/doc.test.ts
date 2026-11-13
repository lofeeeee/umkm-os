import { describe, expect, it } from "vitest";

import { fromDoc, isV2, safeId, toDoc, type StudioState } from "./doc";
import { pluginDef } from "./plugins";

const st: StudioState = {
  primary: "#c2410c", surface: "cream", font: "poppins", textScale: 0.9, installed: ["hero"],
  seo: { title: " Kopi Senja ", description: "" },
  cells: [
    { id: "hero_t1abc", kind: "hero", x: 0, y: 0, w: 8, h: 10, minW: 4, minH: 2, style: "plain", data: { title: "Kopi" }, layout: undefined, autoH: true, chatOpen: true },
    { id: "chatbot-1", kind: "chatbot", x: 8, y: 0, w: 4, h: 9, minW: 3, minH: 2, style: "card", data: { title: "Tanya" }, layout: "bar", autoH: true, chatOpen: false, locked: true },
    { id: "galeri", kind: "product_grid", x: 0, y: 10, w: 12, h: 11, minW: 4, minH: 2, style: "plain", data: { title: "Menu", limit: 8 }, layout: "carousel", autoH: false },
  ],
};

describe("dokumen Studio v2", () => {
  it("toDoc menghasilkan bentuk yang diterima server", () => {
    const d = toDoc(st, "modern_store");
    expect(d.schema_version).toBe(2);
    expect(d.template).toBe("modern_store");
    expect(d.theme).toEqual({ primary: "#C2410C", font: "poppins", surface: "cream", text_scale: 0.9 });
    expect(d.seo).toEqual({ title: "Kopi Senja" });
    expect(d.blocks[0]!.id).toBe("hero-t1abc");
    expect(d.blocks[0]).not.toHaveProperty("layout");
    expect(d.blocks[0]).not.toHaveProperty("chat_open");
    expect(d.blocks[1]).toMatchObject({ layout: "bar", chat_open: false, locked: true });
    expect(d.blocks[2]).toMatchObject({ layout: "carousel", auto_height: false });
  });
  it("bolak-balik toDoc → fromDoc mempertahankan isi", () => {
    const back = fromDoc(toDoc(st, "t"), (k) => ({ minW: pluginDef(k).minW, minH: pluginDef(k).minH }));
    expect(back.surface).toBe("cream");
    expect(back.textScale).toBe(0.9);
    expect(back.cells.map((c) => [c.kind, c.x, c.y, c.w, c.h, c.layout, c.locked, c.chatOpen, c.autoH])).toEqual([
      ["hero", 0, 0, 8, 10, undefined, undefined, true, true],
      ["chatbot", 8, 0, 4, 9, "bar", true, false, true],
      ["product_grid", 0, 10, 12, 11, "carousel", undefined, true, false],
    ]);
    expect(isV2(toDoc(st, "t"))).toBe(true);
    expect(isV2({ schema_version: 1 })).toBe(false);
  });
  it("id aman dan unik", () => {
    expect(safeId("Business_Hours T1")).toBe("business-hours-t1");
    const d = toDoc({ ...st, cells: [st.cells[0]!, { ...st.cells[0]!, id: "hero-t1abc", x: 0, y: 20 }] }, "t");
    expect(new Set(d.blocks.map((b) => b.id)).size).toBe(2);
  });
});
