"use client";

// Beranda: ringkasan 30 hari + checklist penyiapan toko + pesanan terbaru + tautan cepat.
// Checklist dihitung dari data nyata (bukan flag terpisah) supaya tidak bisa "bohong":
// profil bisnis, produk aktif, QRIS, website terbit, anggota tim.

import Link from "@/ui/link";
import { useEffect, useState } from "react";

import {
  ArrowRight, Buildings, ChatCircleDots, Check, CurrencyCircleDollar, Globe, Package, Receipt, Stack, Users, UsersThree,
} from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { ORDER_STATUS, angka, relatif, rupiah, rupiahShort, status } from "@/lib/labels";
import { Badge, Card, EmptyState, Stat } from "@/ui/page";
import { t as tx } from "@/i18n";
import { Tr } from "@/i18n/react";

interface Overview {
  revenue: string; orders: number; paid_orders: number;
  stock_alerts: number; customers_total: number; customers_new: number;
  tickets_created: number; ai_resolved: number;
}
interface OrderRow { id: string; code: string; status: string; customer_name: string | null; total: string; created_at: string }
interface Setup { profile: boolean; product: boolean; qris: boolean; website: boolean; team: boolean }
interface GopayStatus { connected: boolean; status: string | null }

export default function Home() {
  return <WithTenant>{(ctx) => <Dashboard ctx={ctx} />}</WithTenant>;
}

function greeting(): string {
  const h = new Date().getHours();
  if (h < 11) return tx("Selamat pagi");
  if (h < 15) return tx("Selamat siang");
  if (h < 18) return tx("Selamat sore");
  return tx("Selamat malam");
}

/** Petunjuk tata letak dari kunjungan terakhir (per toko) agar kerangka seukuran konten akhir —
 *  mencegah layout shift saat data tiba (UAT 15 Nov perf #2, CLS 0.395). Murni kosmetik. */
interface LayoutHint { orders?: number; allDone?: boolean }
const hintKey = (t: string) => `umkm-beranda:${t}`;
function readHint(t: string): LayoutHint {
  try { return JSON.parse(localStorage.getItem(hintKey(t)) ?? "{}") as LayoutHint; } catch { return {}; }
}
function writeHint(t: string, h: LayoutHint) {
  try { localStorage.setItem(hintKey(t), JSON.stringify({ ...readHint(t), ...h })); } catch { /* opsional */ }
}

const STAT_LABELS = [
  { get label() { return tx("Omzet"); }, icon: CurrencyCircleDollar }, { get label() { return tx("Pesanan"); }, icon: Receipt }, { get label() { return tx("Stok menipis"); }, icon: Stack },
  { get label() { return tx("Pelanggan"); }, icon: Users }, { get label() { return tx("Tiket"); }, icon: ChatCircleDots },
];

/** Hasil `null` = gagal/tak berizin; langkah tersebut dianggap belum diketahui (disembunyikan). */
const soft = <T,>(p: Promise<T>) => p.then((x) => x, () => null);

