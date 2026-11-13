import type { Metadata, Viewport } from "next";
import Script from "next/script";
import type { ReactNode } from "react";

import "../renderer/site.css";
import "./globals.css";

import { ConfirmHost } from "@/ui/dialog";
import { Toaster } from "@/ui/toast";

export const metadata: Metadata = {
  // R3: %s diisi tiap halaman supaya tab & riwayat browser bisa dibedakan.
  title: { default: "UMKM OS", template: "%s · UMKM OS" },
  description: "Website dan operasional digital untuk UMKM",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#ffffff" },
    { media: "(prefers-color-scheme: dark)", color: "#09090b" },
  ],
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="id" suppressHydrationWarning>
      <head>
        {/* Tema sebelum paint (anti-FOUC); file statis 'self' — aman CSP (ADR-0014) dan bebas inline script. */}
        <Script src="/theme-boot.js" strategy="beforeInteractive" />
      </head>
      <body>
        {children}
        <Toaster />
        <ConfirmHost />
      </body>
    </html>
  );
}
