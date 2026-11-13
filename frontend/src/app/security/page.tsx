"use client";

// Keamanan akun (AUTH-011/012): MFA TOTP — siapkan, aktifkan, kode pemulihan, matikan; plus kebijakan
// "wajibkan MFA untuk admin" milik toko bila dibuka dengan ?tenant=. Rahasia & kode pemulihan hanya
// ditampilkan sekali dan tidak disimpan di browser.

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert, TopBar } from "@/dashboard/shell";
import { api, ApiError, describe, isMfaRequired, loadSession, tenantParam, type Session, type TenantView } from "@/lib/api";

interface MfaStatus {
  enabled: boolean; recovery_codes_remaining: number; session_verified: boolean; required_by_platform: boolean;
}
interface Setup { secret: string; otpauth_uri: string }
interface Codes { recovery_codes: string[] }

/** Kunci base32 dikelompokkan 4 karakter agar mudah diketik manual. */
function groups(s: string): string {
  return s.replace(/(.{4})/g, "$1 ").trim();
}

const noop = () => () => {};
const reasonParam = () => new URLSearchParams(window.location.search).get("reason") === "mfa";

export default function SecurityPage() {
  const router = useRouter();
  const [session, setSession] = useState<Session | null>(null);
  const [status, setStatus] = useState<MfaStatus | null>(null);
  const [setup, setSetup] = useState<Setup | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const [busy, setBusy] = useState(false);
  const reason = useSyncExternalStore(noop, reasonParam, () => false);
  const tenantId = useSyncExternalStore(noop, tenantParam, () => null);

  const refresh = useCallback(async () => {
    const s = await loadSession();
    if (!s) {
      router.replace("/login");
      return;
    }
    setSession(s);
    setStatus(await api<MfaStatus>("GET", "/api/v1/auth/mfa"));
  }, [router]);

  useEffect(() => {
    (async () => {
      await refresh();
    })().catch(() => setMsg({ err: "Tidak dapat menghubungi server." }));
  }, [refresh]);

  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setMsg({});
    try {
      await fn();
    } catch (e) {
      setMsg({ err: describe(e) });
    } finally {
      setBusy(false);
    }
  }

  const begin = () => run(async () => {
    setCodes(null);
    setSetup(await api<Setup>("POST", "/api/v1/auth/mfa/setup"));
  });

  function enable(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const code = new FormData(e.currentTarget).get("code");
    return run(async () => {
      const r = await api<Codes>("POST", "/api/v1/auth/mfa/enable", { json: { code } });
      setSetup(null);
      setCodes(r.recovery_codes);
      await refresh();
      setMsg({ ok: "MFA aktif. Sesi di perangkat lain telah dikeluarkan." });
    });
  }

  function regenerate(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const code = new FormData(form).get("code");
    return run(async () => {
      const r = await api<Codes>("POST", "/api/v1/auth/mfa/recovery-codes", { json: { code } });
      form.reset();
      setCodes(r.recovery_codes);
      await refresh();
      setMsg({ ok: "Kode pemulihan baru dibuat; kode lama tidak berlaku lagi." });
    });
  }

  function disable(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const f = new FormData(form);
    return run(async () => {
      await api("POST", "/api/v1/auth/mfa/disable", { json: { password: f.get("password"), code: f.get("code") } });
      form.reset();
      setCodes(null);
      await refresh();
      setMsg({ ok: "MFA dimatikan." });
    });
  }

  const mustEnroll = reason || !!session?.mfa_enrollment_required;

  return (
    <>
      <TopBar />
      <main className="d-main">
        <h1>Keamanan akun</h1>
        {mustEnroll && status && !status.session_verified && (
          <Alert kind="error">
            {status.enabled
              ? "Akses ini mewajibkan MFA. Keluar lalu masuk lagi dengan kode authenticator."
              : "Akun atau toko Anda mewajibkan verifikasi dua langkah (MFA). Aktifkan MFA untuk melanjutkan."}
          </Alert>
        )}
        <Alert kind="error">{msg.err}</Alert>
        <Alert kind="ok">{msg.ok}</Alert>
        {!status && !msg.err && <p className="d-muted" aria-busy="true">Memuat…</p>}

        {status && (
          <div className="d-card">
            <h2>Verifikasi dua langkah (MFA)</h2>
            <p>
              Status: <strong>{status.enabled ? "Aktif" : "Belum aktif"}</strong>
              {status.enabled && ` · sisa kode pemulihan: ${status.recovery_codes_remaining}`}
            </p>
            {status.required_by_platform && <p className="d-muted">Akun platform ini wajib memakai MFA.</p>}

            {!status.enabled && !setup && (
              <>
                <p className="d-muted">
                  Gunakan aplikasi authenticator (mis. Google Authenticator, Microsoft Authenticator, 2FAS, Aegis).
                </p>
                <button className="d-btn d-btn-primary" disabled={busy} onClick={begin}>Siapkan MFA</button>
              </>
            )}

            {setup && (
              <form onSubmit={enable}>
                <ol>
                  <li>
                    Di aplikasi authenticator pilih &ldquo;masukkan kunci&rdquo;, lalu ketik kunci berikut (jenis: berbasis waktu):
                    <p><code aria-label="Kunci rahasia" data-testid="mfa-secret">{groups(setup.secret)}</code></p>
                    <p className="d-muted">Di ponsel yang sama, Anda juga bisa <a href={setup.otpauth_uri}>membuka tautan ini</a> di aplikasi authenticator.</p>
                  </li>
                  <li>Masukkan 6 digit yang muncul di aplikasi.</li>
                </ol>
                <label className="d-field"><span>Kode dari aplikasi</span>
                  <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required />
                </label>
                <div className="d-row">
                  <button className="d-btn d-btn-primary" disabled={busy}>Aktifkan</button>
                  <button type="button" className="d-btn" onClick={() => setSetup(null)}>Batal</button>
                </div>
              </form>
            )}

            {codes && (
              <div className="d-alert d-alert-ok" role="status">
                <p><strong>Simpan kode pemulihan ini sekarang.</strong> Setiap kode hanya bisa dipakai sekali bila ponsel
                  Anda hilang. Kode ini tidak akan ditampilkan lagi.</p>
                <ul aria-label="Kode pemulihan">
                  {codes.map((c) => <li key={c}><code>{c}</code></li>)}
                </ul>
              </div>
            )}

            {status.enabled && (
              <>
                <h3>Buat ulang kode pemulihan</h3>
                <form onSubmit={regenerate}>
                  <label className="d-field"><span>Kode dari aplikasi</span>
                    <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required />
                  </label>
                  <button className="d-btn" disabled={busy}>Buat ulang</button>
                </form>
                {!status.required_by_platform && (
                  <>
                    <h3>Matikan MFA</h3>
                    <form onSubmit={disable}>
                      <label className="d-field"><span>Password</span>
                        <input name="password" type="password" autoComplete="current-password" required />
                      </label>
                      <label className="d-field"><span>Kode dari aplikasi atau kode pemulihan</span>
                        <input name="code" autoComplete="one-time-code" maxLength={16} required />
                      </label>
                      <button className="d-btn" disabled={busy}>Matikan MFA</button>
                    </form>
                  </>
                )}
              </>
            )}
          </div>
        )}

        {status && tenantId && <TenantPolicy tenantId={tenantId} verified={status.session_verified} />}
      </main>
    </>
  );
}

