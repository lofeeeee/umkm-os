// Plugin Isi artikel & Daftar artikel (blog). Teks artikel memakai penanda ringan yang aman — diurai menjadi
// elemen React (tanpa innerHTML): `## Judul`, `### Subjudul`, `- daftar`, `1. daftar bernomor`, `> kutipan`,
// `**tebal**`, `*miring*`, `[teks](tautan)`. Baris kosong memisahkan paragraf.

import type { ReactNode } from "react";

import { intlLocale, t as tx } from "@/i18n";

import { onImgError, safeHref } from "./safe";

export interface ArticleMeta { excerpt?: string; cover?: string; author?: string; published_at?: string; tags?: string[] }

type Node =
  | { t: "h2" | "h3"; text: string; id: string }
  | { t: "p" | "quote"; text: string }
  | { t: "ul" | "ol"; items: string[] };

export const slugify = (s: string) => s.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 60) || "bagian";

export function parseArticle(body: string): Node[] {
  const out: Node[] = [];
  const used = new Map<string, number>();
  const id = (s: string) => {
    const base = slugify(s);
    const n = used.get(base) ?? 0;
    used.set(base, n + 1);
    return n ? `${base}-${n + 1}` : base;
  };
  let para: string[] = [];
  const flush = () => { if (para.length) out.push({ t: "p", text: para.join(" ") }); para = []; };
  for (const raw of body.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line) { flush(); continue; }
    let m: RegExpMatchArray | null;
    if ((m = line.match(/^(#{2,3})\s+(.+)$/))) {
      flush();
      out.push({ t: m[1] === "##" ? "h2" : "h3", text: m[2]!, id: id(m[2]!) });
    } else if ((m = line.match(/^[-*]\s+(.+)$/)) || (m = line.match(/^\d+[.)]\s+(.+)$/))) {
      flush();
      const kind = /^\d/.test(line) ? "ol" : "ul";
      const last = out[out.length - 1];
      if (last && last.t === kind) last.items.push(m[1]!);
      else out.push({ t: kind, items: [m[1]!] });
    } else if ((m = line.match(/^>\s?(.*)$/))) {
      flush();
      const last = out[out.length - 1];
      if (last && last.t === "quote") last.text += ` ${m[1]}`;
      else out.push({ t: "quote", text: m[1] ?? "" });
    } else {
      para.push(line);
    }
  }
  flush();
  return out;
}

/** Penanda sebaris: **tebal**, *miring*, [teks](tautan aman). Selain itu teks biasa. */
export function inline(s: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /\*\*([^*]+)\*\*|\*([^*]+)\*|\[([^\]]+)\]\(([^)\s]+)\)/g;
  let last = 0;
  let k = 0;
  for (let m = re.exec(s); m; m = re.exec(s)) {
    if (m.index > last) out.push(s.slice(last, m.index));
    if (m[1]) out.push(<strong key={k++}>{m[1]}</strong>);
    else if (m[2]) out.push(<em key={k++}>{m[2]}</em>);
    else {
      const href = safeHref(m[4]);
      out.push(href ? <a key={k++} href={href} rel={href.startsWith("http") ? "noopener nofollow" : undefined}>{m[3]}</a> : m[3]);
    }
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push(s.slice(last));
  return out;
}

const fmtDate = (d?: string) => {
  if (!d || !/^\d{4}-\d{2}-\d{2}$/.test(d)) return undefined;
  try { return new Date(`${d}T00:00:00`).toLocaleDateString(intlLocale(), { day: "numeric", month: "long", year: "numeric" }); } catch { return d; }
};
const readingMinutes = (body: string) => Math.max(1, Math.round(body.split(/\s+/).filter(Boolean).length / 200));

