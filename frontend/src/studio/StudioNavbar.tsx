"use client";

import { useMemo } from "react";
import { House, Info, Phone, Compass, List, PushPin } from "@phosphor-icons/react/ssr";
import type { Business } from "@/renderer/types";
import type { PageItem } from "./PagesPanel";

export interface StudioNavbarProps {
  style: "top" | "side";
  sticky?: boolean;
  business: Business;
  pages: PageItem[];
  activeSlug: string;
  onSelectPage: (slug: string) => void;
  logoUrl?: string;
  primaryColor?: string;
}

export function StudioNavbar({
  style,
  sticky = false,
  business,
  pages,
  activeSlug,
  onSelectPage,
  logoUrl,
  primaryColor = "#4F46E5",
}: StudioNavbarProps) {
  const pageList = useMemo(() => {
    if (pages && pages.length > 0) return pages;
    return [{ slug: "home", title: "Beranda" }];
  }, [pages]);

  const navLinks = pageList.map((p) => ({
    slug: p.slug,
    title: p.title,
    href: p.slug === "home" ? "/" : `/${p.slug}`,
  }));

  const brandName = business.profile.name || "Toko Saya";

  if (style === "side") {
    return (
      <aside
        className="st-navbar-side"
        style={
          {
            "--nav-primary": primaryColor,
          } as React.CSSProperties
        }
        aria-label="Navigasi Samping Situs"
      >
        <div className="st-nav-brand">
          {logoUrl ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={logoUrl} alt={brandName} className="st-nav-logo" />
          ) : (
            <span className="st-nav-logo-fallback" style={{ backgroundColor: primaryColor }}>
              {(brandName[0] || "T").toUpperCase()}
            </span>
          )}
          <div className="st-nav-title-group">
            <strong className="st-nav-brand-name">{brandName}</strong>
            <span className="st-nav-badge">Bilah Samping</span>
          </div>
        </div>

        <nav className="st-nav-menu" aria-label="Menu Halaman">
          <span className="st-nav-menu-label">Halaman</span>
          {navLinks.map((item) => {
            const active = activeSlug === item.slug;
            return (
              <button
                key={item.slug}
                type="button"
                className={`st-nav-item${active ? " active" : ""}`}
                onClick={() => onSelectPage(item.slug)}
                aria-current={active ? "page" : undefined}
              >
                {item.slug === "home" ? (
                  <House size={16} weight={active ? "fill" : "bold"} />
                ) : item.slug === "about" ? (
                  <Info size={16} weight={active ? "fill" : "bold"} />
                ) : item.slug === "contact" ? (
                  <Phone size={16} weight={active ? "fill" : "bold"} />
                ) : (
                  <Compass size={16} weight={active ? "fill" : "bold"} />
                )}
                <span>{item.title}</span>
                {active && <span className="st-nav-dot" />}
              </button>
            );
          })}
        </nav>

        {business.profile.whatsapp && (
          <div className="st-nav-footer">
            <span className="st-nav-hint">Hubungi:</span>
            <span className="st-nav-contact">{business.profile.whatsapp}</span>
          </div>
        )}
      </aside>
    );
  }

  // Top Navbar
  return (
    <header
      className={`st-navbar-top${sticky ? " st-navbar-sticky" : ""}`}
      style={
        {
          "--nav-primary": primaryColor,
        } as React.CSSProperties
      }
      aria-label="Navigasi Atas Situs"
    >
      <div className="st-nav-brand">
        {logoUrl ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={logoUrl} alt={brandName} className="st-nav-logo" />
        ) : (
          <span className="st-nav-logo-fallback" style={{ backgroundColor: primaryColor }}>
            {(brandName[0] || "T").toUpperCase()}
          </span>
        )}
        <strong className="st-nav-brand-name">{brandName}</strong>
      </div>

      <nav className="st-nav-links" aria-label="Tautan Halaman">
        {navLinks.map((item) => {
          const active = activeSlug === item.slug;
          return (
            <button
              key={item.slug}
              type="button"
              className={`st-nav-link-btn${active ? " active" : ""}`}
              onClick={() => onSelectPage(item.slug)}
              aria-current={active ? "page" : undefined}
            >
              <span>{item.title}</span>
              {active && <span className="st-nav-underline" style={{ backgroundColor: primaryColor }} />}
            </button>
          );
        })}
      </nav>

      {sticky && (
        <span className="st-sticky-pill" title="Navigasi menempel saat halaman digulir">
          <PushPin size={12} weight="fill" /> Menempel
        </span>
      )}
    </header>
  );
}