/** AUTH-012: Owner mewajibkan MFA untuk Owner/Manager/Admin toko. */
function TenantPolicy({ tenantId, verified }: { tenantId: string; verified: boolean }) {
  const url = `/api/v1/tenants/${tenantId}/security`;
  const [value, setValue] = useState<boolean | null>(null);
  const [canUpdate, setCanUpdate] = useState(false);
  const [msg, setMsg] = useState<{ ok?: string; err?: string; blocked?: boolean }>({});

  useEffect(() => {
    (async () => {
      const t = await api<TenantView>("GET", `/api/v1/tenants/${tenantId}`);
      if (!t.my_permissions.includes("tenant.read")) return;
      setCanUpdate(t.my_permissions.includes("tenant.update"));
      setValue((await api<{ require_admin_mfa: boolean }>("GET", url)).require_admin_mfa);
    })().catch((e) => {
      if (isMfaRequired(e)) setMsg({ blocked: true });
      else if (!(e instanceof ApiError && e.status === 404)) setMsg({ err: describe(e) });
    });
  }, [tenantId, url, verified]);

  async function save(v: boolean) {
    setMsg({});
    try {
      const r = await api<{ require_admin_mfa: boolean }>("PUT", url, { json: { require_admin_mfa: v } });
      setValue(r.require_admin_mfa);
      setMsg({ ok: "Kebijakan toko tersimpan." });
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  if (msg.blocked) {
    return (
      <div className="d-card">
        <h2>Kebijakan toko</h2>
        <p>Toko ini mewajibkan MFA untuk peran Anda. Aktifkan MFA di atas, lalu buka toko lagi.</p>
      </div>
    );
  }
  if (value === null && !msg.err) return null;
  return (
    <div className="d-card">
      <h2>Kebijakan toko</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {value !== null && (
        <>
          <label className="d-check">
            <input type="checkbox" disabled={!canUpdate} checked={value} onChange={(e) => save(e.target.checked)} />
            Wajibkan MFA untuk Owner, Manager, dan Admin toko ini
          </label>
          <p className="d-muted">
            Anggota dengan peran tersebut yang belum masuk dengan MFA akan diarahkan ke halaman ini. Anda harus sudah
            masuk dengan MFA sebelum dapat mengaktifkannya.
          </p>
        </>
      )}
    </div>
  );
}
