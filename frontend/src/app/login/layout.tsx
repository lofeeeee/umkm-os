import type { Metadata } from "next";
import type { ReactNode } from "react";

import { TITLES } from "@/i18n/titles";

export const metadata: Metadata = { title: TITLES.login };

export default function Layout({ children }: { children: ReactNode }) {
  return children;
}
