// Penampil teks legal (D-12) — dipakai dashboard & situs toko. Teks biasa (tanpa HTML), semua dari content.ts.
import { LEGAL_EFFECTIVE, LEGAL_VERSION, type Section } from "./content";

export function LegalDoc({ sections }: { sections: Section[] }) {
  return (
    <article className="legal-doc">
      <p className="legal-meta">Versi {LEGAL_VERSION} · berlaku sejak {LEGAL_EFFECTIVE}</p>
      {sections.map((s) => (
        <section key={s.title}>
          <h2>{s.title}</h2>
          {s.paragraphs.map((p, i) => <p key={i}>{p}</p>)}
        </section>
      ))}
    </article>
  );
}
