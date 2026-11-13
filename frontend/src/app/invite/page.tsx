"use client";

// Terima undangan tim (P4-1, AUTH-009/010). Token di fragment URL (tidak terkirim ke server/log).

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { api, describe, loadSession, setCsrf } from "@/lib/api";

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
  if (t === undefined) return <main id="main" className="d-narrow d-muted" aria-busy="true">Memuat…</main>;
  return (
    <main id="main" className="d-narrow">
      <h1>Terima undangan</h1>
      {!t && (
        <>
          <Alert kind="error">
            Tautan undangan tidak valid atau sudah kedaluwarsa. Undangan hanya berlaku 7 hari dan sekali pakai.
          </Alert>
          <p className="d-muted">Minta pemilik toko mengirim ulang undangan, lalu buka tautan terbaru dari email.</p>
          <p><Link className="d-btn" href="/">Kembali ke beranda</Link></p>
        </>
      )}
      {t && (
        <form onSubmit={accept}>
          <p className="d-muted">Sudah punya akun? Masuk dulu, lalu buka tautan ini lagi. Belum punya akun? Buat password di bawah.</p>
          <label className="d-field"><span>Password baru (bila belum punya akun)</span>
            <input type="password" minLength={8} value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="new-password" />
          </label>
          <Alert kind="error">{err}</Alert>
          <button className="d-btn d-btn-primary" disabled={busy}>Terima undangan</button>
        </form>
      )}
    </main>
  );
}
