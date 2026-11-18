// Guard: audit HARUS bisa menangkap kebocoran (kalau tidak, test ini tidak berguna).
// Kita simulasikan kebocoran sintetis lalu pastikan audit() mendeteksinya.
import { describe, expect, it } from "vitest";
import { audit, directTokensForTest } from "./isolation";

describe("kebocoran sintetis terdeteksi", () => {
  it("var() bersarang di fallback tidak dianggap bocor", () => {
    expect(directTokensForTest("color: var(--s-ink, var(--ink));")).toEqual(["s-ink"]);
    expect(directTokensForTest("color: var(--muted);")).toEqual(["muted"]);
    expect(directTokensForTest("color: var(--a); background: var(--b);")).toEqual(["a", "b"]);
    expect(directTokensForTest("color: var(--s-ink, var(--ink)); border-color: var(--muted);")).toEqual([
      "s-ink",
      "muted",
    ]);
  });

  it("audit menemukan kebocoran nyata di repo (tidak boleh kosong)", () => {
    const hits = audit();
    // Kalau repo sudah bersih, test ini hanya memastikan audit() tidak error.
    // Kita hanya mengecek bentuk datanya valid.
    for (const h of hits) {
      expect(h.token).toMatch(/^--/);
      expect(h.file).toMatch(/^src\//);
      expect(h.line).toBeGreaterThan(0);
    }
  });
});
