import Link from "@/ui/link";

import { TERMS } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";
import { t } from "@/i18n";

export const metadata = { title: "Syarat & Ketentuan — UMKM Business OS" };

export default function TermsPage() {
  return (
    <main className="d-narrow d-legal">
      <h1>{t("Syarat & Ketentuan")}</h1>
      <LegalDoc sections={TERMS} />
      <p><Link href="/legal/privasi">{t("Kebijakan Privasi")}</Link> · <Link href="/register">{t("Kembali ke pendaftaran")}</Link></p>
    </main>
  );
}
