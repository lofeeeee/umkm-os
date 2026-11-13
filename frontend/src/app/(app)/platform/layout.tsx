import type { Metadata } from "next";

export const metadata: Metadata = { title: "Admin platform" };

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
