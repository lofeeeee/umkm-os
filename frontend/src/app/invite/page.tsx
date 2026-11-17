"use client";

// Terima undangan tim (P4-1, AUTH-009/010). Token di fragment URL (tidak terkirim ke server/log).

import Link from "@/ui/link";
import { useRouter } from "next/navigation";
import { useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { api, describe, loadSession, setCsrf } from "@/lib/api";
import { AuthShell, PasswordInput } from "@/ui/auth";
import { BusyButton, PageSkeleton } from "@/ui/page";
import { t as tx } from "@/i18n";

function token(): string | null {
  const m = /(?:^|&)token=([^&]+)/.exec(window.location.hash.replace(/^#/, ""));
  return m ? decodeURIComponent(m[1]!) : null;
}
const noop = () => () => {};

export default function InvitePage() {
  const router = useRouter();
  const t = useSyncExternalStore(noop, token, () => undefined);
  const [password, setPassword] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function accept(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      const session = await loadSession().catch(() => null);
      const r = await api<{ tenant_id: string; session: { csrf_token: string } | null }>("POST", "/api/v1/invitations/accept", {
        json: { token: t, password: session ? null : password },
      });
      if (r.session) setCsrf(r.session.csrf_token);
      router.replace(`/?tenant=${r.tenant_id}`);
    } catch (e2) {
      setErr(describe(e2));
    } finally {
      setBusy(false);
    }
  }
  if (t === undefined) return <AuthShell pitch={tx("Bergabung dengan tim toko")}><PageSkeleton /></AuthShell>;
  return (
    <AuthShell pitch={tx("Bergabung dengan tim toko")}>
      <h1>{tx("Terima undangan")}</h1>
      {!t && (
        <>
          <Alert kind="error">
            {tx("Tautan undangan tidak valid atau sudah kedaluwarsa. Undangan hanya berlaku 7 hari dan sekali pakai.")}
          </Alert>
          <p className="d-muted">{tx("Minta pemilik toko mengirim ulang undangan, lalu buka tautan terbaru dari email.")}</p>
          <Link className="d-btn" href="/login">{tx("Ke halaman masuk")}</Link>
        </>
      )}
      {t && (
        <form className="d-card" onSubmit={accept}>
          <p className="d-muted">{tx("Sudah punya akun?")}{" "}<Link href="/login">{tx("Masuk dulu")}</Link>{tx(", lalu buka tautan ini lagi. Belum punya akun? Buat password di bawah.")}</p>
          <PasswordInput name="password" label={tx("Password baru (bila belum punya akun)")} autoComplete="new-password" minLength={8} value={password} onChange={setPassword} />
          <Alert kind="error">{err}</Alert>
          <BusyButton busy={busy} className="d-btn d-btn-primary d-btn-block d-btn-lg">{tx("Terima undangan")}</BusyButton>
        </form>
      )}
    </AuthShell>
  );
}
