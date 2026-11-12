"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { api, describe, setCsrf, type Session } from "@/lib/api";

export default function Login() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setBusy(true);
    setError(null);
    try {
      const s = await api<Session>("POST", "/api/v1/auth/login", {
        json: { email: f.get("email"), password: f.get("password") },
      });
      setCsrf(s.csrf_token);
      router.push("/");
    } catch (err) {
      setError(describe(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="d-narrow">
      <h1>Masuk</h1>
      <form className="d-card" onSubmit={submit}>
        {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
        <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required /></label>
        <label className="d-field"><span>Password</span><input name="password" type="password" autoComplete="current-password" required /></label>
        <button className="d-btn d-btn-primary" disabled={busy}>{busy ? "Memproses…" : "Masuk"}</button>
      </form>
      <p>Belum punya akun? <Link href="/register">Daftar</Link></p>
    </main>
  );
}
