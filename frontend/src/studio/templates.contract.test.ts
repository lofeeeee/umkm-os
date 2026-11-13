// Kontrak template Studio ↔ validator Rust: setiap template dikonversi ke dokumen v2 dan dibandingkan
// dengan fixture yang divalidasi crates/domain/tests/frontend_contract.rs.
// Perbarui fixture: UPDATE_FIXTURES=1 npx vitest run src/studio/templates.contract.test.ts
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { toDoc } from "./doc";
import { TEMPLATES } from "./templates";

const FIXTURE = resolve(__dirname, "../../../crates/domain/tests/fixtures/studio_templates.json");

function docs() {
  return Object.fromEntries(TEMPLATES.map((t) => {
    const r = t.build({ name: "Kopi Senja Bandung", description: "Kedai kopi rumahan dengan biji pilihan." });
    const d = toDoc({ ...r, installed: [] }, "modern_store");
    // id acak → deterministik untuk fixture
    d.blocks = d.blocks.map((b, i) => ({ ...b, id: `${b.kind.replace(/_/g, "-")}-${i + 1}` }));
    return [t.key, d];
  }));
}

describe("kontrak template Studio", () => {
  it("fixture sama dengan keluaran template", () => {
    const now = JSON.stringify(docs(), null, 2) + "\n";
    if (process.env.UPDATE_FIXTURES) writeFileSync(FIXTURE, now);
    expect(readFileSync(FIXTURE, "utf8")).toBe(now);
  });
});
