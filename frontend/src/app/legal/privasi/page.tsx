import Link from "@/ui/link";

import { PRIVACY } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";

export const metadata = { title: "Kebijakan Privasi — UMKM Business OS" };

export default function PrivacyPage() {
  return (
    <main className="d-narrow d-legal">
      <h1>Kebijakan Privasi</h1>
      <LegalDoc sections={PRIVACY} />
      <p><Link href="/legal/syarat">Syarat & Ketentuan</Link> · <Link href="/register">Kembali ke pendaftaran</Link></p>
    </main>
  );
}