export function ArticleBody({ body, toc, title, meta, coverUrl }: { body: string; toc: boolean; title?: string; meta?: ArticleMeta; coverUrl?: string }) {
  const nodes = parseArticle(body);
  const heads = nodes.filter((n): n is Extract<Node, { t: "h2" | "h3" }> => n.t === "h2" || n.t === "h3");
  const date = fmtDate(meta?.published_at);
  return (
    <article className="s-article">
      {meta && (
        <header className="s-article-head">
          {meta.tags?.length ? <p className="s-article-tags">{meta.tags.map((t) => <span key={t}>{t}</span>)}</p> : null}
          {title && <h1>{title}</h1>}
          {meta.excerpt && <p className="s-article-lead">{meta.excerpt}</p>}
          <p className="s-article-meta">
            {meta.author && <span>{meta.author}</span>}
            {date && <time dateTime={meta.published_at}>{date}</time>}
            <span>{tx("{0} menit baca", readingMinutes(body))}</span>
          </p>
          {/* eslint-disable-next-line @next/next/no-img-element -- aset tenant bertanda tangan */}
          {coverUrl && <img className="s-article-cover" src={coverUrl} alt="" loading="eager" decoding="async" onError={onImgError} />}
        </header>
      )}
      {toc && heads.length > 1 && (
        <nav className="s-article-toc" aria-label={tx("Daftar isi")}>
          <strong>{tx("Daftar isi")}</strong>
          <ol>{heads.map((h) => <li key={h.id} data-level={h.t}><a href={`#${h.id}`}>{h.text}</a></li>)}</ol>
        </nav>
      )}
      <div className="s-article-body">
        {nodes.map((n, i) => {
          switch (n.t) {
            case "h2": return <h2 key={i} id={n.id}>{inline(n.text)}</h2>;
            case "h3": return <h3 key={i} id={n.id}>{inline(n.text)}</h3>;
            case "quote": return <blockquote key={i}>{inline(n.text)}</blockquote>;
            case "ul": return <ul key={i}>{n.items.map((x, j) => <li key={j}>{inline(x)}</li>)}</ul>;
            case "ol": return <ol key={i}>{n.items.map((x, j) => <li key={j}>{inline(x)}</li>)}</ol>;
            default: return <p key={i}>{inline(n.text)}</p>;
          }
        })}
      </div>
    </article>
  );
}

export interface BlogEntry { slug: string; title: string; article: ArticleMeta }

export function BlogList({ title, entries, limit, layout, assetUrl, editing }: {
  title?: string; entries: BlogEntry[]; limit: number; layout: "grid" | "list"; assetUrl: (r: string) => string | undefined; editing?: boolean;
}) {
  const list = [...entries].sort((a, b) => (b.article.published_at ?? "").localeCompare(a.article.published_at ?? "")).slice(0, limit);
  const shown: BlogEntry[] = list.length || !editing ? list : [1, 2, 3].map((i) => ({ slug: `#contoh-${i}`, title: tx("Judul artikel contoh {0}", i), article: { excerpt: tx("Artikel yang Anda terbitkan muncul di sini otomatis, terbaru lebih dulu."), published_at: "2026-11-0" + i } }));
  return (
    <section className="s s-blog">
      <div className="s-inner">
        {title && <h2 className="s-title">{title}</h2>}
        {shown.length === 0 ? <p className="s-muted">{tx("Belum ada artikel.")}</p> : (
          <ul className={`s-blog-list s-blog-${layout}`}>
            {shown.map((e) => {
              const cover = e.article.cover ? assetUrl(e.article.cover) : undefined;
              return (
                <li key={e.slug}>
                  <a className="s-blog-card" href={e.slug.startsWith("#") ? undefined : `/${e.slug}`}>
                    {/* eslint-disable-next-line @next/next/no-img-element -- aset tenant bertanda tangan */}
                    {cover ? <img src={cover} alt="" loading="lazy" decoding="async" onError={onImgError} /> : <span className="s-blog-noimg" aria-hidden="true" />}
                    <span className="s-blog-text">
                      {fmtDate(e.article.published_at) && <time dateTime={e.article.published_at}>{fmtDate(e.article.published_at)}</time>}
                      <strong>{e.title}</strong>
                      {e.article.excerpt && <span className="s-muted">{e.article.excerpt}</span>}
                      <span className="s-blog-more">{tx("Baca selengkapnya →")}</span>
                    </span>
                  </a>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
