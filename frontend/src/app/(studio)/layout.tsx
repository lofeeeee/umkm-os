import type { ReactNode } from "react";

import { BareAppShell } from "@/dashboard/shell";

// Halaman layar penuh (Studio): sesi & toko dari shell, tanpa sidebar/topbar dasbor.
export default function StudioLayout({ children }: { children: ReactNode }) {
  return <BareAppShell>{children}</BareAppShell>;
}
