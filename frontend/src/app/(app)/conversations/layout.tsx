import type { Metadata } from "next";

import { TITLES } from "@/i18n/titles";

export const metadata: Metadata = { title: TITLES.conversations };

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
