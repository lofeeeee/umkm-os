"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { api, describe, setCsrf } from "@/lib/api";

export default function Register() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setBusy(true);
    setError(null);
    try {
      const r = await api<{ csrf_token: string }>("POST", "/api/v1/auth/register", {
        json: {
          email: f.get("email"),
          password: f.get("password"),
          business_name: f.get("business_name"),
          slug: (f.get("slug") as string) || null,
          accept_terms: f.get("terms") === "on",
          accept_privacy: f.get("privacy") === "on",
        },
      });
      setCsrf(r.csrf_token);
      router.push("/");
    } catch (err) {
      setError(describe(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="d-narrow">
      <h1>Daftar</h1>
      <form className="d-card" onSubmit={submit}>
        {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
        <label className="d-field"><span>Nama bisnis</span><input name="business_name" required maxLength={120} /></label>
        <label className="d-field"><span>Alamat toko (opsional)</span><input name="slug" pattern="[a-z0-9-]{3,63}" placeholder="tokosaya" /></label>
        <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required /></label>
        <label className="d-field"><span>Password</span><input name="password" type="password" autoComplete="new-password" required minLength={8} /></label>
        <label className="d-check"><input name="terms" type="checkbox" required /> Saya menyetujui syarat layanan</label>
        <label className="d-check"><input name="privacy" type="checkbox" required /> Saya menyetujui kebijakan privasi</label>
        <button className="d-btn d-btn-primary" disabled={busy}>{busy ? "Memproses…" : "Daftar"}</button>
      </form>
      <p>Sudah punya akun? <Link href="/login">Masuk</Link></p>
    </main>
  );
}
