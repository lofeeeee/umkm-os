// Footer situs toko (Studio v2). Dulu hanya satu baris tautan yang terpencar; sekarang footer menjadi
// "kartu nama" toko: identitas + status buka + kontak langsung, kolom tautan, dan baris hukum/bahasa.
// Semua isi berasal dari Profil bisnis & halaman terbit — pemilik tidak perlu mengisi apa pun lagi, dan
// kolom yang datanya kosong tidak dirender (tidak ada judul tanpa isi).

import { LogoMark } from "./LogoMark";
import type { ReactNode } from "react";

import { t as tx } from "@/i18n";
import { LanguageSwitch } from "@/i18n/react";

import type { NavLink } from "./PublicNav";
import { safeHref, text, waLink } from "./safe";
import type { Business, OpenStatus } from "./types";

export interface SiteFooterProps {
  business: Business;
  status?: OpenStatus | null;
  logoUrl?: string;
  /** Tautan halaman terbit (sama dengan navigasi). */
  pages: NavLink[];
  /** Kanvas Studio: tanpa pemilih bahasa (bukan bagian dokumen). */
  editing?: boolean;
  /** Lencana "Dibuat dengan UMKM OS"; hanya toko Pro yang bisa mematikannya (server membuang pilihan toko non-Pro). */
  badge?: boolean;
  platformUrl?: string | null;
}

const SOCIAL_LABEL: Record<string, string> = {
  instagram: "Instagram", facebook: "Facebook", tiktok: "TikTok", youtube: "YouTube", x: "X", website: "Website",
};

function Col({ title, children, wide }: { title: string; children: ReactNode; wide?: boolean }) {
  return (
    <div className={wide ? "sf-col sf-wide" : "sf-col"}>
      <p className="sf-col-title">{title}</p>
      {children}
    </div>
  );
}

/** Lencana platform: kecil & netral (tidak bersaing dengan merek toko), tautan `nofollow` ke beranda platform. */
function MadeWith({ href }: { href?: string }) {
  const body = (
    <>
      <span className="sf-made-mark" aria-hidden="true">U</span>
      <span>{tx("Dibuat dengan")} <strong>UMKM OS</strong></span>
    </>
  );
  return (
    <div className="sf-made">
      {href
        ? <a href={href} target="_blank" rel="noopener nofollow" data-testid="made-with">{body}</a>
        : <span data-testid="made-with">{body}</span>}
    </div>
  );
}

export function SiteFooter({ business, status, logoUrl, pages, editing, badge = true, platformUrl }: SiteFooterProps) {
  const p = business.profile;
  const name = p.name;
  const wa = waLink(p.whatsapp);
  const address = business.addresses[0]?.address?.trim();
  const mapHref = address ? `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(address)}` : undefined;
  const socials = p.socials
    .map((s) => ({ label: SOCIAL_LABEL[s.platform] ?? text(s.platform), href: safeHref(s.url) }))
    .filter((s): s is { label: string; href: string } => Boolean(s.href));
  const description = p.description?.trim();
  const hasContact = Boolean(address || p.phone || p.email || socials.length);
  const year = new Date().getFullYear();

  return (
    <footer className="sv2-foot">
      <div className="sf-inner">
        <div className="sf-top">
          <div className="sf-brand">
            <div className="sf-id">
              <LogoMark url={logoUrl} name={name} className="sf-logo" size={44} lazy />
              <span className="sf-name">{name}</span>
            </div>
            {description && <p className="sf-desc">{description}</p>}
            <div className="sf-actions">
              {status && business.hours.length > 0 && (
                <span className={`sf-status sf-status-${status.status}`}>
                  {status.status === "open" ? tx("Buka sekarang") : tx("Tutup sekarang")}
                </span>
              )}
              {wa && (
                <a className="sf-wa" href={wa} target="_blank" rel="noopener">
                  {tx("Chat via WhatsApp")}
                </a>
              )}
            </div>
          </div>

          {pages.length > 0 && (
            <Col title={tx("Halaman")}>
              <nav aria-label={tx("Halaman")} className="sf-links">
                {pages.map((l) => <a key={l.href} href={l.href}>{l.label}</a>)}
              </nav>
            </Col>
          )}

          {/* UAT 15 Nov customer #5: keranjang/pesanan/akun terjangkau dari setiap halaman. */}
          <Col title={tx("Belanja")}>
            <nav aria-label={tx("Belanja")} className="sf-links">
              <a href="/keranjang">{tx("Keranjang")}</a>
              <a href="/pesanan">{tx("Pesanan saya")}</a>
              <a href="/akun">{tx("Akun")}</a>
            </nav>
          </Col>

          {hasContact && (
            <Col title={tx("Kunjungi & hubungi")} wide>
              <div className="sf-links">
                {address && <a href={mapHref} target="_blank" rel="noopener" className="sf-addr">{address}</a>}
                {p.phone && <a href={`tel:${p.phone}`}>{p.phone}</a>}
                {p.email && <a href={`mailto:${p.email}`}>{p.email}</a>}
              </div>
              {socials.length > 0 && (
                <div className="sf-social">
                  {socials.map((s) => <a key={s.href} href={s.href} target="_blank" rel="noopener">{s.label}</a>)}
                </div>
              )}
            </Col>
          )}
        </div>

        {/* P9-9: kebijakan privasi & syarat selalu bisa dijangkau dari situs (chat & akun mengumpulkan data). */}
        <div className="sf-bottom">
          <span>© {year} {name}</span>
          <nav aria-label={tx("Informasi toko")} className="sf-legal">
            <a href="/kebijakan-privasi">{tx("Kebijakan privasi")}</a>
            <a href="/syarat-transaksi">{tx("Syarat transaksi")}</a>
          </nav>
          {!editing && <LanguageSwitch className="s-lang" />}
        </div>
        {badge && <MadeWith href={editing ? undefined : platformUrl ?? undefined} />}
      </div>
    </footer>
  );
}
