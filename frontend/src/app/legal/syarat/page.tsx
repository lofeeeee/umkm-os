import Link from "next/link";

import { TERMS } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";

export const metadata = { title: "Syarat & Ketentuan — UMKM Business OS" };

export default function TermsPage() {
  return (
    <main className="d-narrow d-legal">
      <h1>Syarat & Ketentuan</h1>
      <LegalDoc sections={TERMS} />
      <p><Link href="/legal/privasi">Kebijakan Privasi</Link> · <Link href="/register">Kembali ke pendaftaran</Link></p>
    </main>
  );
}
