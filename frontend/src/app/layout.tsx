import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

import "../renderer/site.css";
import "./globals.css";

import { ConfirmHost } from "@/ui/dialog";
import { Toaster } from "@/ui/toast";
import { ValidityLocalizer } from "@/ui/validity";

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
        {/* Skrip pemblokir di <head> sengaja: harus jalan sebelum paint. (next/script beforeInteractive
            dimuat lewat runtime → tema bisa berkedip.) Peringatan dev React soal <script> boleh diabaikan. */}
        <script src="/theme-boot.js" async={false} />
      </head>
      <body>
        {children}
        <Toaster />
        <ConfirmHost />
        <ValidityLocalizer />
      </body>
    </html>
  );
}
