import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

import "../renderer/site.css";
import "./globals.css";

export const metadata: Metadata = {
  title: "UMKM OS",
  description: "Website dan operasional digital untuk UMKM",
};

export const viewport: Viewport = { width: "device-width", initialScale: 1 };

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="id">
      <body>{children}</body>
    </html>
  );
}
