// Navigasi situs publik (v1 & v2). Satu komponen untuk kedua gaya (atas/samping) dengan tautan
// halaman terbit — sebelumnya `s-nav-side` dirender tanpa CSS dan gaya "top" tidak dirender sama sekali,
// sehingga navigasi yang terlihat di Studio hilang setelah diterbitkan.

import { LogoMark } from "./LogoMark";
import type { ReactNode } from "react";
import { t } from "@/i18n";

export interface NavLink { href: string; label: string }

export interface PublicNavProps {
  style: "top" | "side";
  sticky?: boolean;
  /** Nama toko; dipakai bila tidak ada logo. */
  brand: string;
  logoUrl?: string;
  links: NavLink[];
  /** Alt logo kosong = hiasan (nama toko sudah ada sebagai teks). */
  ariaLabel?: string;
}

function Brand({ brand, logoUrl }: { brand: string; logoUrl?: string }): ReactNode {
  return (
    <div className="s-brand">
      <LogoMark url={logoUrl} name={brand} className="s-logo" size={36} />
      <span className="s-brand-name">{brand}</span>
    </div>
  );
}

function Links({ links }: { links: NavLink[] }) {
  if (links.length === 0) return null;
  return (
    <nav className="s-nav" aria-label={t("Halaman")}>
      {links.map((l) => <a key={l.href} href={l.href}>{l.label}</a>)}
    </nav>
  );
}

/** Navigasi situs publik. `side` = kolom kiri menetap (desktop) / bilah atas (ponsel). */
export function PublicNav({ style, sticky = false, brand, logoUrl, links, ariaLabel }: PublicNavProps) {
  if (style === "side") {
    return (
      <aside className="s-nav-side" aria-label={ariaLabel ?? t("Navigasi samping")}>
        <Brand brand={brand} logoUrl={logoUrl} />
        <Links links={links} />
      </aside>
    );
  }
  return (
    <header className={`s-headbar${sticky ? " s-sticky" : ""}`} aria-label={ariaLabel ?? t("Navigasi atas")}>
      <Brand brand={brand} logoUrl={logoUrl} />
      <Links links={links} />
    </header>
  );
}
