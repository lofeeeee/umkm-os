// Validasi kontak di peramban harus identik dengan backend (`crates/domain/src/business.rs`)
// dan CHECK constraint DB (migrasi 0042). Tes ini adalah salinan ketiga dari aturan itu;
// kalau satu sisi berubah, sisi lain harus ikut.
import { describe, expect, it } from "vitest";

import {
  emailErrorCode,
  isValidEmail,
  isValidPhone,
  normalizePhoneId,
  phoneErrorCode,
  PHONE_PATTERN,
} from "./contact";

describe("nomor telepon (hanya angka, awalan 0/62)", () => {
  it("menerima format lokal 0…", () => {
    expect(normalizePhoneId("081234567890")).toBe("081234567890");
    expect(normalizePhoneId("0812 3456 7890")).toBe("081234567890");
    expect(normalizePhoneId("0812-3456-7890")).toBe("081234567890");
    expect(normalizePhoneId("0812.3456.7890")).toBe("081234567890");
    expect(normalizePhoneId("(0812) 3456 7890")).toBe("081234567890");
    expect(isValidPhone("081234567890")).toBe(true);
  });

  it("menerima format internasional 62… (dengan atau tanpa +)", () => {
    expect(normalizePhoneId("6281234567890")).toBe("6281234567890");
    expect(normalizePhoneId("+6281234567890")).toBe("6281234567890");
    expect(normalizePhoneId("+62 812-3456-7890")).toBe("6281234567890");
    expect(isValidPhone("+6281234567890")).toBe(true);
  });

  it("menolak awalan lain", () => {
    expect(normalizePhoneId("81234567890")).toBeNull();
    expect(normalizePhoneId("+81234567890")).toBeNull();
    expect(normalizePhoneId("+6581234567890")).toBeNull();
    expect(normalizePhoneId("1234567890")).toBeNull();
  });

  it("menolak huruf, tanda, dan karakter lain", () => {
    expect(normalizePhoneId("0812abc45678")).toBeNull();
    expect(normalizePhoneId("0812#4567890")).toBeNull();
    expect(normalizePhoneId("0812 3456 789a")).toBeNull();
  });

  it("menolak panjang di luar 9–15 digit", () => {
    expect(normalizePhoneId("081234567")).toBe("081234567"); // 9 → batas bawah, ok
    expect(normalizePhoneId("0812345678")).toBe("0812345678"); // 10 → ok
    expect(normalizePhoneId("081234567890123")).toBe("081234567890123"); // 15 → batas atas, ok
    expect(normalizePhoneId("0812345678901234")).toBeNull(); // 16 → ditolak
    expect(normalizePhoneId("08123456")).toBeNull(); // 8 → ditolak
    expect(normalizePhoneId("0812345")).toBeNull(); // 7 → ditolak
  });

  it("kosong bukan sah dan bukan galat", () => {
    expect(normalizePhoneId("")).toBeNull();
    expect(phoneErrorCode("")).toBeNull();
    expect(phoneErrorCode("   ")).toBeNull();
  });

  it("kode galat: invalid_phone", () => {
    expect(phoneErrorCode("081234567890")).toBeNull();
    expect(phoneErrorCode("81234567890")).toBe("invalid_phone");
    expect(phoneErrorCode("abc")).toBe("invalid_phone");
  });
});

describe("email", () => {
  it("menerima email biasa", () => {
    for (const ok of ["a@toko.id", "nama.pemain@example.co.id", "tok+o@umkm.id", "x_y-z@mail.id"]) {
      expect(isValidEmail(ok), ok).toBe(true);
    }
  });

  it("menolak email rusak", () => {
    for (const bad of [
      "toko",
      "toko@",
      "@toko.id",
      "toko@id",
      "toko@.id",
      "toko@toko.",
      "toko@tok o.id",
      "tok..o@toko.id",
      ".toko@toko.id",
      "toko.@toko.id",
      "toko@-toko.id",
      "toko@toko-.id",
      "toko@toko.c",
      "toko@toko.1a",
      "a@b@c.id",
    ]) {
      expect(isValidEmail(bad), bad).toBe(false);
    }
  });

  it("menolak email terpanjang & yang berisi karakter kontrol", () => {
    expect(isValidEmail(`${"a".repeat(65)}@x.id`)).toBe(false);
    expect(isValidEmail(`${"a".repeat(64)}@x.id`)).toBe(true);
    expect(isValidEmail("a@b.id\n")).toBe(false);
    expect(isValidEmail("a@b.id\r")).toBe(false);
  });

  it("kode galat: invalid_email", () => {
    expect(emailErrorCode("")).toBeNull();
    expect(emailErrorCode("a@toko.id")).toBeNull();
    expect(emailErrorCode("bukan-email")).toBe("invalid_email");
  });
});

describe("atribut pattern HTML", () => {
  it("hanya menerima digit dengan awalan 0/62 (tanpa pemisah)", () => {
    const re = new RegExp(PHONE_PATTERN);
    expect(re.test("081234567890")).toBe(true);
    expect(re.test("6281234567890")).toBe(true);
    expect(re.test("+6281234567890")).toBe(true);
    expect(re.test("81234567890")).toBe(false);
    expect(re.test("0812 3456 7890")).toBe(false);
    expect(re.test("+6581234567890")).toBe(false);
  });
});
