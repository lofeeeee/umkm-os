// Renderer situs tenant (ADR-0007, ADR-0014). Dipakai SAMA PERSIS oleh halaman publik dan
// preview dashboard, sehingga preview = hasil publik. Semua teks dirender sebagai teks
// (React escaping); tidak ada dangerouslySetInnerHTML (dijaga ESLint + tes).

import type { CSSProperties, ReactNode } from "react";

import { fontStack, safeColor, safeHref, text, waLink } from "./safe";
import type { Business, Json, OpenStatus, PageContent, Section } from "./types";

export interface SiteProps {
  content: PageContent;
  business: Business;
  status?: OpenStatus | null;
  /** `asset:<uuid>` -> URL absolut gambar; rujukan tak dikenal tidak dirender. */
  assetUrl: (ref: string) => string | undefined;
  /** Preview dashboard: section nonaktif tetap tampil dengan penanda. */
  preview?: boolean;
}

const DAYS = ["", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];

type Data = Section["data"];

function obj(v: Json | undefined): { [k: string]: Json } | undefined {
  return v && typeof v === "object" && !Array.isArray(v) ? v : undefined;
}

function list(v: Json | undefined): { [k: string]: Json }[] {
  return Array.isArray(v) ? (v.map(obj).filter(Boolean) as { [k: string]: Json }[]) : [];
}

function Img({ r, alt, assetUrl, className }: { r: Json | undefined; alt: string; assetUrl: SiteProps["assetUrl"]; className?: string }) {
  const src = typeof r === "string" ? assetUrl(r) : undefined;
  // eslint-disable-next-line @next/next/no-img-element -- static export, gambar dari API bertanda tangan
  return src ? <img className={className} src={src} alt={alt} loading="lazy" /> : null;
}

function hasImg(r: Json | undefined, assetUrl: SiteProps["assetUrl"]): boolean {
  return typeof r === "string" && !!assetUrl(r);
}

function Link({ href, children, className }: { href: Json | undefined; children: ReactNode; className?: string }) {
  const h = safeHref(href);
  if (!h) return <span className={className}>{children}</span>;
  const external = h.startsWith("http");
  return (
    <a className={className} href={h} {...(external ? { target: "_blank", rel: "noopener noreferrer nofollow" } : {})}>
      {children}
    </a>
  );
}

function Cta({ cta }: { cta: Json | undefined }) {
  const c = obj(cta);
  if (!c) return null;
  return <Link className="s-btn" href={c.href}>{text(c.label)}</Link>;
}

function Title({ v }: { v: Json | undefined }) {
  const t = text(v);
  return t ? <h2 className="s-title">{t}</h2> : null;
}

function Paragraphs({ v }: { v: Json | undefined }) {
  return (
    <>
      {text(v)
        .split("\n")
        .filter((p) => p.trim())
        .map((p, i) => <p key={i}>{p}</p>)}
    </>
  );
}

