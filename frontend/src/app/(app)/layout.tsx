import type { ReactNode } from "react";

import { AppShell } from "@/dashboard/shell";

// Shell persisten: sesi & toko aktif dimuat sekali, sidebar tidak dirender ulang per halaman.
export default function AppLayout({ children }: { children: ReactNode }) {
  return <AppShell>{children}</AppShell>;
}
