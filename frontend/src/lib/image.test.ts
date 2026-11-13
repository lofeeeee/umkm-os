import { describe, expect, it } from "vitest";

import { ImagePrepError, MAX_UPLOAD_BYTES, prepareImage } from "./image";

describe("prepareImage", () => {
  it("file kecil dengan tipe diterima dikirim apa adanya", async () => {
    const f = new File([new Uint8Array(1000)], "logo.png", { type: "image/png" });
    expect(await prepareImage(f)).toBe(f);
  });
  it("bukan gambar ditolak dengan pesan jelas", async () => {
    const f = new File(["x"], "data.pdf", { type: "application/pdf" });
    await expect(prepareImage(f)).rejects.toBeInstanceOf(ImagePrepError);
  });
  it("batas aman di bawah body limit API (2 162 688 byte)", () => {
    expect(MAX_UPLOAD_BYTES).toBeLessThan(2_162_688 - 64_000);
  });
});
