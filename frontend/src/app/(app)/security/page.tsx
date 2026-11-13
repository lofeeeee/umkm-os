"use client";

// Keamanan akun (AUTH-011/012): MFA TOTP — siapkan, aktifkan, kode pemulihan, matikan; plus kebijakan
// "wajibkan MFA untuk admin" milik toko bila dibuka dengan ?tenant=. Rahasia & kode pemulihan hanya
// ditampilkan sekali dan tidak disimpan di browser.

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { toast } from "@/ui/toast";
import { BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
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
  const [msg, setMsg] = useState<{ err?: string }>({});
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
      toast.ok("MFA aktif. Sesi di perangkat lain telah dikeluarkan.");
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
      toast.ok("Kode pemulihan baru dibuat; kode lama tidak berlaku lagi.");
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
      toast.ok("MFA dimatikan.");
    });
  }

  const mustEnroll = reason || !!session?.mfa_enrollment_required;

  return (
    <>
      <PageHeader title="Keamanan akun" description="Lindungi akun Anda dengan verifikasi dua langkah (MFA). Pengaturan ini berlaku untuk semua toko Anda." />
      {mustEnroll && status && !status.session_verified && (
        <Alert kind="warn">
          {status.enabled
            ? "Akses ini mewajibkan MFA. Keluar lalu masuk lagi dengan kode authenticator."
            : "Akun atau toko Anda mewajibkan verifikasi dua langkah (MFA). Aktifkan MFA untuk melanjutkan."}
        </Alert>
      )}
      <Alert kind="error">{msg.err}</Alert>
      {!status && !msg.err && <PageSkeleton />}

      {status && (
        <Card
          title="Verifikasi dua langkah (MFA)"
          description="Setiap kali masuk, Anda diminta kode 6 digit dari aplikasi authenticator di ponsel."
        >
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
              <BusyButton type="button" busy={busy} onClick={begin}>Siapkan MFA</BusyButton>
            </>
          )}

          {setup && (
            <form onSubmit={enable} className="d-stack" style={{ maxWidth: 520 }}>
              <ol style={{ margin: 0 }}>
                <li>
                  Di aplikasi authenticator pilih &ldquo;masukkan kunci&rdquo;, lalu ketik kunci berikut (jenis: berbasis waktu):
                  <p style={{ margin: "8px 0" }}><code aria-label="Kunci rahasia" data-testid="mfa-secret" style={{ fontSize: "0.9375rem", letterSpacing: "0.05em" }}>{groups(setup.secret)}</code></p>
                  <p className="d-muted">Di ponsel yang sama, Anda juga bisa <a href={setup.otpauth_uri}>membuka tautan ini</a> di aplikasi authenticator.</p>
                </li>
                <li>Masukkan 6 digit yang muncul di aplikasi.</li>
              </ol>
              <label className="d-field"><span>Kode dari aplikasi</span>
                <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required style={{ maxWidth: 200 }} />
              </label>
              <div className="d-row">
                <BusyButton busy={busy}>Aktifkan</BusyButton>
                <button type="button" className="d-btn" onClick={() => setSetup(null)}>Batal</button>
              </div>
            </form>
          )}

          {codes && (
            <div className="d-alert d-alert-warn" role="status">
              <div>
                <p><strong>Simpan kode pemulihan ini sekarang.</strong> Setiap kode hanya bisa dipakai sekali bila ponsel
                  Anda hilang. Kode ini tidak akan ditampilkan lagi.</p>
                <ul aria-label="Kode pemulihan" style={{ columns: 2, margin: 0 }}>
                  {codes.map((c) => <li key={c}><code>{c}</code></li>)}
                </ul>
              </div>
            </div>
          )}

          {status.enabled && (
            <div className="d-grid2" style={{ marginTop: 16 }}>
              <form onSubmit={regenerate}>
                <h3>Buat ulang kode pemulihan</h3>
                <p className="d-muted d-small">Kode lama langsung tidak berlaku.</p>
                <label className="d-field"><span>Kode dari aplikasi</span>
                  <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required />
                </label>
                <BusyButton busy={busy} className="d-btn">Buat ulang</BusyButton>
              </form>
              {!status.required_by_platform && (
                <form onSubmit={disable}>
                  <h3>Matikan MFA</h3>
                  <p className="d-muted d-small">Akun Anda akan kembali hanya dilindungi password.</p>
                  <label className="d-field"><span>Password</span>
                    <input name="password" type="password" autoComplete="current-password" required />
                  </label>
                  <label className="d-field"><span>Kode dari aplikasi atau kode pemulihan</span>
                    <input name="code" autoComplete="one-time-code" maxLength={16} required />
                  </label>
                  <BusyButton busy={busy} className="d-btn d-btn-danger">Matikan MFA</BusyButton>
                </form>
              )}
            </div>
          )}
        </Card>
      )}

      {status && tenantId && <TenantPolicy tenantId={tenantId} verified={status.session_verified} />}
    </>
  );
}

/** AUTH-012: Owner mewajibkan MFA untuk Owner/Manager/Admin toko. */
function TenantPolicy({ tenantId, verified }: { tenantId: string; verified: boolean }) {
  const url = `/api/v1/tenants/${tenantId}/security`;
  const [value, setValue] = useState<boolean | null>(null);
  const [canUpdate, setCanUpdate] = useState(false);
  const [msg, setMsg] = useState<{ err?: string; blocked?: boolean }>({});

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
      toast.ok("Kebijakan toko tersimpan.");
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  if (msg.blocked) {
    return (
      <div className="d-card">
        <h2>Kebijakan toko</h2>
        <p className="d-muted">Toko ini mewajibkan MFA untuk peran Anda. Aktifkan MFA di atas, lalu buka toko lagi.</p>
      </div>
    );
  }
  if (value === null && !msg.err) return null;
  return (
    <div className="d-card">
      <h2>Kebijakan toko</h2>
      <Alert kind="error">{msg.err}</Alert>
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
