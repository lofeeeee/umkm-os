// Sinkronisasi pesan server (Rust) ↔ kamus English: setiap pesan yang bisa sampai ke pengguna punya
// terjemahan, dan teks persetujuan AI di server sama dengan yang diterjemahkan.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { CONSENT_EN, SERVER_EN } from "./en/server-6";
import { getLocale, loadEnglish, setLocale } from "./index";

// Kamus EN dimuat malas (chunk terpisah); tes memuatnya dulu agar setLocale("en") langsung berlaku.
beforeAll(() => loadEnglish());
import { consentText, serverMessage } from "./server";

const ROOT = resolve(__dirname, "../../..");

function rs(dir: string, out: string[] = []): string[] {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) { if (f !== "tests") rs(p, out); } else if (f.endsWith(".rs")) out.push(p);
  }
  return out;
}

/** Pesan yang dikirim API: helper error berargumen literal + atribut #[error] tipe yang diteruskan ke klien. */
function serverMessages(): Set<string> {
  const api = rs(join(ROOT, "apps/api/src"));
  const domain = [...rs(join(ROOT, "crates/domain/src")), join(ROOT, "crates/infrastructure/src/payment_repo.rs")];
  const pats: [string[], RegExp, number][] = [
    [api, /\b(forbidden|mfa_required|conflict|validation)\(\s*(?:"[^"]*",\s*)?"((?:[^"\\]|\\.)*)"\s*[,)]/g, 2],
    [api, /with_message\(\s*"((?:[^"\\]|\\.)*)"/g, 1],
    [api, /msg\(\s*ErrorCode::\w+,\s*"((?:[^"\\]|\\.)*)"/g, 1],
    [[...domain, ...api], /#\[error\("((?:[^"\\]|\\.)*)"/g, 1],
  ];
  const out = new Set<string>();
  for (const [files, re, g] of pats) {
    for (const f of files) {
      for (const m of readFileSync(f, "utf8").matchAll(re)) {
        const raw = m[g]!;
        if (/^[a-z_]+$/.test(raw)) continue; // nama field/alasan, bukan pesan
        let i = 0;
        out.add(raw.replace(/\{[a-z_0-9]*\}/g, () => `{${i++}}`).replace(/\\"/g, '"'));
      }
    }
  }
  return out;
}

describe("pesan server", () => {
  it("setiap pesan di kode Rust punya terjemahan English", () => {
    const missing = [...serverMessages()].filter((m) => !(m in SERVER_EN));
    expect(missing, `Tambahkan ke src/i18n/en/server-6.ts:\n${missing.join("\n")}`).toEqual([]);
  });

  it("tidak ada terjemahan yatim (pesan sudah tidak ada di server)", () => {
    const now = serverMessages();
    expect(Object.keys(SERVER_EN).filter((k) => !now.has(k))).toEqual([]);
  });

  it("teks persetujuan AI di server sama dengan kunci terjemahan", () => {
    const src = readFileSync(join(ROOT, "crates/domain/src/ai/consent.rs"), "utf8");
    const m = /pub const TEXT: &str = "((?:[^"\\]|\\.)*)";/s.exec(src);
    const text = m![1]!.replace(/\\\n\s*/g, "").replace(/\\"/g, '"');
    expect(Object.keys(CONSENT_EN)).toEqual([text]);
  });

  it("menerjemahkan pesan tetap, berparameter, dan generik", () => {
    const before = getLocale();
    try {
      setLocale("en", false);
      expect(serverMessage("CONFLICT", "Draf sudah diubah orang lain; muat ulang")).toBe("Someone else changed the draft; reload");
      expect(serverMessage("CONFLICT", "stok tidak cukup (tersedia 2, diminta 5)")).toBe("Not enough stock (available 2, requested 5)");
      expect(serverMessage("CONFLICT", "Conflict")).toBe("The data has changed. Reload and try again.");
      expect(serverMessage("BAD_REQUEST", "pesan baru yang belum dikenal")).toBe("Pesan baru yang belum dikenal");
      setLocale("id", false);
      expect(serverMessage("CONFLICT", "stok tidak cukup (tersedia 2, diminta 5)")).toBe("Stok tidak cukup (tersedia 2, diminta 5)");
      expect(serverMessage("VALIDATION_FAILED", "Validation failed")).toBe("Periksa kembali isian Anda.");
      expect(consentText("x")).toBe("x");
    } finally {
      setLocale(before, false);
    }
  });
});
