"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { CheckCircle, Globe, Storefront, Wallet } from "@phosphor-icons/react/ssr";

import { api, describe, setCsrf, type MfaChallenge, type Session } from "@/lib/api";

function AuthBrand() {
  return (
    <div className="d-auth-brand" aria-hidden="true">
      <div className="d-auth-logo">
        <span>
          <Storefront size={20} weight="bold" />
        </span>
        UMKM OS
      </div>
      <div className="d-auth-pitch">
        <h2>Satu tempat untuk semua operasional usaha Anda</h2>
        <p>Website, katalog, pesanan, pelanggan, dan asisten AI — dikelola dalam satu dashboard yang cepat dan aman.</p>
      </div>
      <ul className="d-auth-feats">
        <li>
          <Globe aria-hidden="true" weight="bold" />
          <div>
            <strong>Website siap pakai</strong>
            <span>Editor visual dengan pratinjau ponsel langsung.</span>
          </div>
        </li>
        <li>
          <Wallet aria-hidden="true" weight="bold" />
          <div>
            <strong>Pesanan & pembayaran QRIS</strong>
            <span>Verifikasi tercatat, approval berlapis untuk keamanan.</span>
          </div>
        </li>
        <li>
          <CheckCircle aria-hidden="true" weight="bold" />
          <div>
            <strong>Asisten AI percakapan</strong>
            <span>Jawab pelanggan otomatis, serahkan ke staf bila perlu.</span>
          </div>
        </li>
      </ul>
      <p className="d-auth-foot">© {new Date().getFullYear()} UMKM OS — sistem operasi usaha mikro, kecil, dan menengah.</p>
    </div>
  );
}

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
    <div className="d-auth">
      <AuthBrand />
      <div className="d-auth-form">
        {mfaToken ? (
          <main className="d-narrow">
            <h1>Verifikasi dua langkah</h1>
            <form className="d-card" onSubmit={submitCode}>
              {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
              <p className="d-muted">
                Masukkan 6 digit dari aplikasi authenticator, atau salah satu kode pemulihan (format xxxxx-xxxxx).
              </p>
              <label className="d-field"><span>Kode verifikasi</span>
                <input name="code" autoComplete="one-time-code" inputMode="text" maxLength={16} required autoFocus />
              </label>
              <div className="d-row">
                <button className="d-btn d-btn-primary" disabled={busy}>{busy ? "Memproses…" : "Verifikasi"}</button>
                <button type="button" className="d-btn" onClick={() => { setMfaToken(null); setError(null); }}>Batal</button>
              </div>
            </form>
          </main>
        ) : (
          <main className="d-narrow">
            <h1>Masuk</h1>
            <p className="d-muted">Selamat datang kembali. Masuk untuk mengelola toko Anda.</p>
            <form className="d-card" onSubmit={submit}>
              {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
              <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required autoFocus /></label>
              <label className="d-field"><span>Password</span><input name="password" type="password" autoComplete="current-password" required /></label>
              <button className="d-btn d-btn-primary" disabled={busy} style={{ width: "100%" }}>{busy ? "Memproses…" : "Masuk"}</button>
            </form>
            <p>Belum punya akun? <Link href="/register">Daftar</Link></p>
          </main>
        )}
      </div>
    </div>
  );
}
