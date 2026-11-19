// Navigasi situs publik (v1 & v2). Satu komponen untuk kedua gaya (atas/samping) dengan tautan
// halaman terbit — sebelumnya `s-nav-side` dirender tanpa CSS dan gaya "top" tidak dirender sama sekali,
// sehingga navigasi yang terlihat di Studio hilang setelah diterbitkan.

import { LogoMark } from "./LogoMark";
import type { ReactNode } from "react";
import { t } from "@/i18n";

export interface NavLink { href: string; label: string; /** Halaman yang sedang dibuka (aria-current). */ current?: boolean }

export interface PublicNavProps {
  style: "top" | "side";
  sticky?: boolean;
  /** Nama toko; dipakai bila tidak ada logo. */
  brand: string;
  logoUrl?: string;
  /** Plugin Logo ada di halaman: logo (dan nama bila plugin menampilkannya) tidak diulang di navigasi. */
  hideLogo?: boolean;
  hideName?: boolean;
  links: NavLink[];
  /** Tombol tambahan di ujung navigasi (halaman toko: keranjang, pesanan, akun). */
  actions?: ReactNode;
  /** Alt logo kosong = hiasan (nama toko sudah ada sebagai teks). */
  ariaLabel?: string;
}

function Brand({ brand, logoUrl, hideLogo, hideName }: { brand: string; logoUrl?: string; hideLogo?: boolean; hideName?: boolean }): ReactNode {
  return (
    <div className="s-brand">
      {!hideLogo && <LogoMark url={logoUrl} name={brand} className="s-logo" size={36} />}
      {hideName ? <span className="s-sr-only">{brand}</span> : <span className="s-brand-name">{brand}</span>}
    </div>
  );
}

function Links({ links }: { links: NavLink[] }) {
  if (links.length === 0) return null;
  return (
    <nav className="s-nav" aria-label={t("Halaman")}>
      {links.map((l) => <a key={l.href} href={l.href} aria-current={l.current ? "page" : undefined}>{l.label}</a>)}
    </nav>
  );
}

/** Navigasi situs publik. `side` = kolom kiri menetap (desktop) / bilah atas (ponsel). */
export function PublicNav({ style, sticky = false, brand, logoUrl, hideLogo, hideName, links, actions, ariaLabel }: PublicNavProps) {
  if (style === "side") {
    return (
      <aside className="s-nav-side" aria-label={ariaLabel ?? t("Navigasi samping")}>
        <Brand brand={brand} logoUrl={logoUrl} hideLogo={hideLogo} hideName={hideName} />
        <Links links={links} />
        {actions && <div className="s-nav-actions">{actions}</div>}
      </aside>
    );
  }
  return (
    <header className={`s-headbar${sticky ? " s-sticky" : ""}`} aria-label={ariaLabel ?? t("Navigasi atas")}>
      <Brand brand={brand} logoUrl={logoUrl} hideLogo={hideLogo} hideName={hideName} />
      <Links links={links} />
      {actions && <div className="s-nav-actions">{actions}</div>}
    </header>
  );
}
