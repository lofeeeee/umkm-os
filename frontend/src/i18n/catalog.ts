// Ekstraktor kunci terjemahan dari kode sumber (dipakai catalog.test.ts dan skrip pemeliharaan).
// Kunci = argumen literal pertama t()/tx()/S()/m(), atribut `s` pada <Tr>, dan nilai objek di dalam tmap().

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import ts from "typescript";

// Kamus & mesin i18n sendiri dilewati; komponen React-nya (react.tsx) tetap diekstrak.
const SKIP = /(\.test\.|\/i18n\/(en|catalog|index)|\.d\.ts$)/;

function walk(dir: string, out: string[] = []): string[] {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx)$/.test(f) && !SKIP.test(p)) out.push(p);
  }
  return out;
}

const text = (n: ts.Node | undefined): string | null =>
  n && (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) ? n.text : null;

/** Semua kunci beserta lokasi pertamanya. */
export function extractKeys(srcDir: string): Map<string, string> {
  const keys = new Map<string, string>();
  for (const file of walk(srcDir)) {
    const src = readFileSync(file, "utf8");
    const sf = ts.createSourceFile(file, src, ts.ScriptTarget.Latest, true, file.endsWith("x") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    const add = (s: string | null, n: ts.Node) => {
      if (s === null || keys.has(s)) return;
      keys.set(s, `${file}:${sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1}`);
    };
    const visit = (n: ts.Node) => {
      if (ts.isCallExpression(n)) {
        const callee = n.expression.getText(sf);
        if (/^(t|tx|S|m)$/.test(callee)) add(text(n.arguments[0]), n);
        if (callee === "tmap" && n.arguments[0] && ts.isObjectLiteralExpression(n.arguments[0])) {
          for (const p of n.arguments[0].properties) if (ts.isPropertyAssignment(p)) add(text(p.initializer), p);
        }
      }
      if (ts.isJsxAttribute(n) && n.name.getText(sf) === "s" && ts.isJsxSelfClosingElement(n.parent.parent) && n.parent.parent.tagName.getText(sf) === "Tr") {
        add(text(n.initializer ?? undefined), n);
      }
      ts.forEachChild(n, visit);
    };
    visit(sf);
  }
  return keys;
}

/** Placeholder {0}, {1}, … yang dipakai sebuah teks (terurut, unik). */
export const placeholders = (s: string) => [...new Set(s.match(/\{\d+\}/g) ?? [])].sort();
