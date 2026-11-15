"use client";

import Link from "@/ui/link";
import { useRouter } from "next/navigation";
import { useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { api, describe, setCsrf } from "@/lib/api";
import { AuthShell, PasswordInput } from "@/ui/auth";
import { BusyButton } from "@/ui/page";

/** "Kopi Senja Bandung" → "kopi-senja-bandung" (3–63 karakter a-z0-9-). */
function slugify(s: string): string {
  return s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 63);
}
/** Domain toko = host dasbor tanpa label pertama (app.contoh.id → contoh.id). */
const rootDomain = () => window.location.hostname.split(".").slice(1).join(".") || window.location.hostname;
const noop = () => () => {};

export default function Register() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [slugTouched, setSlugTouched] = useState(false);
  const [password, setPassword] = useState("");
  const domain = useSyncExternalStore(noop, rootDomain, () => "");
  const effectiveSlug = slugTouched ? slug : slugify(name);

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setBusy(true);
    setError(null);
    try {
      const r = await api<{ csrf_token: string }>("POST", "/api/v1/auth/register", {
        json: {
          email: f.get("email"),
          password,
          business_name: name,
          slug: effectiveSlug.length >= 3 ? effectiveSlug : null,
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
  const strength = password.length === 0 ? null : password.length < 8 ? "Minimal 8 karakter" : password.length < 12 ? "Cukup — lebih panjang lebih aman" : "Kuat";
  return (
    <AuthShell pitch="Mulai digitalisasi usaha Anda hari ini">
      <h1>Daftarkan usaha</h1>
      <p className="d-muted">Gratis 14 hari, tanpa kartu kredit. Website toko langsung jadi.</p>
      <form className="d-card" onSubmit={submit}>
        <Alert kind="error">{error}</Alert>
        <label className="d-field"><span>Nama bisnis</span>
          <input name="business_name" required maxLength={120} autoFocus placeholder="mis. Kopi Senja" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="d-field"><span>Alamat toko</span>
          <span className="d-input-suffix">
            <input name="slug" pattern="[a-z0-9\-]{3,63}" placeholder="tokosaya" value={effectiveSlug}
              onChange={(e) => { setSlugTouched(true); setSlug(e.target.value.toLowerCase()); }} style={{ paddingRight: domain ? `${domain.length * 7.5 + 24}px` : undefined }} />
            {domain && <em aria-hidden="true">.{domain}</em>}
          </span>
          <span className="d-hint">Huruf kecil, angka, dan tanda hubung. Boleh dikosongkan agar dibuat otomatis.</span>
        </label>
        <label className="d-field"><span>Email</span><input name="email" type="email" autoComplete="email" required placeholder="nama@contoh.id" /></label>
        <PasswordInput name="password" autoComplete="new-password" minLength={8} value={password} onChange={setPassword} hint={strength ?? "Minimal 8 karakter."} />
        <label className="d-check"><input name="terms" type="checkbox" required /> <span>Saya menyetujui <a href="/legal/syarat" target="_blank" rel="noopener">syarat & ketentuan</a></span></label>
        <label className="d-check"><input name="privacy" type="checkbox" required /> <span>Saya menyetujui <a href="/legal/privasi" target="_blank" rel="noopener">kebijakan privasi</a></span></label>
        <BusyButton busy={busy} className="d-btn d-btn-primary d-btn-block d-btn-lg">Daftar</BusyButton>
      </form>
      <p>Sudah punya akun? <Link href="/login">Masuk</Link></p>
    </AuthShell>
  );
}
