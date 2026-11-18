// Bentuk data dari API (apps/api/src/website.rs, business.rs). content_json mengikuti ADR-0007.

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

export type SectionType =
  | "header" | "hero" | "about" | "product_grid" | "promotion" | "gallery"
  | "testimonials" | "business_hours" | "location" | "contact" | "footer";

export const SECTION_TYPES: SectionType[] = [
  "header", "hero", "about", "product_grid", "promotion", "gallery",
  "testimonials", "business_hours", "location", "contact", "footer",
];

export interface Section {
  id: string;
  type: SectionType;
  enabled: boolean;
  data: { [k: string]: Json };
}

export interface PageContent {
  schema_version: 1;
  template: string;
  theme: { primary: string; font: string };
  sections: Section[];
}

export interface Social { platform: string; url: string }

export interface BusinessProfile {
  name: string;
  description?: string | null;
  logo?: string | null;
  cover?: string | null;
  phone?: string | null;
  email?: string | null;
  whatsapp?: string | null;
  socials: Social[];
  timezone: string;
  pickup_available: boolean;
  delivery_available: boolean;
  fulfillment_note?: string | null;
}

export interface Address { label?: string | null; address: string; latitude?: number | null; longitude?: number | null }
/**
 * Alamat saat diedit di dasbor: koordinat boleh berupa teks sementara (mis. "-" atau "-6.")
 * selagi pengguna mengetik, lalu angka setelah selesai. Backend hanya menerima angka.
 */
export interface EditableAddress extends Omit<Address, "latitude" | "longitude"> {
  latitude?: number | string | null;
  longitude?: number | string | null;
}
export interface Hours { day_of_week: number; open: string; close: string }
export interface HourException { date: string; is_closed: boolean; open?: string | null; close?: string | null; note?: string | null }

export interface Business {
  profile: BusinessProfile;
  addresses: Address[];
  hours: Hours[];
  exceptions: HourException[];
}

export interface OpenStatus { status: "open" | "closed"; timezone: string; local_time: string }

export interface PublicSite {
  page: { slug: string; version: number; content: PageContent };
  business: Business;
  status: OpenStatus;
  assets: Record<string, string>;
  /** Halaman terbit untuk navigasi. Lama/absent = nav hanya nama toko. */
  pages?: { slug: string; title: string; article?: import("./article").ArticleMeta }[];
  /** Beranda platform untuk lencana footer (server lama: absen → lencana tanpa tautan). */
  platform_url?: string | null;
}
