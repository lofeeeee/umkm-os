// Pasca-build: hitung SHA-256 setiap <script> inline di out/**/*.html dan tulis snippet Caddy
// `out/csp.caddy` berisi CSP ketat (tanpa 'unsafe-inline' untuk skrip). ADR-0014 butir 6.
// {$ROOT_DOMAIN} / {$PUBLIC_API_ORIGIN} disubstitusi Caddy saat memuat konfigurasi.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const OUT = new URL("../out/", import.meta.url).pathname;
const walk = (d) => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? walk(join(d, f)) : [join(d, f)]));
const hashes = new Set();
let pages = 0;
for (const f of walk(OUT).filter((x) => x.endsWith(".html"))) {
  pages++;
  const html = readFileSync(f, "utf8");
  for (const m of html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)) {
    hashes.add(`'sha256-${createHash("sha256").update(m[1], "utf8").digest("base64")}'`);
  }
}
if (pages === 0) throw new Error("csp.mjs: tidak ada HTML di out/ (jalankan next build dulu)");
const script = ["'self'", ...[...hashes].sort()].join(" ");
const common = `default-src 'self'; script-src ${script}; style-src 'self' 'unsafe-inline'; font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`;
const snippet = `# DIBUAT OTOMATIS oleh frontend/scripts/csp.mjs — jangan diedit. ${hashes.size} hash dari ${pages} halaman.
(csp_app) {
\theader Content-Security-Policy "${common}; connect-src 'self' {$PUBLIC_API_ORIGIN}; img-src 'self' {$PUBLIC_API_ORIGIN} data: blob:"
}
(csp_site) {
\theader Content-Security-Policy "${common}; connect-src 'self'; img-src 'self' {$PUBLIC_API_ORIGIN} data:"
}
`;
writeFileSync(join(OUT, "csp.caddy"), snippet);
console.log(`csp.mjs: ${hashes.size} hash inline script dari ${pages} halaman -> out/csp.caddy`);
