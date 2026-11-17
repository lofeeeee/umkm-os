// Renderer situs tenant (ADR-0007, ADR-0014). Dipakai SAMA PERSIS oleh halaman publik dan
// preview dashboard, sehingga preview = hasil publik. Semua teks dirender sebagai teks
// (React escaping); tidak ada dangerouslySetInnerHTML (dijaga ESLint + tes).

import { createContext, useContext, type CSSProperties, type ReactNode } from "react";

import { T, useEditing } from "./edit";
import { PublicNav } from "./PublicNav";
import { fontStack, primaryVars, safeColor, safeHref, sizesFor, srcSet, text, waLink } from "./safe";
import type { Business, Json, OpenStatus, PageContent, Section } from "./types";
import { t as tx, m } from "@/i18n";
import { rupiah } from "@/lib/labels";
import { LanguageSwitch } from "@/i18n/react";

export type SiteTheme = {
  primary: string;
  font: string;
  navbar_style?: "top" | "side";
  navbar_sticky?: boolean;
  background_image?: string;
  background_size?: "cover" | "contain" | "repeat";
  background_position?: string;
  background_opacity?: number;
};

export type SiteContent = Omit<PageContent, "theme"> & {
  theme: PageContent["theme"] & Partial<SiteTheme>;
};

export interface SiteProps {
  content: SiteContent;
  business: Business;
  status?: OpenStatus | null;
  /** `asset:<uuid>` -> URL absolut gambar; rujukan tak dikenal tidak dirender. */
  assetUrl: (ref: string) => string | undefined;
  /** Preview dashboard: section nonaktif tetap tampil dengan penanda. */
  preview?: boolean;
  /** Produk aktif untuk section product_grid (situs publik). `undefined` = preview/placeholder. */
  products?: GridProduct[];
  /** Lebar slot desktop (px) untuk `sizes` gambar (Studio: lebar blok; v1: lebar halaman). */
  slotWidth?: number;
}

/** P9-7: lebar slot tempat section dirender, agar browser memilih varian gambar terkecil yang cukup. */
const SlotWidth = createContext(1180);
/** Porsi lebar gambar = aturan container query di site.css (split 2 kolom ≥ 768; grid 2/3/4 kolom). */
const splitShare = (slot: number) => (slot >= 768 ? 0.5 : 1);
const gridShare = (slot: number) => (slot >= 1024 ? 1 / 4 : slot >= 768 ? 1 / 3 : 1 / 2);

/** Bentuk minimal produk publik untuk grid (gambar sudah URL absolut). */
export interface GridProduct {
  slug: string;
  name: string;
  image?: string;
  price: string;
  promo_price: string | null;
  available: boolean;
}

/** Harga produk: format sama dengan dasbor (pemisah ribuan mengikuti bahasa). */
const rp = (v: string): string => rupiah(v);

const DAYS = ["", m("Senin"), m("Selasa"), m("Rabu"), m("Kamis"), m("Jumat"), m("Sabtu"), m("Minggu")];

type Data = Section["data"];

function obj(v: Json | undefined): { [k: string]: Json } | undefined {
  return v && typeof v === "object" && !Array.isArray(v) ? v : undefined;
}

function list(v: Json | undefined): { [k: string]: Json }[] {
  return Array.isArray(v) ? (v.map(obj).filter(Boolean) as { [k: string]: Json }[]) : [];
}

/** `priority`: gambar LCP (banner) dimuat segera; lainnya lazy. Semua didekode async (P9-7). */
export function Img({ r, alt, assetUrl, className, priority, share = () => 1, mobile = 1, fit, position, style }: {
  r: Json | undefined; alt: string; assetUrl: SiteProps["assetUrl"]; className?: string; priority?: boolean;
  /** Porsi lebar slot yang ditempati gambar di desktop, dari lebar slot (container query). */
  share?: (slot: number) => number; mobile?: number;
  fit?: "cover" | "contain"; position?: string;
  style?: CSSProperties;
}) {
  const slot = useContext(SlotWidth);
  const src = typeof r === "string" ? assetUrl(r) : undefined;
  const set = srcSet(src);
  return src ? (
    // eslint-disable-next-line @next/next/no-img-element -- static export, gambar dari API bertanda tangan
    <img className={className} src={src} srcSet={set} sizes={set ? sizesFor(slot * share(slot), mobile) : undefined} alt={alt}
      decoding="async" loading={priority ? "eager" : "lazy"} fetchPriority={priority ? "high" : undefined}
      style={{ objectFit: fit ?? "cover", objectPosition: position ?? "center", ...style }} />
  ) : null;
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
  return <Link className="s-btn" href={c.href}><T path="cta.label" v={c.label} /></Link>;
}

