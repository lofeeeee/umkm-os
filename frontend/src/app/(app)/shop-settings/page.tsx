"use client";

// Pengaturan toko online (P3): QRIS statis milik toko (ADR-0004), ongkir, approval, SoD.
// Hanya Owner (tenant.update) yang dapat mengubah; pergantian QRIS tercatat di audit log.

import { useEffect, useState } from "react";

import { useUploads } from "@/dashboard/assets";
import { Assets, ImagePicker } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";
import { api, describe } from "@/lib/api";

interface Settings {
  qris: string | null; qris_receiver_name: string | null; checkout_enabled: boolean; delivery_fee: string;
  order_approval_required: boolean; reservation_ttl_minutes: number; separation_of_duties: boolean;
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
  useEffect(() => { api<Settings>("GET", url).then(setS, (e) => setErr(describe(e))); }, [url]);
  if (!s) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  async function save() {
    setErr(null);
    setBusy(true);
    try { setS(await api<Settings>("PUT", url, { json: s })); toast.ok("Tersimpan."); } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  const ready = !!s.qris && s.checkout_enabled;
  return (
    <Assets.Provider value={assets}>
      <PageHeader
        title="Pembayaran & pengiriman"
        description="Cara pembeli membayar dan menerima pesanan dari website toko."
        actions={<Badge tone={ready ? "ok" : "warn"}>{ready ? "Pesanan online aktif" : "Pesanan online belum aktif"}</Badge>}
      />
      {!can && <Alert kind="info">Anda hanya dapat melihat. Hanya pemilik toko yang dapat mengubah pengaturan ini.</Alert>}
      <Alert kind="error">{err}</Alert>
      <form onSubmit={(e) => { e.preventDefault(); void save(); }}>
        <Card title="Pembayaran QRIS" description="Unggah QRIS merchant milik toko. Pembeli membayar langsung ke rekening toko; Anda mengonfirmasi setiap pembayaran di menu Pesanan. Platform tidak memegang dana.">
          <div className="d-grid">
            <ImagePicker label="Gambar QRIS" value={s.qris ?? undefined} onChange={(v) => setS({ ...s, qris: v ?? null })} />
            <label className="d-field"><span>Nama penerima (tampil di halaman bayar)</span>
              <input disabled={!can} maxLength={80} placeholder="mis. Kopi Senja" value={s.qris_receiver_name ?? ""} onChange={(e) => setS({ ...s, qris_receiver_name: e.target.value || null })} />
            </label>
          </div>
          <label className="d-switch">
            <input type="checkbox" disabled={!can} checked={s.checkout_enabled} onChange={(e) => setS({ ...s, checkout_enabled: e.target.checked })} />
            <span><strong>Terima pesanan online</strong><span className="d-muted">Tombol beli dan keranjang muncul di website.</span></span>
          </label>
        </Card>
        <GopayCard tenantId={ctx.tenant.id} can={can} />
        <Card title="Pengiriman & pesanan" description="Pilihan ambil di toko / diantar diatur di Profil bisnis.">
          <div className="d-grid">
            <label className="d-field"><span>Ongkir datar untuk pengiriman</span>
              <div className="d-input-prefix"><span aria-hidden="true">Rp</span><input aria-label="Ongkir datar untuk pengiriman (Rp)" disabled={!can} inputMode="decimal" value={s.delivery_fee} onChange={(e) => setS({ ...s, delivery_fee: e.target.value })} /></div>
            </label>
            <label className="d-field"><span>Batas waktu pembayaran</span>
              <div className="d-input-suffix"><input aria-label="Batas waktu pembayaran (menit, 15–1440)" disabled={!can} type="number" min={15} max={1440} value={s.reservation_ttl_minutes} onChange={(e) => setS({ ...s, reservation_ttl_minutes: Number(e.target.value) })} /><em aria-hidden="true">menit</em></div>
              <span className="d-hint">15–1440 menit. Stok ditahan selama waktu ini.</span>
            </label>
          </div>
          <label className="d-switch"><input type="checkbox" disabled={!can} checked={s.order_approval_required} onChange={(e) => setS({ ...s, order_approval_required: e.target.checked })} />
            <span><strong>Pesanan yang sudah dibayar perlu disetujui sebelum diproses</strong><span className="d-muted">Cocok bila stok/produksi perlu dicek manual.</span></span></label>
          <label className="d-switch"><input type="checkbox" disabled={!can} checked={s.separation_of_duties} onChange={(e) => setS({ ...s, separation_of_duties: e.target.checked })} />
            <span><strong>Penyetuju harus berbeda dari staf yang mengonfirmasi pembayaran</strong><span className="d-muted">Mencegah satu orang mengonfirmasi dan menyetujui sendiri.</span></span></label>
        </Card>
        {can && (
          <div className="d-savebar">
            <span className="d-muted d-small">Perubahan QRIS tercatat di log aktivitas.</span>
            <BusyButton busy={busy}>Simpan</BusyButton>
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
}

function GopayCard({ tenantId, can }: { tenantId: string; can: boolean }) {
  const base = `/api/v1/tenants/${tenantId}/gopay`;
  const [st, setSt] = useState<GopayStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // Alur OTP: phase 'phone' -> 'otp'.
  const [phone, setPhone] = useState("");
  const [otp, setOtp] = useState("");
  const [otpToken, setOtpToken] = useState<string | null>(null);
  const [qris, setQris] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => { api<GopayStatus>("GET", base).then(setSt, (e) => setErr(describe(e))); }, [base]);
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
  const requestOtp = () => run(async () => {
    const r = await api<{ otp_token: string }>("POST", `${base}/connect/request-otp`, { json: { phone } });
    setOtpToken(r.otp_token); toast.ok("Kode OTP dikirim via SMS/WhatsApp.");
  });
  const verifyOtp = () => run(async () => {
    setSt(await api<GopayStatus>("POST", `${base}/connect/verify`, { json: { phone, otp, otp_token: otpToken } }));
    setOtp(""); setOtpToken(null); toast.ok("Akun GoPay terhubung.");
  });
  const saveQris = () => run(async () => {
    setSt(await api<GopayStatus>("PUT", `${base}/static-qris`, { json: { qris } }));
    setQris(""); toast.ok("QRIS statis tersimpan.");
  });
  const disconnect = () => run(async () => {
    if (!confirm("Putuskan akun GoPay? Pesanan tidak lagi dikonfirmasi otomatis.")) return;
    setSt(await api<GopayStatus>("DELETE", base)); toast.ok("Akun GoPay diputus.");
  });

  if (!st) return null;
  return (
    <div id="gopay">
    <Card
      title="Konfirmasi otomatis (GoPay Merchant)"
      description="Opsional. Hubungkan akun GoPay/GoBiz Merchant toko agar pembayaran QRIS dikonfirmasi otomatis — pesanan langsung berstatus Diproses tanpa Anda cek manual. Gratis untuk usaha mikro. Konfirmasi manual tetap tersedia sebagai cadangan."
    >
      <Alert kind="error">{err}</Alert>
      {!st.connected ? (
        <div className="d-grid">
          {!otpToken ? (
            <>
              <label className="d-field"><span>Nomor HP GoBiz/GoPay Merchant</span>
                <input disabled={!can} inputMode="tel" placeholder="mis. 0812xxxxxxx" value={phone} onChange={(e) => setPhone(e.target.value)} />
                <span className="d-hint">Nomor yang terdaftar di aplikasi GoPay Merchant / GoBiz.</span>
              </label>
              {can && <div className="d-savebar"><BusyButton type="button" busy={busy} onClick={requestOtp} disabled={phone.trim().length < 8}>Kirim OTP</BusyButton></div>}
            </>
          ) : (
            <>
              <label className="d-field"><span>Kode OTP</span>
                <input disabled={!can} inputMode="numeric" autoComplete="one-time-code" placeholder="Masukkan kode dari SMS/WhatsApp" value={otp} onChange={(e) => setOtp(e.target.value)} />
              </label>
              {can && <div className="d-savebar">
                <button type="button" className="d-btn d-btn-ghost" onClick={() => { setOtpToken(null); setOtp(""); }}>Ganti nomor</button>
                <BusyButton type="button" busy={busy} onClick={verifyOtp} disabled={otp.trim().length < 4}>Verifikasi</BusyButton>
              </div>}
            </>
          )}
        </div>
      ) : (
        <>
          <div className="d-grid">
            <div className="d-field"><span>Akun terhubung</span>
              <div className="d-row" style={{ gap: 8, alignItems: "center" }}>
                <Badge tone={st.status === "active" ? "ok" : "warn"}>{st.status === "active" ? "Aktif" : "Perlu login ulang"}</Badge>
                <span>{st.outlet_name ?? "Merchant GoPay"}{st.phone_masked ? ` · ${st.phone_masked}` : ""}</span>
              </div>
            </div>
            <label className="d-field"><span>QRIS statis toko (teks)</span>
              <textarea disabled={!can} rows={3} placeholder="Tempel teks QRIS (mulai 00020101...). Dapat dari penyedia/aplikasi QRIS toko." value={qris} onChange={(e) => setQris(e.target.value)} />
              <span className="d-hint">
                {st.has_static_qris ? "QRIS statis sudah diatur. Tempel baru untuk mengganti." : "Wajib: dipakai menyusun QR bernominal unik per pesanan."}
              </span>
            </label>
          </div>
          {!st.has_static_qris && <Alert kind="warn">Konfirmasi otomatis belum aktif sampai QRIS statis diisi.</Alert>}
          {can && <div className="d-savebar">
            <button type="button" className="d-btn d-btn-ghost" onClick={disconnect}>Putuskan akun</button>
            <BusyButton type="button" busy={busy} onClick={saveQris} disabled={qris.trim().length < 20}>Simpan QRIS</BusyButton>
          </div>}
        </>
      )}
    </Card>
    </div>
  );
}
