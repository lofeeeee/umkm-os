import Link from "@/ui/link";

import { TERMS } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";
import { t } from "@/i18n";
import { TITLES } from "@/i18n/titles";

export const metadata = { title: TITLES.terms };

export default function TermsPage() {
  return (
    <main className="d-narrow d-legal">
      <h1>{t("Syarat & Ketentuan")}</h1>
      <LegalDoc sections={TERMS} />
      <p><Link href="/legal/privasi">{t("Kebijakan Privasi")}</Link> · <Link href="/register">{t("Kembali ke pendaftaran")}</Link></p>
    </main>
  );
}