function Title({ v }: { v: Json | undefined }) {
  const t = text(v);
  const editing = useEditing();
  return t || editing ? <h2 className="s-title"><T path="title" v={v} placeholder={tx("Judul")} /></h2> : null;
}

function Paragraphs({ v, path = "body" }: { v: Json | undefined; path?: string }) {
  // Saat mengedit: satu isian multi-baris (paragraf dipisah baris kosong).
  if (useEditing()) return <p><T path={path} v={v} multiline /></p>;
  return (
    <>
      {text(v)
        .split("\n")
        .filter((p) => p.trim())
        .map((p, i) => <p key={i}>{p}</p>)}
    </>
  );
}

function Subtitle({ v }: { v: Json | undefined }) {
  const editing = useEditing();
  return text(v) || editing ? <p className="s-lead"><T path="subtitle" v={v} multiline placeholder={tx("Kalimat pendek tentang usaha Anda")} /></p> : null;
}

function Caption({ v, path }: { v: Json | undefined; path: string }) {
  const editing = useEditing();
  return text(v) || editing ? <figcaption><T path={path} v={v} placeholder={tx("Keterangan")} /></figcaption> : null;
}

function SectionBody({ s, props }: { s: Section; props: SiteProps }) {
  const d: Data = s.data;
  const { business: b, assetUrl, content } = props;
  const theme = content.theme as (PageContent["theme"] & SiteTheme) | undefined;
  const editing = useEditing();
  switch (s.type) {
    case "header": {
      // Renderer v1 (warisan): header selalu jadi bilah atas. Gaya `side` di v1 tidak pernah
      // berfungsi (markup lama tanpa CSS) — dokumen baru memakai renderer v2 (StudioSite).
      const isSticky = theme?.navbar_sticky === true;
      const links = list(d.nav)
        .map((n) => ({ href: safeHref(text(n.href)), label: text(n.label) }))
        .filter((l): l is { href: string; label: string } => !!l.href && !!l.label);
      const logoRef = typeof d.logo === "string" ? d.logo : b.profile.logo ?? undefined;
      return (
        <PublicNav
          style="top"
          sticky={isSticky}
          brand={text(d.title) || b.profile.name}
          logoUrl={logoRef ? assetUrl(logoRef) : undefined}
          links={links}
        />
      );
    }
    case "hero":
      return (
        <div className={`s-hero${hasImg(d.image ?? b.profile.cover ?? undefined, assetUrl) ? " has-media" : ""}`}>
          <div className="s-hero-text">
            <h1><T path="title" v={d.title} placeholder={tx("Judul utama")} /></h1>
            <Subtitle v={d.subtitle} />
            <Cta cta={d.cta} />
          </div>
          <Img
            r={d.image ?? b.profile.cover ?? undefined}
            alt={text(d.image_alt)}
            assetUrl={assetUrl}
            className="s-hero-img"
            priority
            share={splitShare}
            fit={d.image_fit === "contain" ? "contain" : d.image_fit === "cover" ? "cover" : undefined}
            position={typeof d.image_position === "string" ? d.image_position : undefined}
          />
        </div>
      );
    case "about":
      return (
        <div className={`s-split${hasImg(d.image, assetUrl) ? " has-media" : ""}`}>
          <div>
            <Title v={d.title} />
            <Paragraphs v={d.body} />
          </div>
          <Img
            r={d.image}
            alt={text(d.image_alt)}
            assetUrl={assetUrl}
            className="s-media"
            share={splitShare}
            fit={d.image_fit === "contain" ? "contain" : d.image_fit === "cover" ? "cover" : undefined}
            position={typeof d.image_position === "string" ? d.image_position : undefined}
          />
        </div>
      );
    case "product_grid": {
      const limit = typeof d.limit === "number" ? d.limit : 8;
      const slot = props.slotWidth ?? 1180;
      const items = (props.products ?? []).slice(0, limit);
      return (
        <div>
          <Title v={d.title} />
          {props.products === undefined && <p className="s-muted">{tx("Produk aktif toko tampil di sini.")}</p>}
          {props.products !== undefined && items.length === 0 && <p className="s-muted">{tx("Belum ada produk.")}</p>}
          <div className="s-products">
            {items.map((p) => (
              <a key={p.slug} className="s-product" href={`/produk/${p.slug}`}>
                {p.image ? (
                  // eslint-disable-next-line @next/next/no-img-element -- static export, gambar bertanda tangan
                  <img src={p.image} srcSet={srcSet(p.image)} sizes={srcSet(p.image) ? sizesFor(slot * gridShare(slot), 0.5) : undefined} alt="" loading="lazy" decoding="async" />
                ) : (
                  <span className="s-product-noimg" aria-hidden="true" />
                )}
                <span className="s-product-name">{p.name}</span>
                <span className="s-product-price">
                  {p.promo_price ? <><s>{rp(p.price)}</s> {rp(p.promo_price)}</> : rp(p.price)}
                </span>
                {!p.available && <span className="s-muted">{tx("Stok habis")}</span>}
              </a>
            ))}
          </div>
        </div>
      );
    }
    case "promotion":
      return (
        <div className={`s-promo${hasImg(d.image, assetUrl) ? " has-media" : ""}`}>
          <div>
            <h2 className="s-title"><T path="title" v={d.title} placeholder={tx("Judul promo")} /></h2>
            <Paragraphs v={d.body} />
            <Cta cta={d.cta} />
          </div>
          <Img
            r={d.image}
            alt={text(d.image_alt)}
            assetUrl={assetUrl}
            className="s-media"
            share={splitShare}
            fit={d.image_fit === "contain" ? "contain" : d.image_fit === "cover" ? "cover" : undefined}
            position={typeof d.image_position === "string" ? d.image_position : undefined}
          />
        </div>
      );
    case "gallery":
      return (
        <div>
          <Title v={d.title} />
          <div className="s-gallery">
            {list(d.images).map((g, i) => (
              <figure key={i}>
                <Img
                  r={g.asset}
                  alt={text(g.alt) || text(g.caption)}
                  assetUrl={assetUrl}
                  share={gridShare}
                  mobile={0.5}
                  fit={g.image_fit === "contain" ? "contain" : g.image_fit === "cover" ? "cover" : undefined}
                  position={typeof g.image_position === "string" ? g.image_position : undefined}
                />
                <Caption v={g.caption} path={`images.${i}.caption`} />
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
                <p>“<T path={`items.${i}.quote`} v={t.quote} multiline />”</p>
                <cite><T path={`items.${i}.name`} v={t.name} placeholder={tx("Nama")} /></cite>
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
              {props.status.status === "open" ? tx("Buka sekarang") : tx("Tutup sekarang")}
            </p>
          )}
          {b.hours.length === 0 ? (
            <p className="s-muted">{tx("Jam buka belum diatur.")}</p>
          ) : (
            <table className="s-hours">
              <tbody>
                {[1, 2, 3, 4, 5, 6, 7].map((day) => {
                  const h = b.hours.find((x) => x.day_of_week === day);
                  return (
                    <tr key={day}>
                      <th scope="row">{tx(DAYS[day] ?? "")}</th>
                      <td>{h ? tx("{0} – {1}", h.open, h.close) : tx("Tutup")}</td>
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
          {b.addresses.length === 0 && <p className="s-muted">{tx("Alamat belum diatur.")}</p>}
          <ul className="s-plain">
            {b.addresses.map((a, i) => (
              <li key={i}>
                {a.label && <strong>{a.label}: </strong>}
                {a.address}
                {d.show_map === true && a.latitude != null && a.longitude != null && (
                  <>
                    {" "}
                    <Link href={`https://www.openstreetmap.org/?mlat=${a.latitude}&mlon=${a.longitude}#map=17/${a.latitude}/${a.longitude}`}>
                      {tx("Lihat peta")}
                    </Link>
                  </>
                )}
              </li>
            ))}
          </ul>
          {(b.profile.pickup_available || b.profile.delivery_available) && (
            <p className="s-muted">
              {[b.profile.pickup_available && tx("Bisa ambil di tempat"), b.profile.delivery_available && tx("Bisa diantar")]
                .filter(Boolean)
                .join(" · ")}
              {b.profile.fulfillment_note ? tx(" — {0}", b.profile.fulfillment_note) : ""}
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
          {!any && <p className="s-muted">{tx("Kontak belum diatur.")}</p>}
          <div className="s-contact">
            {d.show_whatsapp === true && wa && <Link className="s-btn" href={wa}>{tx("WhatsApp")}</Link>}
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
          <span><T path="text" v={d.text} /></span>
          <span className="s-muted">
            © {b.profile.name} · <a href="/keranjang">{tx("Keranjang")}</a> · <a href="/pesanan">{tx("Pesanan saya")}</a> · <a href="/akun">{tx("Akun")}</a>
            {" · "}<a href="/kebijakan-privasi">{tx("Kebijakan privasi")}</a> · <a href="/syarat-transaksi">{tx("Syarat transaksi")}</a>
          </span>
          {!editing && <LanguageSwitch className="s-lang" />}
        </div>
      );
    default:
      return null;
  }
}

export function Site(props: SiteProps) {
  const { content, preview } = props;
  const theme = content.theme as (PageContent["theme"] & SiteTheme) | undefined;
  const bgUrl = theme?.background_image ? (props.assetUrl(theme.background_image) ?? theme.background_image) : undefined;
  const isSticky = theme?.navbar_sticky === true;
  const style = {
    ...primaryVars(content.theme?.primary),
    "--s-font": fontStack(content.theme?.font),
    ...(bgUrl ? { "--s-bg-img": `url("${bgUrl}")` } : {}),
    "--s-bg-size": theme?.background_size ?? "cover",
    "--s-bg-pos": theme?.background_position ?? "center",
    "--s-bg-opacity": ((theme?.background_opacity ?? 0) / 100).toString(),
  } as CSSProperties;
  const sections = (content.sections ?? []).filter((s) => preview || s.enabled);
  return (
    <SlotWidth.Provider value={props.slotWidth ?? 1180}>
    <div className="site" style={style} data-template={content.template}>
      {bgUrl && (
        <div
          className="s-bg-overlay"
          aria-hidden="true"
          style={{
            position: "fixed",
            inset: 0,
            pointerEvents: "none",
            backgroundImage: "var(--s-bg-img)",
            backgroundSize: "var(--s-bg-size, cover)",
            backgroundPosition: "var(--s-bg-pos, center)",
            backgroundRepeat: theme?.background_size === "repeat" ? "repeat" : "no-repeat",
            opacity: "var(--s-bg-opacity, 0)",
            backdropFilter: "blur(12px)",
            WebkitBackdropFilter: "blur(12px)",
            zIndex: 0,
          }}
        />
      )}
      {sections.map((s) => (
        <section
          key={s.id}
          id={s.id}
          className={`s s--${s.type}${s.enabled ? "" : " s-disabled"}${s.type === "header" && isSticky ? " s-sticky" : ""}`}
          data-section-type={s.type}
        >
          {preview && !s.enabled && <div className="s-disabled-badge">{tx("Nonaktif — tidak tampil di publik")}</div>}
          <div className="s-inner">
            <SectionBody s={s} props={props} />
          </div>
        </section>
      ))}
    </div>
    </SlotWidth.Provider>
  );
}
