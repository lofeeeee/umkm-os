import { describe, expect, it } from "vitest";

import { LAT_RANGE, LNG_RANGE, coordInRange, coordNumber, parseCoord } from "./coords";

describe("parseCoord()", () => {
  it("menyimpan teks koordinat apa adanya", () => {
    expect(parseCoord("6.2")).toBe("6.2");
    expect(parseCoord("-6.2")).toBe("-6.2");
    expect(parseCoord("106.8")).toBe("106.8");
    expect(parseCoord("-106.8")).toBe("-106.8");
    expect(parseCoord("0")).toBe("0");
  });

  it("tanda minus tak lengkap tetap tampil (bug lama: Number('-') = NaN)", () => {
    // Inilah bug lama: `Number("-")` = NaN, jadi isian menampilkan "NaN".
    expect(parseCoord("-")).toBe("-");
    expect(parseCoord("+")).toBe("+");
    expect(parseCoord("-6.")).toBe("-6.");
    expect(parseCoord("-.5")).toBe("-.5");
  });

  it("kosong → null; teks bukan koordinat → null (tidak tersimpan ke server)", () => {
    expect(parseCoord("")).toBeNull();
    expect(parseCoord("   ")).toBeNull();
    expect(parseCoord("abc")).toBeNull();
    expect(parseCoord("6,2")).toBeNull();
    expect(parseCoord("1e5")).toBeNull();
    expect(parseCoord("1 2")).toBeNull();
  });
});

describe("coordNumber()", () => {
  it("mengubah teks menjadi angka, negatif tetap negatif", () => {
    expect(coordNumber("-6.2")).toBe(-6.2);
    expect(coordNumber("  -106.8 ")).toBe(-106.8);
    expect(coordNumber(106.8)).toBe(106.8);
    expect(coordNumber("-0.5")).toBe(-0.5);
  });

  it("null untuk kosong, teks tak lengkap, atau bukan angka", () => {
    expect(coordNumber(null)).toBeNull();
    expect(coordNumber(undefined)).toBeNull();
    expect(coordNumber("")).toBeNull();
    expect(coordNumber("-")).toBeNull();
    expect(coordNumber("abc")).toBeNull();
    expect(coordNumber(Number.NaN)).toBeNull();
  });

  it("angka desimal yang belum selesai tetap terbaca (-6. → -6)", () => {
    expect(coordNumber("-6.")).toBe(-6);
    expect(coordNumber("6.")).toBe(6);
    expect(coordNumber("-.5")).toBe(-0.5);
  });
});

describe("coordInRange()", () => {
  it("menerima nilai negatif dalam rentang", () => {
    expect(coordInRange(-6.2, LAT_RANGE)).toBe(true);
    expect(coordInRange(-90, LAT_RANGE)).toBe(true);
    expect(coordInRange(-180, LNG_RANGE)).toBe(true);
    expect(coordInRange(106.8, LNG_RANGE)).toBe(true);
  });

  it("menolak di luar rentang", () => {
    expect(coordInRange(-90.1, LAT_RANGE)).toBe(false);
    expect(coordInRange(90.1, LAT_RANGE)).toBe(false);
    expect(coordInRange(-180.1, LNG_RANGE)).toBe(false);
    expect(coordInRange(180.1, LNG_RANGE)).toBe(false);
  });

  it("kosong (null) dianggap sah karena koordinat opsional", () => {
    expect(coordInRange(null, LAT_RANGE)).toBe(true);
  });
});
