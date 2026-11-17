"use client";

// Keamanan akun (AUTH-011/012): MFA TOTP — siapkan, aktifkan, kode pemulihan, matikan; plus kebijakan
// "wajibkan MFA untuk admin" milik toko bila dibuka dengan ?tenant=. Rahasia & kode pemulihan hanya
// ditampilkan sekali dan tidak disimpan di browser.

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState, useSyncExternalStore, type FormEvent } from "react";

import { Alert } from "@/dashboard/shell";
import { deviceName, relatif } from "@/lib/labels";
import { PasswordInput } from "@/ui/auth";
import { confirm } from "@/ui/dialog";
import { toast } from "@/ui/toast";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { api, ApiError, describe, isMfaRequired, loadSession, tenantParam, type Session, type TenantView } from "@/lib/api";
import { t as tx } from "@/i18n";
import { Tr } from "@/i18n/react";

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
    })().catch(() => setMsg({ err: tx("Tidak dapat menghubungi server.") }));
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
      toast.ok(tx("MFA aktif. Sesi di perangkat lain telah dikeluarkan."));
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
      toast.ok(tx("Kode pemulihan baru dibuat; kode lama tidak berlaku lagi."));
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
      toast.ok(tx("MFA dimatikan."));
    });
  }

  const mustEnroll = reason || !!session?.mfa_enrollment_required;

  return (
    <>
      <PageHeader title={tx("Keamanan akun")} description={tx("Kata sandi, perangkat yang sedang masuk, dan verifikasi dua langkah (MFA). Berlaku untuk semua toko Anda.")} />
      {mustEnroll && status && !status.session_verified && (
        <Alert kind="warn">
          {status.enabled
            ? tx("Akses ini mewajibkan MFA. Keluar lalu masuk lagi dengan kode authenticator.")
            : tx("Akun atau toko Anda mewajibkan verifikasi dua langkah (MFA). Aktifkan MFA untuk melanjutkan.")}
        </Alert>
      )}
      <Alert kind="error">{msg.err}</Alert>
      {!status && !msg.err && <PageSkeleton />}

      {status && (
        <Card
          title={tx("Verifikasi dua langkah (MFA)")}
          description={tx("Setiap kali masuk, Anda diminta kode 6 digit dari aplikasi authenticator di ponsel.")}
        >
          <p>
            {tx("Status:")}{" "}<strong>{status.enabled ? tx("Aktif") : tx("Belum aktif")}</strong>
            {status.enabled && tx(" · sisa kode pemulihan: {0}", status.recovery_codes_remaining)}
          </p>
          {status.required_by_platform && <p className="d-muted">{tx("Akun platform ini wajib memakai MFA.")}</p>}

          {!status.enabled && !setup && (
            <>
              <p className="d-muted">
                {tx("Gunakan aplikasi authenticator (mis. Google Authenticator, Microsoft Authenticator, 2FAS, Aegis).")}
              </p>
              <BusyButton type="button" busy={busy} onClick={begin}>{tx("Siapkan MFA")}</BusyButton>
            </>
          )}

          {setup && (
            <form onSubmit={enable} className="d-stack" style={{ maxWidth: 520 }}>
              <ol style={{ margin: 0 }}>
                <li>
                  {tx("Di aplikasi authenticator pilih “masukkan kunci”, lalu ketik kunci berikut (jenis: berbasis waktu):")}
                  <p style={{ margin: "8px 0" }}><code aria-label={tx("Kunci rahasia")} data-testid="mfa-secret" style={{ fontSize: "0.9375rem", letterSpacing: "0.05em" }}>{groups(setup.secret)}</code></p>
                  <p className="d-muted">{tx("Di ponsel yang sama, Anda juga bisa")}{" "}<a href={setup.otpauth_uri}>{tx("membuka tautan ini")}</a>{" "}{tx("di aplikasi authenticator.")}</p>
                </li>
                <li>{tx("Masukkan 6 digit yang muncul di aplikasi.")}</li>
              </ol>
              <label className="d-field"><span>{tx("Kode dari aplikasi")}</span>
                <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required style={{ maxWidth: 200 }} />
              </label>
              <div className="d-row">
                <BusyButton busy={busy}>{tx("Aktifkan")}</BusyButton>
                <button type="button" className="d-btn" onClick={() => setSetup(null)}>{tx("Batal")}</button>
              </div>
            </form>
          )}

          {codes && (
            <div className="d-alert d-alert-warn" role="status">
              <div>
                <p><strong>{tx("Simpan kode pemulihan ini sekarang.")}</strong>{" "}{tx("Setiap kode hanya bisa dipakai sekali bila ponsel Anda hilang. Kode ini tidak akan ditampilkan lagi.")}</p>
                <ul aria-label={tx("Kode pemulihan")} style={{ columns: 2, margin: 0 }}>
                  {codes.map((c) => <li key={c}><code>{c}</code></li>)}
                </ul>
              </div>
            </div>
          )}

          {status.enabled && (
            <div className="d-grid2" style={{ marginTop: 16 }}>
              <form onSubmit={regenerate}>
                <h3>{tx("Buat ulang kode pemulihan")}</h3>
                <p className="d-muted d-small">{tx("Kode lama langsung tidak berlaku.")}</p>
                <label className="d-field"><span>{tx("Kode dari aplikasi")}</span>
                  <input name="code" autoComplete="one-time-code" inputMode="numeric" maxLength={7} required />
                </label>
                <BusyButton busy={busy} className="d-btn">{tx("Buat ulang")}</BusyButton>
              </form>
              {!status.required_by_platform && (
                <form onSubmit={disable}>
                  <h3>{tx("Matikan MFA")}</h3>
                  <p className="d-muted d-small">{tx("Akun Anda akan kembali hanya dilindungi password.")}</p>
                  <label className="d-field"><span>{tx("Password")}</span>
                    <input name="password" type="password" autoComplete="current-password" required />
                  </label>
                  <label className="d-field"><span>{tx("Kode dari aplikasi atau kode pemulihan")}</span>
                    <input name="code" autoComplete="one-time-code" maxLength={16} required />
                  </label>
                  <BusyButton busy={busy} className="d-btn d-btn-danger">{tx("Matikan MFA")}</BusyButton>
                </form>
              )}
            </div>
          )}
        </Card>
      )}

      {status && tenantId && <TenantPolicy tenantId={tenantId} verified={status.session_verified} />}
      {status && session && <ChangePassword />}
      {status && session && <Sessions />}
    </>
  );
}

