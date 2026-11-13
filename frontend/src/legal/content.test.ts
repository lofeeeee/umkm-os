import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { LEGAL_VERSION, PRIVACY, TERMS } from "./content";

// Versi yang tercatat saat persetujuan (server, config [legal]) HARUS sama dengan teks yang ditampilkan.
describe("teks legal D-12", () => {
  for (const env of ["development", "staging", "production"]) {
    it(`versi sama dengan config/${env}.toml`, () => {
      const toml = readFileSync(join(__dirname, "../../../config", `${env}.toml`), "utf8");
      const get = (k: string) => toml.match(new RegExp(`^${k}\\s*=\\s*"([^"]+)"`, "m"))?.[1];
      expect(get("privacy_policy_version")).toBe(LEGAL_VERSION);
      expect(get("terms_of_service_version")).toBe(LEGAL_VERSION);
    });
  }
  it("tanpa teks placeholder/draf di isi dokumen", () => {
    const all = [...PRIVACY, ...TERMS].flatMap((s) => [s.title, ...s.paragraphs]).join("\n");
    expect(all).not.toMatch(/\bplaceholder\b|\bdraf\b|\bbelum berlaku\b/i);
    expect(PRIVACY.length).toBeGreaterThanOrEqual(8);
    expect(TERMS.length).toBeGreaterThanOrEqual(8);
  });
});
