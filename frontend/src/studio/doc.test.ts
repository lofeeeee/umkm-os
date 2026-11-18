import { describe, expect, it } from "vitest";

import { fromDoc, isV2, safeId, siteThemeOf, toDoc, withSiteTheme, type StudioState } from "./doc";
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
  it("toDoc dan fromDoc mendukung navbar, background, dan drawings", () => {
    const customSt: StudioState = {
      ...st,
      navbarStyle: "side",
      navbarSticky: true,
      hideBadge: true,
      backgroundImage: "asset:6f9619ff-8b86-d011-b42d-00c04fc964ff",
      backgroundSize: "contain",
      backgroundPosition: "top left",
      backgroundOpacity: 85,
      drawings: [
        { id: "stroke-1", target: "background", color: "#ff0000", size: 4, opacity: 1, points: [[0, 0], [10, 10]] },
      ],
    };
    const d = toDoc(customSt, "custom");
    expect(d.theme.navbar_style).toBe("side");
    expect(d.theme.navbar_sticky).toBe(true);
    expect(d.theme.hide_badge).toBe(true);
    expect(fromDoc(d, () => ({ minW: 1, minH: 1 })).hideBadge).toBe(true);
    expect(d.theme.background_image).toBe("asset:6f9619ff-8b86-d011-b42d-00c04fc964ff");
    expect(d.theme.background_size).toBe("contain");
    expect(d.theme.background_position).toBe("top left");
    expect(d.theme.background_opacity).toBe(85);
    expect(d.drawings).toEqual([
      { id: "stroke-1", target: "background", color: "#ff0000", size: 4, opacity: 1, points: [[0, 0], [10, 10]] },
    ]);

    const back = fromDoc(d, (k) => ({ minW: pluginDef(k).minW, minH: pluginDef(k).minH }));
    expect(back.navbarStyle).toBe("side");
    expect(back.navbarSticky).toBe(true);
    expect(back.backgroundImage).toBe("asset:6f9619ff-8b86-d011-b42d-00c04fc964ff");
    expect(back.backgroundSize).toBe("contain");
    expect(back.backgroundPosition).toBe("top left");
    expect(back.backgroundOpacity).toBe(85);
    expect(back.drawings).toEqual([
      { id: "stroke-1", target: "background", color: "#ff0000", size: 4, opacity: 1, points: [[0, 0], [10, 10]] },
    ]);
  });
  it("toDoc menghilangkan field kosong/undefined", () => {
    const emptySt: StudioState = {
      ...st,
      backgroundImage: "   ",
      backgroundPosition: "",
      drawings: [],
    };
    const d = toDoc(emptySt, "clean");
    expect(d.theme).not.toHaveProperty("navbar_style");
    expect(d.theme).not.toHaveProperty("hide_badge");
    expect(d.theme).not.toHaveProperty("navbar_sticky");
    expect(d.theme).not.toHaveProperty("background_image");
    expect(d.theme).not.toHaveProperty("background_size");
    expect(d.theme).not.toHaveProperty("background_position");
    expect(d.theme).not.toHaveProperty("background_opacity");
    expect(d).not.toHaveProperty("drawings");
  });
});

describe("tema situs (brand kit)", () => {
  const base: StudioState = { primary: "#4f46e5", surface: "light", font: "inter", installed: [], cells: [] };
  it("kunci gaya baru ikut dokumen & tema situs; bawaan tidak dikirim", () => {
    const st: StudioState = { ...base, headingFont: "playfair", headingWeight: 700, accent: "#f59e0b", gradient: true, radius: "round", motion: "subtle", texture: "grain", pageTransition: true };
    const d = toDoc(st, "t");
    expect(d.theme).toMatchObject({ heading_font: "playfair", heading_weight: 700, accent: "#F59E0B", gradient: true, radius: "round", motion: "subtle", texture: "grain", page_transition: true });
    expect(fromDoc(d, () => ({ minW: 1, minH: 1 }))).toMatchObject({ headingFont: "playfair", radius: "round", pageTransition: true });
    expect(siteThemeOf({ ...base, radius: "soft", motion: "none", texture: "none" })).toEqual({ primary: "#4F46E5", font: "inter", surface: "light", text_scale: 1 });
  });
  it("halaman yang dibuka mengikuti tema situs (gaya lama halaman diganti)", () => {
    const page: StudioState = { ...base, primary: "#000000", texture: "dots", navbarSticky: true };
    const out = withSiteTheme(page, { primary: "#0F766E", font: "fraunces", accent: "#F59E0B" });
    expect(out).toMatchObject({ primary: "#0F766E", font: "fraunces", accent: "#F59E0B", texture: undefined, navbarSticky: undefined });
    expect(withSiteTheme(page, null)).toBe(page);
  });
});