interface SessionRow { id: string; user_agent: string | null; created_at: string; last_seen_at: string; current: boolean }

/** AUTH-006: perangkat yang sedang masuk; cabut satu atau semua yang lain. */
function Sessions() {
  const [list, setList] = useState<SessionRow[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(() => api<SessionRow[]>("GET", "/api/v1/auth/sessions").then(setList, (e) => setErr(describe(e))), []);
  useEffect(() => { void load(); }, [load]);
  async function revoke(row: SessionRow) {
    if (!(await confirm({ title: tx("Keluarkan perangkat ini?"), body: tx("{0} harus masuk lagi untuk memakai akun Anda.", deviceName(row.user_agent)), confirmLabel: tx("Keluarkan"), danger: true }))) return;
    setBusy(row.id);
    try { await api("POST", `/api/v1/auth/sessions/${row.id}/revoke`); await load(); toast.ok(tx("Perangkat dikeluarkan.")); }
    catch (e) { setErr(describe(e)); } finally { setBusy(null); }
  }
  async function revokeOthers() {
    if (!(await confirm({ title: tx("Keluar dari semua perangkat lain?"), body: tx("Semua perangkat selain yang ini harus masuk lagi."), confirmLabel: tx("Keluar dari semua"), danger: true }))) return;
    setBusy("all");
    try {
      const r = await api<{ revoked: number }>("POST", "/api/v1/auth/sessions/revoke-others");
      await load();
      toast.ok(r.revoked ? tx("{0} perangkat dikeluarkan.", r.revoked) : tx("Tidak ada perangkat lain."));
    } catch (e) { setErr(describe(e)); } finally { setBusy(null); }
  }
  const others = list?.filter((s) => !s.current).length ?? 0;
  return (
    <Card
      title={tx("Perangkat yang sedang masuk")}
      description={tx("Keluarkan perangkat yang tidak Anda kenali, lalu ganti kata sandi.")}
      actions={others > 0 ? <BusyButton type="button" busy={busy === "all"} className="d-btn d-btn-sm" onClick={() => void revokeOthers()}>{tx("Keluar dari semua perangkat lain")}</BusyButton> : undefined}
    >
      <Alert kind="error">{err}</Alert>
      {!list && !err && <span className="d-skel d-skel-block" style={{ height: 96 }} aria-hidden="true" />}
      {list && (
        <ul className="d-list" aria-label={tx("Sesi aktif")}>
          {list.map((s) => (
            <li key={s.id}>
              <div className="d-list-main">
                <strong>{deviceName(s.user_agent)} {s.current && <Badge tone="ok">{tx("Perangkat ini")}</Badge>}</strong>
                <span className="d-muted"><Tr s="Masuk {0} · aktif" a={[relatif(s.created_at)]} />{" "}{relatif(s.last_seen_at)}</span>
              </div>
              {!s.current && (
                <BusyButton type="button" busy={busy === s.id} className="d-btn d-btn-sm d-btn-ghost" onClick={() => void revoke(s)}>{tx("Keluarkan")}</BusyButton>
              )}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/** Ubah kata sandi: wajib kata sandi saat ini; perangkat lain otomatis dikeluarkan. */
function ChangePassword() {
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const f = new FormData(form);
    if (f.get("new_password") !== f.get("confirm_password")) {
      setErr(tx("Konfirmasi kata sandi baru tidak sama."));
      return;
    }
    setErr(null);
    setBusy(true);
    try {
      const r = await api<{ other_sessions_revoked: number }>("POST", "/api/v1/auth/password", {
        json: { current_password: f.get("current_password"), new_password: f.get("new_password") },
      });
      form.reset();
      toast.ok(r.other_sessions_revoked ? tx("Kata sandi diubah. {0} perangkat lain dikeluarkan.", r.other_sessions_revoked) : tx("Kata sandi diubah."));
    } catch (e2) {
      setErr(describe(e2, { current_password: tx("Kata sandi saat ini"), new_password: tx("Kata sandi baru") }));
      requestAnimationFrame(() => form.querySelector<HTMLElement>("[role=alert]")?.focus());
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card title={tx("Kata sandi")} description={tx("Setelah diubah, semua perangkat lain harus masuk lagi.")}>
      <form onSubmit={submit} className="d-stack" style={{ maxWidth: 420 }}>
        <Alert kind="error">{err}</Alert>
        <PasswordInput name="current_password" label={tx("Kata sandi saat ini")} autoComplete="current-password" />
        <PasswordInput name="new_password" label={tx("Kata sandi baru")} autoComplete="new-password" minLength={8} hint={tx("Minimal 8 karakter, tidak sama dengan email.")} />
        <label className="d-field"><span>{tx("Ulangi kata sandi baru")}</span>
          <input name="confirm_password" type="password" autoComplete="new-password" minLength={8} required />
        </label>
        <div><BusyButton busy={busy}>{tx("Ubah kata sandi")}</BusyButton></div>
      </form>
    </Card>
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
      toast.ok(tx("Kebijakan toko tersimpan."));
    } catch (e) {
      setMsg({ err: describe(e) });
    }
  }

  if (msg.blocked) {
    return (
      <div className="d-card">
        <h2>{tx("Kebijakan toko")}</h2>
        <p className="d-muted">{tx("Toko ini mewajibkan MFA untuk peran Anda. Aktifkan MFA di atas, lalu buka toko lagi.")}</p>
      </div>
    );
  }
  if (value === null && !msg.err) return null;
  return (
    <div className="d-card">
      <h2>{tx("Kebijakan toko")}</h2>
      <Alert kind="error">{msg.err}</Alert>
      {value !== null && (
        <>
          <label className="d-check">
            <input type="checkbox" disabled={!canUpdate} checked={value} onChange={(e) => save(e.target.checked)} />
            {tx("Wajibkan MFA untuk Pemilik, Manajer, dan Admin toko ini")}
          </label>
          <p className="d-muted">
            {tx("Anggota dengan peran tersebut yang belum masuk dengan MFA akan diarahkan ke halaman ini. Anda harus sudah masuk dengan MFA sebelum dapat mengaktifkannya.")}
          </p>
        </>
      )}
    </div>
  );
}
