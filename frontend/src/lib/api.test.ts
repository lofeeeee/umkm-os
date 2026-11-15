import { describe as suite, expect, it } from "vitest";

import { ApiError, describe } from "./api";

const err = (fields: { path: string; code: string }[]) =>
  new ApiError(422, "validation_failed", "Validasi gagal", { errors: fields });

suite("describe()", () => {
  it("memakai nama bidang Indonesia, termasuk dalam daftar", () => {
    expect(describe(err([{ path: "name", code: "required" }]))).toBe("Nama: wajib diisi");
    expect(describe(err([{ path: "addresses[1].address", code: "required" }]))).toBe("Alamat #2: wajib diisi");
    expect(describe(err([{ path: "", code: "too_long" }]))).toBe("Dokumen: terlalu panjang");
  });
  it("label per halaman menimpa default; bidang tak dikenal tampil apa adanya", () => {
    expect(describe(err([{ path: "name", code: "required" }]), { name: "Nama bisnis" })).toBe("Nama bisnis: wajib diisi");
    expect(describe(err([{ path: "x_y", code: "required" }]))).toBe("x_y: wajib diisi");
  });
});
