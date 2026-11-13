import type { Metadata } from "next";

export const metadata: Metadata = { title: "Studio website (pratinjau)" };

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