function Dashboard({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const q = `?tenant=${ctx.tenant.id}`;
  const [ov, setOv] = useState<Overview | null | undefined>(undefined);
  const [orders, setOrders] = useState<OrderRow[] | null | undefined>(undefined);
  const [setup, setSetup] = useState<Partial<Setup> | null>(null);
  const [gopay, setGopay] = useState<GopayStatus | null>(null);
  const [hint] = useState<LayoutHint>(() => (typeof window === "undefined" ? {} : readHint(ctx.tenant.id)));

  useEffect(() => {
    let live = true;
    const can = ctx.can;
    // P10: status GoPay untuk banner "hubungkan / sambungkan ulang" (hanya pemilik).
    (can("tenant.update") ? soft(api<GopayStatus>("GET", `${base}/gopay`)) : Promise.resolve(null))
      .then((x) => { if (live) setGopay(x); });
    (can("analytics.read") ? soft(api<Overview>("GET", `${base}/analytics/overview`)) : Promise.resolve(null))
      .then((x) => { if (live) setOv(x); });
    (can("orders.read") ? soft(api<{ items: OrderRow[] }>("GET", `${base}/orders?limit=5`)) : Promise.resolve(null))
      .then((x) => {
        if (!live) return;
        setOrders(x?.items ?? null);
        if (x) writeHint(ctx.tenant.id, { orders: x.items.length });
      });
    Promise.all([
      soft(api<{ profile: { description?: string | null; phone?: string | null; whatsapp?: string | null }; addresses: unknown[] }>("GET", `${base}/business/profile`)),
      can("products.read") ? soft(api<{ items: unknown[] }>("GET", `${base}/products?limit=1&status=active`)) : Promise.resolve(null),
      can("tenant.read") ? soft(api<{ qris: string | null; checkout_enabled: boolean }>("GET", `${base}/commerce/settings`)) : Promise.resolve(null),
      soft(api<{ pages: { published_version: number | null }[] }>("GET", `${base}/website`)),
      can("members.read") ? soft(api<unknown[]>("GET", `${base}/members`)) : Promise.resolve(null),
    ]).then(([biz, prod, shop, site, members]) => {
      if (!live) return;
      const s: Partial<Setup> = {};
      if (biz) s.profile = !!(biz.profile.description || biz.profile.phone || biz.profile.whatsapp) && biz.addresses.length > 0;
      if (prod) s.product = prod.items.length > 0;
      if (shop) s.qris = !!shop.qris && shop.checkout_enabled;
      s.website = !!site?.pages.some((p) => p.published_version !== null);
      if (members) s.team = members.length > 1;
      setSetup(s);
      const vals = Object.values(s);
      writeHint(ctx.tenant.id, { allDone: vals.length > 0 && vals.every(Boolean) });
    });
    return () => { live = false; };
  }, [base, ctx.can, ctx.tenant.id]);

  const steps: { key: keyof Setup; title: string; desc: string; href: string; icon: typeof Globe }[] = [
    { key: "profile", title: tx("Lengkapi profil bisnis"), desc: tx("Alamat, kontak, dan jam buka tampil di website."), href: `/business${q}`, icon: Buildings },
    { key: "product", title: tx("Tambahkan produk pertama"), desc: tx("Produk aktif tampil di katalog dan bisa dipesan."), href: `/products${q}`, icon: Package },
    { key: "qris", title: tx("Aktifkan pembayaran QRIS"), desc: tx("Unggah QRIS toko dan nyalakan pesanan online."), href: `/shop-settings${q}`, icon: CurrencyCircleDollar },
    { key: "website", title: tx("Terbitkan website"), desc: tx("Pilih templat, sesuaikan, lalu terbitkan."), href: `/website${q}`, icon: Globe },
    { key: "team", title: tx("Undang anggota tim"), desc: tx("Bagi tugas dengan peran dan izin yang tepat."), href: `/team${q}`, icon: UsersThree },
  ];
  const known = setup ? steps.filter((s) => setup[s.key] !== undefined) : [];
  const done = known.filter((s) => setup?.[s.key]).length;
  const allDone = setup === null ? !!hint.allDone : known.length > 0 && done === known.length;
  // Jumlah langkah yang akan tampil (untuk kerangka): profil & website selalu, sisanya per izin.
  const expectedSteps = steps.filter((s) =>
    s.key === "product" ? ctx.can("products.read") : s.key === "qris" ? ctx.can("tenant.read") : s.key === "team" ? ctx.can("members.read") : true);

  return (
    <>
      <div className="d-page-head">
        <div className="d-page-head-text">
          <h1>{tx("Beranda")}</h1>
          <p>{greeting()}{" "}<Tr s="— ini ringkasan {0} dalam 30 hari terakhir." a={[ctx.tenant.name]} /></p>
        </div>
        <div className="d-page-head-actions">
          {ctx.tenant.primary_hostname && (
            <a className="d-btn" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noreferrer">
              <Globe weight="bold" aria-hidden="true" />{" "}{tx("Lihat situs")}
            </a>
          )}
          <Link className="d-btn d-btn-primary" href={`/website${q}`}>{tx("Kelola website")}</Link>
        </div>
      </div>

      {/* P10: sinkron pembayaran GoPay — muncul tepat setelah login bila perlu tindakan. */}
      {gopay && ctx.can("tenant.update") && gopay.connected && gopay.status === "needs_relogin" && (
        <Alert kind="warn">
          <Tr s="Koneksi GoPay terputus — pembayaran QRIS tidak lagi dikonfirmasi otomatis sampai disambungkan ulang.{0}" a={[" "]} />
          <Link className="d-strong" href={`/shop-settings${q}#gopay`}>{tx("Sambungkan ulang")}</Link>
        </Alert>
      )}

      {ov === undefined && ctx.can("analytics.read") && (
        <section aria-busy="true" aria-label={tx("Memuat ringkasan")} className="d-stat-grid">
          {STAT_LABELS.map((x) => (
            <Stat key={x.label} icon={x.icon} label={x.label} value={<span className="d-skel d-skel-text" />} note={<span className="d-skel d-skel-text" style={{ width: "12ch" }} />} />
          ))}
        </section>
      )}
      {ov && (
        <section aria-label={tx("Ringkasan toko")} className="d-stat-grid d-stagger">
          <Stat icon={CurrencyCircleDollar} label={tx("Omzet")} value={rupiahShort(ov.revenue)} note={tx("{0} pesanan terbayar", angka(ov.paid_orders))} href={`/analytics${q}`} />
          <Stat icon={Receipt} label={tx("Pesanan")} value={angka(ov.orders)} note={tx("{0} belum dibayar", angka(ov.orders - ov.paid_orders))} href={`/orders${q}`} />
          <Stat icon={Stack} label={tx("Stok menipis")} value={angka(ov.stock_alerts)} note={tx("varian di bawah ambang")} href={`/inventory${q}`} warn={ov.stock_alerts > 0} />
          <Stat icon={Users} label={tx("Pelanggan")} value={angka(ov.customers_total)} note={tx("+{0} baru", angka(ov.customers_new))} href={`/customers${q}`} />
          <Stat icon={ChatCircleDots} label={tx("Tiket")} value={angka(ov.tickets_created)} note={tx("{0} diselesaikan AI", angka(ov.ai_resolved))} href={`/tickets${q}`} />
        </section>
      )}

      <div className="d-split">
        <div className="u-min-w-0">
          {orders !== null && (
            <Card
              flush
              title={tx("Pesanan terbaru")}
              actions={<Link className="d-btn d-btn-ghost d-btn-sm" href={`/orders${q}`}>{tx("Lihat semua")}{" "}<ArrowRight weight="bold" aria-hidden="true" /></Link>}
            >
              {orders === undefined ? (
                hint.orders === 0 ? (
                  <EmptyState icon={Receipt} title={tx("Memuat pesanan…")} />
                ) : (
                  <div className="d-table-wrap" aria-busy="true" aria-label={tx("Memuat pesanan")}>
                    <table className="d-table" aria-hidden="true">
                      <thead><tr><th>{tx("Pesanan")}</th><th className="d-hide-sm">{tx("Pelanggan")}</th><th>{tx("Status")}</th><th className="d-num">{tx("Total")}</th></tr></thead>
                      <tbody>
                        {Array.from({ length: hint.orders ?? 5 }, (_, i) => (
                          <tr key={i}>
                            <td><span className="d-skel d-skel-text" /><span className="d-sub"><span className="d-skel d-skel-text" style={{ width: "9ch" }} /></span></td>
                            <td className="d-hide-sm"><span className="d-skel d-skel-text" style={{ width: "10ch" }} /></td>
                            <td><span className="d-skel d-skel-text" /></td>
                            <td className="d-num"><span className="d-skel d-skel-text" /></td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )
              ) : orders.length === 0 ? (
                <EmptyState icon={Receipt} title={tx("Belum ada pesanan")}>
                  {tx("Pesanan dari website toko akan muncul di sini. Pastikan produk aktif dan QRIS sudah diatur.")}
                </EmptyState>
              ) : (
                <div className="d-table-wrap">
                  <table className="d-table">
                    <thead><tr><th>{tx("Pesanan")}</th><th className="d-hide-sm">{tx("Pelanggan")}</th><th>{tx("Status")}</th><th className="d-num">{tx("Total")}</th></tr></thead>
                    <tbody>
                      {orders.map((o) => {
                        const st = status(ORDER_STATUS, o.status);
                        return (
                          <tr key={o.id}>
                            <td className="d-nowrap"><Link className="d-strong" style={{ textDecoration: "none" }} href={`/orders${q}&open=${o.id}`}>{o.code}</Link><span className="d-sub">{relatif(o.created_at)}</span></td>
                            <td className="d-hide-sm">{o.customer_name ?? "—"}</td>
                            <td><Badge tone={st.tone}>{st.label}</Badge></td>
                            <td className="d-num">{rupiah(o.total)}</td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </Card>
          )}

          {gopay && ctx.can("tenant.update") && !gopay.connected && setup?.qris && (
            <Alert kind="info">
              <Tr s="Aktifkan konfirmasi pembayaran otomatis: pesanan yang dibayar lewat QRIS langsung berstatus Diproses tanpa cek manual. Cukup hubungkan sekali.{0}" a={[" "]} />
              <Link className="d-strong" href={`/shop-settings${q}#gopay`}>{tx("Hubungkan GoPay")}</Link>
            </Alert>
          )}

          <h2 className="d-section-title">{tx("Pintasan")}</h2>
          <div className="d-quick">
            {ctx.can("products.create") && (
              <Link className="d-quick-item" href={`/products${q}&new=1`}>
                <Package aria-hidden="true" weight="bold" /><span>{tx("Tambah produk")}</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            {ctx.can("conversations.read") && (
              <Link className="d-quick-item" href={`/conversations${q}`}>
                <ChatCircleDots aria-hidden="true" weight="bold" /><span>{tx("Balas percakapan")}</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            {ctx.can("inventory.read") && (
              <Link className="d-quick-item" href={`/inventory${q}`}>
                <Stack aria-hidden="true" weight="bold" /><span>{tx("Catat stok masuk")}</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            <Link className="d-quick-item" href={`/business${q}`}>
              <Buildings aria-hidden="true" weight="bold" /><span>{tx("Ubah jam buka")}</span><ArrowRight aria-hidden="true" weight="bold" />
            </Link>
          </div>
        </div>

        {!allDone && (
          <Card
            title={tx("Siapkan toko Anda")}
            description={setup ? tx("{0} dari {1} langkah selesai", done, known.length) : tx("Memeriksa…")}
            label={tx("Penyiapan toko")}
            className="d-split-first"
          >
            <div className="d-progress u-mb-3" aria-hidden="true">
              <span style={{ transform: `scaleX(${known.length ? done / known.length : 0})` }} />
            </div>
            {!setup ? (
              <ol className="d-steps" aria-hidden="true">
                {expectedSteps.map((s) => (
                  <li key={s.key} data-done="false">
                    <a tabIndex={-1}>
                      <span className="d-step-dot"><Check weight="bold" /></span>
                      <span className="d-step-text">
                        <strong>{s.title}</strong>
                        <span><span className="d-skel-mask">{s.desc}</span></span>
                      </span>
                      <ArrowRight weight="bold" />
                    </a>
                  </li>
                ))}
              </ol>
            ) : (
              <ol className="d-steps">
                {known.map((s) => (
                  <li key={s.key} data-done={setup[s.key] ? "true" : "false"}>
                    <Link href={s.href}>
                      <span className="d-step-dot" aria-hidden="true"><Check weight="bold" /></span>
                      <span className="d-step-text">
                        <strong>{s.title}</strong>
                        <span>{setup[s.key] ? tx("Selesai") : s.desc}</span>
                      </span>
                      <ArrowRight weight="bold" aria-hidden="true" />
                    </Link>
                  </li>
                ))}
              </ol>
            )}
          </Card>
        )}
      </div>

      <footer className="d-page-legal">
        <span><Tr s="© {0} UMKM Business OS" a={[new Date().getFullYear()]} /></span>
        <nav aria-label={tx("Legal")}>
          <a href="/legal/syarat" target="_blank" rel="noopener">{tx("Syarat layanan")}</a>
          <span aria-hidden="true">·</span>
          <a href="/legal/privasi" target="_blank" rel="noopener">{tx("Kebijakan privasi")}</a>
        </nav>
      </footer>
    </>
  );
}