function SectionBody({ s, props }: { s: Section; props: SiteProps }) {
  const d: Data = s.data;
  const { business: b, assetUrl } = props;
  switch (s.type) {
    case "header":
      return (
        <div className="s-headbar">
          <div className="s-brand">
            <Img r={d.logo ?? b.profile.logo ?? undefined} alt="" assetUrl={assetUrl} className="s-logo" />
            <span>{text(d.title) || b.profile.name}</span>
          </div>
          <nav className="s-nav">
            {list(d.nav).map((n, i) => <Link key={i} href={n.href}>{text(n.label)}</Link>)}
          </nav>
        </div>
      );
    case "hero":
      return (
        <div className={`s-hero${hasImg(d.image ?? b.profile.cover ?? undefined, assetUrl) ? " has-media" : ""}`}>
          <div className="s-hero-text">
            <h1>{text(d.title)}</h1>
            {text(d.subtitle) && <p className="s-lead">{text(d.subtitle)}</p>}
            <Cta cta={d.cta} />
          </div>
          <Img r={d.image ?? b.profile.cover ?? undefined} alt="" assetUrl={assetUrl} className="s-hero-img" />
        </div>
      );
    case "about":
      return (
        <div className={`s-split${hasImg(d.image, assetUrl) ? " has-media" : ""}`}>
          <div>
            <Title v={d.title} />
            <Paragraphs v={d.body} />
          </div>
          <Img r={d.image} alt="" assetUrl={assetUrl} className="s-media" />
        </div>
      );
    case "product_grid":
      // Katalog dibangun di Phase 3; sampai itu tampilkan keadaan kosong yang jujur.
      return (
        <div>
          <Title v={d.title} />
          <p className="s-muted">Katalog produk segera hadir.</p>
        </div>
      );
    case "promotion":
      return (
        <div className={`s-promo${hasImg(d.image, assetUrl) ? " has-media" : ""}`}>
          <div>
            <h2 className="s-title">{text(d.title)}</h2>
            <Paragraphs v={d.body} />
            <Cta cta={d.cta} />
          </div>
          <Img r={d.image} alt="" assetUrl={assetUrl} className="s-media" />
        </div>
      );
    case "gallery":
      return (
        <div>
          <Title v={d.title} />
          <div className="s-gallery">
            {list(d.images).map((g, i) => (
              <figure key={i}>
                <Img r={g.asset} alt={text(g.caption)} assetUrl={assetUrl} />
                {text(g.caption) && <figcaption>{text(g.caption)}</figcaption>}
              </figure>
            ))}
          </div>
        </div>
      );
    case "testimonials":
      return (
        <div>
          <Title v={d.title} />
          <div className="s-cards">
            {list(d.items).map((t, i) => (
              <blockquote key={i} className="s-card">
                <p>“{text(t.quote)}”</p>
                <cite>{text(t.name)}</cite>
              </blockquote>
            ))}
          </div>
        </div>
      );
    case "business_hours":
      return (
        <div>
          <Title v={d.title} />
          {props.status && (
            <p className={`s-status s-status-${props.status.status}`}>
              {props.status.status === "open" ? "Buka sekarang" : "Tutup sekarang"}
            </p>
          )}
          {b.hours.length === 0 ? (
            <p className="s-muted">Jam buka belum diatur.</p>
          ) : (
            <table className="s-hours">
              <tbody>
                {[1, 2, 3, 4, 5, 6, 7].map((day) => {
                  const h = b.hours.find((x) => x.day_of_week === day);
                  return (
                    <tr key={day}>
                      <th scope="row">{DAYS[day]}</th>
                      <td>{h ? `${h.open} – ${h.close}` : "Tutup"}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      );
    case "location":
      return (
        <div>
          <Title v={d.title} />
          {b.addresses.length === 0 && <p className="s-muted">Alamat belum diatur.</p>}
          <ul className="s-plain">
            {b.addresses.map((a, i) => (
              <li key={i}>
                {a.label && <strong>{a.label}: </strong>}
                {a.address}
                {d.show_map === true && a.latitude != null && a.longitude != null && (
                  <>
                    {" "}
                    <Link href={`https://www.openstreetmap.org/?mlat=${a.latitude}&mlon=${a.longitude}#map=17/${a.latitude}/${a.longitude}`}>
                      Lihat peta
                    </Link>
                  </>
                )}
              </li>
            ))}
          </ul>
          {(b.profile.pickup_available || b.profile.delivery_available) && (
            <p className="s-muted">
              {[b.profile.pickup_available && "Bisa ambil di tempat", b.profile.delivery_available && "Bisa diantar"]
                .filter(Boolean)
                .join(" · ")}
              {b.profile.fulfillment_note ? ` — ${b.profile.fulfillment_note}` : ""}
            </p>
          )}
        </div>
      );
    case "contact": {
      const p = b.profile;
      const wa = waLink(p.whatsapp);
      const socials = [...list(d.socials), ...p.socials.map((x) => ({ platform: x.platform, url: x.url }))];
      const any = (d.show_whatsapp === true && wa) || (d.show_phone === true && p.phone) || (d.show_email === true && p.email) || socials.length > 0;
      return (
        <div>
          <Title v={d.title} />
          {!any && <p className="s-muted">Kontak belum diatur.</p>}
          <div className="s-contact">
            {d.show_whatsapp === true && wa && <Link className="s-btn" href={wa}>WhatsApp</Link>}
            {d.show_phone === true && p.phone && <Link href={`tel:${p.phone}`}>{p.phone}</Link>}
            {d.show_email === true && p.email && <Link href={`mailto:${p.email}`}>{p.email}</Link>}
          </div>
          <div className="s-social">
            {socials.map((x, i) => (
              <Link key={i} href={x.url}>{text(x.platform)}</Link>
            ))}
          </div>
        </div>
      );
    }
    case "footer":
      return (
        <div className="s-foot">
          <span>{text(d.text)}</span>
          <span className="s-muted">© {b.profile.name}</span>
        </div>
      );
    default:
      return null;
  }
}

export function Site(props: SiteProps) {
  const { content, preview } = props;
  const style = {
    "--s-primary": safeColor(content.theme?.primary),
    "--s-font": fontStack(content.theme?.font),
  } as CSSProperties;
  const sections = (content.sections ?? []).filter((s) => preview || s.enabled);
  return (
    <div className="site" style={style} data-template={content.template}>
      {sections.map((s) => (
        <section
          key={s.id}
          id={s.id}
          className={`s s--${s.type}${s.enabled ? "" : " s-disabled"}`}
          data-section-type={s.type}
        >
          {preview && !s.enabled && <div className="s-disabled-badge">Nonaktif — tidak tampil di publik</div>}
          <div className="s-inner">
            <SectionBody s={s} props={props} />
          </div>
        </section>
      ))}
    </div>
  );
}
