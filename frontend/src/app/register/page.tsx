"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { CheckCircle, Globe, Storefront, Wallet } from "@phosphor-icons/react/ssr";

import { api, describe, setCsrf } from "@/lib/api";

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
        <h2>Mulai digitalisasi usaha Anda hari ini</h2>
        <p>Daftar, dapatkan website, dan kelola pesanan pertama dalam hitungan menit.</p>
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
    <div className="d-auth">
      <AuthBrand />
      <div className="d-auth-form">
        <main className="d-narrow">
          <h1>Daftar</h1>
          <p className="d-muted">Buat akun dan toko digital pertama Anda.</p>
          <form className="d-card" onSubmit={submit}>
            {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
            <label className="d-field"><span>Nama bisnis</span><input name="business_name" required maxLength={120} autoFocus /></label>
            <label className="d-field"><span>Alamat toko (opsional)</span><input name="slug" pattern="[a-z0-9-]{3,63}" placeholder="tokosaya" /></label>
            <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required /></label>
            <label className="d-field"><span>Password</span><input name="password" type="password" autoComplete="new-password" required minLength={8} /></label>
            <label className="d-check"><input name="terms" type="checkbox" required /> Saya menyetujui <a href="/legal/syarat" target="_blank" rel="noopener">syarat & ketentuan</a></label>
            <label className="d-check"><input name="privacy" type="checkbox" required /> Saya menyetujui <a href="/legal/privasi" target="_blank" rel="noopener">kebijakan privasi</a></label>
            <button className="d-btn d-btn-primary" disabled={busy} style={{ width: "100%" }}>{busy ? "Memproses…" : "Daftar"}</button>
          </form>
          <p>Sudah punya akun? <Link href="/login">Masuk</Link></p>
        </main>
      </div>
    </div>
  );
}
