"use client";

import Link from "@/ui/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { api, describe, setCsrf, type MfaChallenge, type Session } from "@/lib/api";
import { AuthShell, PasswordInput } from "@/ui/auth";
import { BusyButton } from "@/ui/page";

export default function Login() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Langkah kedua (AUTH-011): token tantangan hanya di memori, tidak pernah di storage.
  const [mfaToken, setMfaToken] = useState<string | null>(null);

  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (err) {
      setError(describe(err));
      // Tombol nonaktif saat memproses → fokus jatuh ke BODY. Pindahkan ke pesan galat (UAT 15 Nov).
      requestAnimationFrame(() => document.querySelector<HTMLElement>(".d-auth-form [role=alert]")?.focus());
    } finally {
      setBusy(false);
    }
  }

  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    return run(async () => {
      const r = await api<Session | MfaChallenge>("POST", "/api/v1/auth/login", {
        json: { email: f.get("email"), password: f.get("password") },
      });
      if ("mfa_required" in r) {
        setMfaToken(r.mfa_token);
        return;
      }
      setCsrf(r.csrf_token);
      router.push("/");
    });
  }

  function submitCode(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    return run(async () => {
      try {
        const s = await api<Session>("POST", "/api/v1/auth/login/mfa", {
          json: { mfa_token: mfaToken, code: f.get("code") },
        });
        setCsrf(s.csrf_token);
        router.push("/");
      } catch (err) {
        // 401 = tantangan habis (kedaluwarsa / 5× salah): kembali ke password.
        if ((err as { status?: number }).status === 401) setMfaToken(null);
        throw err;
      }
    });
  }

  return (
    <AuthShell pitch="Satu tempat untuk semua operasional usaha Anda">
      {mfaToken ? (
        <>
          <h1>Verifikasi dua langkah</h1>
          <p className="d-muted">Masukkan 6 digit dari aplikasi authenticator, atau salah satu kode pemulihan (format xxxxx-xxxxx).</p>
          <form className="d-card" onSubmit={submitCode}>
            <Alert kind="error">{error}</Alert>
            <label className="d-field"><span>Kode verifikasi</span>
              <input name="code" className="d-otp" autoComplete="one-time-code" inputMode="text" maxLength={16} required autoFocus placeholder="123456" />
            </label>
            <BusyButton busy={busy} className="d-btn d-btn-primary d-btn-block d-btn-lg">Verifikasi</BusyButton>
            <button type="button" className="d-btn d-btn-ghost d-btn-block" style={{ marginTop: 8 }} onClick={() => { setMfaToken(null); setError(null); }}>Batal</button>
          </form>
        </>
      ) : (
        <>
          <h1>Masuk</h1>
          <p className="d-muted">Selamat datang kembali. Masuk untuk mengelola toko Anda.</p>
          <form className="d-card" onSubmit={submit}>
            <Alert kind="error">{error}</Alert>
            <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required autoFocus placeholder="nama@contoh.id" /></label>
            <PasswordInput name="password" autoComplete="current-password" />
            <BusyButton busy={busy} className="d-btn d-btn-primary d-btn-block d-btn-lg">Masuk</BusyButton>
          </form>
          <p>Belum punya akun? <Link href="/register">Daftarkan usaha Anda</Link></p>
        </>
      )}
    </AuthShell>
  );
}
