"use client";

// Pengaturan toko online (P3): QRIS statis milik toko (ADR-0004), ongkir, approval, SoD.
// Hanya Owner (tenant.update) yang dapat mengubah; pergantian QRIS tercatat di audit log.

import Link from "@/ui/link";
import { useEffect, useRef, useState } from "react";

import { useUploads } from "@/dashboard/assets";
import { Assets, ImagePicker } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";
import { api, describe } from "@/lib/api";
import { confirm } from "@/ui/dialog";
import { t } from "@/i18n";
import { relatif, waktu } from "@/lib/labels";

interface Settings {
  qris: string | null; qris_receiver_name: string | null; checkout_enabled: boolean; delivery_fee: string;
  order_approval_required: boolean; reservation_ttl_minutes: number; separation_of_duties: boolean;
  qris_static_ready: boolean;
}

export default function ShopSettingsPage() {
  return <WithTenant>{(ctx) => <Form ctx={ctx} />}</WithTenant>;
}

function Form({ ctx }: { ctx: TenantCtx }) {
  const url = `/api/v1/tenants/${ctx.tenant.id}/commerce/settings`;
  const can = ctx.can("tenant.update");
  const assets = useUploads(ctx.tenant.id, can);
  const [s, setS] = useState<Settings | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  // Tanpa ambil di toko maupun pengiriman, checkout publik selalu ditolak (UAT 15 Nov #1).
  const [fulfil, setFulfil] = useState<{ pickup: boolean; delivery: boolean } | null>(null);
  useEffect(() => { api<Settings>("GET", url).then(setS, (e) => setErr(describe(e))); }, [url]);
  useEffect(() => {
    api<{ profile: { pickup_available: boolean; delivery_available: boolean } }>("GET", `/api/v1/tenants/${ctx.tenant.id}/business/profile`)
      .then((b) => setFulfil({ pickup: b.profile.pickup_available, delivery: b.profile.delivery_available }), () => setFulfil(null));
  }, [ctx.tenant.id]);
  if (!s) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const current = s;
  async function save() {
    setErr(null);
    setBusy(true);
    try {
      // Kirim hanya field yang dapat ditulis; SettingsDto memakai deny_unknown_fields.
      const input = {
        qris: current.qris,
        qris_receiver_name: current.qris_receiver_name,
        checkout_enabled: current.checkout_enabled,
        delivery_fee: current.delivery_fee,
        order_approval_required: current.order_approval_required,
        reservation_ttl_minutes: current.reservation_ttl_minutes,
        separation_of_duties: current.separation_of_duties,
      };
      setS(await api<Settings>("PUT", url, { json: input }));
      setRefreshKey((k) => k + 1);
      toast.ok(t("Tersimpan."));
    } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  const noFulfil = !!fulfil && !fulfil.pickup && !fulfil.delivery;
  const ready = !!s.qris && s.checkout_enabled && !noFulfil;
  return (
    <Assets.Provider value={assets}>
      <PageHeader
        title={t("Pembayaran & pengiriman")}
        description={t("Cara pembeli membayar dan menerima pesanan dari website toko.")}
        actions={<Badge tone={ready ? "ok" : "warn"}>{ready ? t("Pesanan online aktif") : t("Pesanan online belum aktif")}</Badge>}
      />
      {!can && <Alert kind="info">{t("Anda hanya dapat melihat. Hanya pemilik toko yang dapat mengubah pengaturan ini.")}</Alert>}
      <Alert kind="error">{err}</Alert>
      {noFulfil && (
        <Alert kind="warn">
          {t("Belum ada cara pengambilan yang aktif. Aktifkan \"Bisa ambil di tempat\" atau \"Bisa diantar\" di")}{" "}<Link href="/business">{t("Profil bisnis")}</Link>{" "}{t("agar pembeli bisa membuat pesanan.")}
        </Alert>
      )}
      <form onSubmit={(e) => { e.preventDefault(); void save(); }}>
        <Card title={t("Pembayaran QRIS")} description={t("Unggah gambar QRIS merchant milik toko — sistem membaca QR-nya secara otomatis. Pembeli membayar langsung ke rekening toko; Anda mengonfirmasi setiap pembayaran di menu Pesanan. Platform tidak memegang dana.")}>
          <div className="d-grid">
            <ImagePicker label={t("Gambar QRIS")} value={s.qris ?? undefined} onChange={(v) => setS({ ...s, qris: v ?? null })} />
            <label className="d-field"><span>{t("Nama penerima (tampil di halaman bayar)")}</span>
              <input disabled={!can} maxLength={80} placeholder={t("mis. Kopi Senja")} value={s.qris_receiver_name ?? ""} onChange={(e) => setS({ ...s, qris_receiver_name: e.target.value || null })} />
            </label>
          </div>
          {s.qris && !s.qris_static_ready && (
            <Alert kind="warn">
              {t("QRIS belum terbaca dari gambar. Unggah ulang gambar QRIS yang jelas; konfirmasi manual tetap tersedia.")}
            </Alert>
          )}
          {s.qris && s.qris_static_ready && (
            <Alert kind="ok">{t("QRIS terbaca — konfirmasi otomatis siap.")}</Alert>
          )}
          <label className="d-switch">
            <input type="checkbox" disabled={!can} checked={s.checkout_enabled} onChange={(e) => setS({ ...s, checkout_enabled: e.target.checked })} />
            <span><strong>{t("Terima pesanan online")}</strong><span className="d-muted">{t("Tombol beli dan keranjang muncul di website.")}</span></span>
          </label>
        </Card>
        <GopayCard tenantId={ctx.tenant.id} can={can} refreshKey={refreshKey} />
        <Card title={t("Pengiriman & pesanan")} description={t("Pilihan ambil di toko / diantar diatur di Profil bisnis.")}>
          <div className="d-grid">
            <label className="d-field"><span>{t("Ongkir datar untuk pengiriman")}</span>
              <div className="d-input-prefix"><span aria-hidden="true">{t("Rp")}</span><input aria-label={t("Ongkir datar untuk pengiriman (Rp)")} disabled={!can} inputMode="decimal" value={s.delivery_fee} onChange={(e) => setS({ ...s, delivery_fee: e.target.value })} /></div>
            </label>
            <label className="d-field"><span>{t("Batas waktu pembayaran")}</span>
              <div className="d-input-suffix"><input aria-label={t("Batas waktu pembayaran (menit, 15–1440)")} disabled={!can} type="number" min={15} max={1440} value={s.reservation_ttl_minutes} onChange={(e) => setS({ ...s, reservation_ttl_minutes: Number(e.target.value) })} /><em aria-hidden="true">menit</em></div>
              <span className="d-hint">{t("15–1440 menit. Stok ditahan selama waktu ini.")}</span>
            </label>
          </div>
          <label className="d-switch"><input type="checkbox" disabled={!can} checked={s.order_approval_required} onChange={(e) => setS({ ...s, order_approval_required: e.target.checked })} />
            <span><strong>{t("Pesanan yang sudah dibayar perlu disetujui sebelum diproses")}</strong><span className="d-muted">{t("Cocok bila stok/produksi perlu dicek manual.")}</span></span></label>
          <label className="d-switch"><input type="checkbox" disabled={!can} checked={s.separation_of_duties} onChange={(e) => setS({ ...s, separation_of_duties: e.target.checked })} />
            <span><strong>{t("Penyetuju harus berbeda dari staf yang mengonfirmasi pembayaran")}</strong><span className="d-muted">{t("Mencegah satu orang mengonfirmasi dan menyetujui sendiri.")}</span></span></label>
        </Card>
        {can && (
          <div className="d-savebar">
            <span className="d-muted d-small">{t("Perubahan QRIS tercatat di log aktivitas.")}</span>
            <BusyButton busy={busy}>{t("Simpan")}</BusyButton>
          </div>
        )}
      </form>
    </Assets.Provider>
  );
}

// P10 (ADR-0020): konfirmasi pembayaran QRIS otomatis via GoPay Merchant (GoBiz).
interface GopayStatus {
  connected: boolean; status: string | null; outlet_name: string | null;
  phone_masked: string | null; has_static_qris: boolean;
  /** Detik epoch: token terakhir diperbarui / kedaluwarsa. Diperbarui otomatis oleh server. */
  refreshed_at_unix: number | null; expires_at_unix: number | null;
  /** Kode kegagalan pembaruan terakhir (mis. `network`, `status_503`). */
  last_error: string | null;
}

/** Penjelasan singkat kode kegagalan pembaruan sesi dari server. */
function gopayErrorText(code: string): string {
  if (code.startsWith("refresh_")) return t("GoBiz menolak perpanjangan sesi");
  if (code === "network") return t("GoBiz tidak dapat dihubungi");
  if (code === "parse") return t("respons GoBiz tidak dikenali");
  if (code === "unauthorized" || code === "status_400" || code === "status_403") return t("GoBiz menolak sesi");
  if (code === "status_429") return t("terlalu banyak permintaan ke GoBiz");
  if (code.startsWith("status_5")) return t("server GoBiz sedang gangguan");
  return code;
}

// Ambil cookie sesi yang dioper ekstensi via fragment URL (#gopay_connect=<base64 cookie>).
function readConnectCookie(): string | null {
  if (typeof window === "undefined") return null;
  const token = window.location.hash.match(/gopay_connect=([^&]+)/)?.[1];
  if (!token) return null;
  try {
    const cookie = decodeURIComponent(escape(atob(decodeURIComponent(token))));
    return cookie.includes("access_token") && cookie.includes("refresh_token") ? cookie : null;
  } catch {
    return null;
  }
}

function GopayCard({ tenantId, can, refreshKey }: { tenantId: string; can: boolean; refreshKey: number }) {
  const base = `/api/v1/tenants/${tenantId}/gopay`;
  const [st, setSt] = useState<GopayStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // Hubungkan via sesi browser: login OTP GoBiz terkunci reCAPTCHA (hanya bisa di browser),
  // jadi pemilik login di browser lalu menempel cookie/token sesinya.
  const [phone, setPhone] = useState("");
  const [session, setSession] = useState<string>(() => readConnectCookie() ?? "");
  const [help, setHelp] = useState(false);
  const [busy, setBusy] = useState(false);
  // Sesi datang dari ekstensi (via #gopay_connect) → kirim otomatis sekali.
  const fromExt = useRef(readConnectCookie() !== null);
  const autoSent = useRef(false);

  useEffect(() => { api<GopayStatus>("GET", base).then(setSt, (e) => setErr(describe(e))); }, [base, refreshKey]);
  // Satukan nomor HP: prefill dari profil bisnis (bisa diubah). Tak menimpa ketikan pengguna.
  useEffect(() => {
    api<{ profile: { phone?: string | null; whatsapp?: string | null } }>("GET", `/api/v1/tenants/${tenantId}/business/profile`)
      .then((b) => { const p = b.profile.phone || b.profile.whatsapp; if (p) setPhone((cur) => cur || p); }, () => {});
  }, [tenantId]);
  // Deep-link dari banner beranda (#gopay): gulirkan kartu ke tampilan.
  useEffect(() => {
    if (typeof window !== "undefined" && window.location.hash === "#gopay") {
      document.getElementById("gopay")?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }, [st]);

  async function run(fn: () => Promise<void>) {
    setErr(null); setBusy(true);
    try { await fn(); } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  const connectSession = () => run(async () => {
    setSt(await api<GopayStatus>("POST", `${base}/connect/session`, { json: { phone, session } }));
    setSession(""); toast.ok(t("Akun GoPay terhubung."));
  });
  const check = () => run(async () => {
    const r = await api<GopayStatus & { result: string }>("POST", `${base}/check`);
    setSt(r);
    if (r.result === "valid") toast.ok(t("Sesi GoPay berfungsi."));
    else if (r.result === "refreshed") toast.ok(t("Sesi GoPay diperbarui dan berfungsi."));
    else if (r.result === "transient" && r.last_error?.startsWith("refresh_")) toast.info(t("Sesi GoPay masih berfungsi, tetapi GoBiz menolak perpanjangannya. Hubungkan ulang sebelum sesi berakhir."));
    else if (r.result === "needs_relogin") toast.error(t("GoBiz menolak sesi ini. Hubungkan ulang akun GoPay."));
    else if (r.result === "busy") toast.info(t("Sesi sedang diperbarui otomatis. Coba lagi sebentar."));
    else toast.info(t("GoBiz sedang tidak dapat dihubungi. Sesi tetap aktif dan dicoba lagi otomatis."));
  });
  const disconnect = () => run(async () => {
    const ok = await confirm({
      title: t("Putuskan akun GoPay?"),
      body: t("Pesanan tidak lagi dikonfirmasi otomatis."),
      confirmLabel: t("Putuskan akun"),
      danger: true,
    });
    if (!ok) return;
    setSt(await api<GopayStatus>("DELETE", base)); toast.ok(t("Akun GoPay diputus."));
  });

  // Bersihkan fragment token dari URL setelah ditangkap (tanpa setState).
  useEffect(() => {
    if (fromExt.current && typeof window !== "undefined") {
      window.history.replaceState(null, "", `${window.location.pathname}${window.location.search}#gopay`);
    }
  }, []);
  // Kirim otomatis bila sesi datang dari ekstensi & nomor sudah siap (sekali saja).
  useEffect(() => {
    if (autoSent.current || !fromExt.current || st?.connected) return;
    if (can && !busy && session.trim().length >= 20 && phone.trim().length >= 8) {
      autoSent.current = true;
      queueMicrotask(() => connectSession());
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session, phone, st, can, busy]);

  if (!st) return null;
  return (
    <div id="gopay">
    <Card
      title={t("Konfirmasi otomatis (GoPay Merchant)")}
      description={t("Opsional. Hubungkan akun GoPay/GoBiz Merchant toko agar pembayaran QRIS dikonfirmasi otomatis — pesanan langsung berstatus Diproses tanpa Anda cek manual. Gratis untuk usaha mikro. Konfirmasi manual tetap tersedia sebagai cadangan.")}
    >
      <Alert kind="error">{err}</Alert>
      {st.connected && st.status === "needs_relogin" && (
        <Alert kind="warn">
          {t("GoBiz menolak sesi yang tersimpan, jadi pembayaran tidak lagi dikonfirmasi otomatis. Hubungkan ulang di bawah — konfirmasi manual tetap berjalan.")}
          {st.last_error ? " " + t("(kode: {0})", st.last_error) : ""}
          {can && (
            <>
              {" "}
              <button type="button" className="d-link" disabled={busy} onClick={check}>{t("Cek ulang sesi")}</button>
            </>
          )}
        </Alert>
      )}
      {!st.connected || st.status === "needs_relogin" ? (
        <div className="gp-stack">
          <Alert kind="info">
            <strong>{t("Cara termudah (tanpa menyalin token):")}</strong>{" "}{t("pasang ekstensi")}{" "}<em>{t("\"UMKM OS — Penghubung GoPay\"")}</em>{t(", login GoBiz di browser, lalu klik")}{" "}<em>{t("Hubungkan")}</em>{" "}{t("di ekstensi — kolom di bawah terisi sendiri. Login GoPay dilindungi reCAPTCHA sehingga harus di browser; ekstensi hanya menangkap sesinya. Tak punya ekstensi? Isi manual di bawah.")}
          </Alert>
          <label className="d-field"><span>{t("Nomor HP GoBiz/GoPay Merchant")}</span>
            <input disabled={!can} inputMode="tel" placeholder={t("mis. 0812xxxxxxx")} value={phone} onChange={(e) => setPhone(e.target.value)} />
            <span className="d-hint">{t("Nomor akun merchant (dipakai untuk memperbarui sesi).")}</span>
          </label>
          <label className="d-field"><span>{t("Token sesi GoBiz")}</span>
            <textarea disabled={!can} rows={4} placeholder={t("Tempel cookie sesi (memuat access_token dan refresh_token)")} value={session} onChange={(e) => setSession(e.target.value)} />
            <span className="d-hint">
              <button type="button" className="d-link" onClick={() => setHelp((v) => !v)} aria-expanded={help}>
                {help ? t("Sembunyikan cara mendapatkan token") : t("Cara mendapatkan token sesi")}
              </button>
            </span>
          </label>
          {help && (
            <ol className="d-muted d-small gp-steps">
              <li>{t("Buka")}{" "}<strong>portal.gofoodmerchant.co.id</strong>{" "}{t("di browser (komputer), login dengan nomor & OTP seperti biasa (selesaikan reCAPTCHA bila ada).")}</li>
              <li>{t("Tekan")}{" "}<strong>F12</strong>{" "}{t("→ tab")}{" "}<strong>{t("Application")}</strong>{" "}{t("(Chrome) /")}{" "}<strong>{t("Storage")}</strong>{" "}{t("(Firefox) →")}{" "}<strong>{t("Cookies")}</strong>{" "}{t("→ pilih")}{" "}<strong>https://portal.gofoodmerchant.co.id</strong>.</li>
              <li>{t("Salin nilai")}{" "}<strong>access_token</strong> dan <strong>refresh_token</strong>{t(", lalu tempel ke kotak di atas (boleh format")}{" "}<code>{t("access_token=...; refresh_token=...")}</code>).</li>
              <li>{t("Setelah token tersalin, tutup tab GoBiz tanpa keluar (logout). Tab yang dibiarkan terbuka memperbarui sesinya sendiri, sehingga token yang tersimpan di sini menjadi tidak berlaku.")}</li>
              <li>{t("Selanjutnya sesi diperbarui otomatis oleh server sebelum kedaluwarsa dan minimal tiap 6 jam — tidak perlu login ulang.")}</li>
            </ol>
          )}
          {can && (
            <div className="d-row u-justify-end">
              <BusyButton type="button" busy={busy} onClick={connectSession} disabled={phone.trim().length < 8 || session.trim().length < 20}>{t("Hubungkan")}</BusyButton>
            </div>
          )}
        </div>
      ) : (
        <div className="gp-stack">
          <div className="d-field"><span>{t("Akun terhubung")}</span>
            <div className="d-row u-gap-2 u-items-center">
              <Badge tone={st.status === "active" ? "ok" : "warn"}>{st.status === "active" ? t("Aktif") : t("Perlu login ulang")}</Badge>
              <span>{st.outlet_name ?? t("Merchant GoPay")}{st.phone_masked ? t(" · {0}", st.phone_masked) : ""}</span>
            </div>
          </div>
          <div className="d-field">
            <span>{t("Sesi")}</span>
            <span className="d-hint">
              {st.refreshed_at_unix
                ? t("Diperbarui otomatis {0} · berlaku sampai {1}", relatif(st.refreshed_at_unix * 1000), waktu((st.expires_at_unix ?? 0) * 1000))
                : t("Diperbarui otomatis sebelum kedaluwarsa dan minimal tiap 6 jam.")}
            </span>
          </div>
          {st.status === "active" && st.last_error && (
            <Alert kind="warn">
              {st.last_error.startsWith("refresh_")
                ? t("GoBiz menolak perpanjangan sesi, tetapi sesi saat ini masih berfungsi sampai {0}. Konfirmasi otomatis tetap berjalan sampai saat itu — hubungkan ulang sebelumnya agar tidak terputus.", waktu((st.expires_at_unix ?? 0) * 1000))
                : t("Pembaruan sesi terakhir gagal ({0}). Sesi tetap aktif dan dicoba lagi otomatis.", gopayErrorText(st.last_error))}
            </Alert>
          )}
          <div className="d-field">
            <span>{t("QRIS statis toko")}</span>
            <span className="d-hint">
              {st.has_static_qris
                ? t("QRIS terbaca — konfirmasi otomatis siap.")
                : t("QRIS diambil otomatis dari gambar di kartu Pembayaran QRIS.")}
            </span>
          </div>
          {!st.has_static_qris && (
            <Alert kind="warn">
              {t("QRIS belum terbaca dari gambar. Unggah ulang/unggah QRIS yang jelas di kartu Pembayaran QRIS; konfirmasi manual tetap tersedia.")}
            </Alert>
          )}
          {can && (
            <div className="d-row" style={{ justifyContent: "flex-end", gap: "var(--sp-3)" }}>
              <button type="button" className="d-btn d-btn-ghost" onClick={disconnect}>{t("Putuskan akun")}</button>
              <BusyButton type="button" busy={busy} className="d-btn" onClick={check}>{t("Cek sekarang")}</BusyButton>
            </div>
          )}
        </div>
      )}
    </Card>
    </div>
  );
}
