import Link from "@/ui/link";

import { PRIVACY } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";
import { t } from "@/i18n";

export const metadata = { title: "Kebijakan Privasi — UMKM Business OS" };

export default function PrivacyPage() {
  return (
    <main className="d-narrow d-legal">
      <h1>{t("Kebijakan Privasi")}</h1>
      <LegalDoc sections={PRIVACY} />
      <p><Link href="/legal/syarat">{t("Syarat & Ketentuan")}</Link> · <Link href="/register">{t("Kembali ke pendaftaran")}</Link></p>
    </main>
  );
}
