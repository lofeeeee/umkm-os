"use client";

// Beranda: ringkasan 30 hari + checklist penyiapan toko + pesanan terbaru + tautan cepat.
// Checklist dihitung dari data nyata (bukan flag terpisah) supaya tidak bisa "bohong":
// profil bisnis, produk aktif, QRIS, website terbit, anggota tim.

import Link from "next/link";
import { useEffect, useState } from "react";

import {
  ArrowRight, Buildings, ChatCircleDots, Check, CurrencyCircleDollar, Globe, Package, Receipt, Stack, Users, UsersThree,
} from "@phosphor-icons/react/ssr";

import { WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api } from "@/lib/api";
import { ORDER_STATUS, angka, relatif, rupiah, rupiahShort, status } from "@/lib/labels";
import { Badge, Card, EmptyState, Skeleton, Stat } from "@/ui/page";

interface Overview {
  revenue: string; orders: number; paid_orders: number;
  stock_alerts: number; customers_total: number; customers_new: number;
  tickets_created: number; ai_resolved: number;
}
interface OrderRow { id: string; code: string; status: string; customer_name: string | null; total: string; created_at: string }
interface Setup { profile: boolean; product: boolean; qris: boolean; website: boolean; team: boolean }

export default function Home() {
  return <WithTenant>{(ctx) => <Dashboard ctx={ctx} />}</WithTenant>;
}

function greeting(): string {
  const h = new Date().getHours();
  if (h < 11) return "Selamat pagi";
  if (h < 15) return "Selamat siang";
  if (h < 18) return "Selamat sore";
  return "Selamat malam";
}

/** Hasil `null` = gagal/tak berizin; langkah tersebut dianggap belum diketahui (disembunyikan). */
const soft = <T,>(p: Promise<T>) => p.then((x) => x, () => null);

function Dashboard({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const q = `?tenant=${ctx.tenant.id}`;
  const [ov, setOv] = useState<Overview | null | undefined>(undefined);
  const [orders, setOrders] = useState<OrderRow[] | null | undefined>(undefined);
  const [setup, setSetup] = useState<Partial<Setup> | null>(null);

  useEffect(() => {
    let live = true;
    const can = ctx.can;
    (can("analytics.read") ? soft(api<Overview>("GET", `${base}/analytics/overview`)) : Promise.resolve(null))
      .then((x) => { if (live) setOv(x); });
    (can("orders.read") ? soft(api<{ items: OrderRow[] }>("GET", `${base}/orders?limit=5`)) : Promise.resolve(null))
      .then((x) => { if (live) setOrders(x?.items ?? null); });
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
    });
    return () => { live = false; };
  }, [base, ctx.can]);

  const steps: { key: keyof Setup; title: string; desc: string; href: string; icon: typeof Globe }[] = [
    { key: "profile", title: "Lengkapi profil bisnis", desc: "Alamat, kontak, dan jam buka tampil di website.", href: `/business${q}`, icon: Buildings },
    { key: "product", title: "Tambahkan produk pertama", desc: "Produk aktif tampil di katalog dan bisa dipesan.", href: `/products${q}`, icon: Package },
    { key: "qris", title: "Aktifkan pembayaran QRIS", desc: "Unggah QRIS toko dan nyalakan pesanan online.", href: `/shop-settings${q}`, icon: CurrencyCircleDollar },
    { key: "website", title: "Terbitkan website", desc: "Pilih template, sesuaikan, lalu terbitkan.", href: `/website${q}`, icon: Globe },
    { key: "team", title: "Undang anggota tim", desc: "Bagi tugas dengan peran dan izin yang tepat.", href: `/team${q}`, icon: UsersThree },
  ];
  const known = setup ? steps.filter((s) => setup[s.key] !== undefined) : [];
  const done = known.filter((s) => setup?.[s.key]).length;
  const allDone = setup !== null && known.length > 0 && done === known.length;

  return (
    <>
      <div className="d-page-head">
        <div className="d-page-head-text">
          <h1>Beranda</h1>
          <p>{greeting()} — ini ringkasan {ctx.tenant.name} dalam 30 hari terakhir.</p>
        </div>
        <div className="d-page-head-actions">
          {ctx.tenant.primary_hostname && (
            <a className="d-btn" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noreferrer">
              <Globe weight="bold" aria-hidden="true" /> Lihat situs
            </a>
          )}
          <Link className="d-btn d-btn-primary" href={`/website${q}`}>Kelola website</Link>
        </div>
      </div>

      {ov === undefined && <span className="d-skel d-skel-block" style={{ height: 96, marginBottom: 24 }} aria-hidden="true" />}
      {ov && (
        <section aria-label="Ringkasan toko" className="d-stat-grid d-stagger">
          <Stat icon={CurrencyCircleDollar} label="Omzet" value={rupiahShort(ov.revenue)} note={`${angka(ov.paid_orders)} pesanan terbayar`} href={`/analytics${q}`} />
          <Stat icon={Receipt} label="Pesanan" value={angka(ov.orders)} note={`${angka(ov.orders - ov.paid_orders)} belum dibayar`} href={`/orders${q}`} />
          <Stat icon={Stack} label="Stok menipis" value={angka(ov.stock_alerts)} note="varian di bawah ambang" href={`/inventory${q}`} warn={ov.stock_alerts > 0} />
          <Stat icon={Users} label="Pelanggan" value={angka(ov.customers_total)} note={`+${angka(ov.customers_new)} baru`} href={`/customers${q}`} />
          <Stat icon={ChatCircleDots} label="Tiket" value={angka(ov.tickets_created)} note={`${angka(ov.ai_resolved)} diselesaikan AI`} href={`/tickets${q}`} />
        </section>
      )}

      <div className="d-split">
        <div style={{ minWidth: 0 }}>
          {orders !== null && (
            <Card
              flush
              title="Pesanan terbaru"
              actions={<Link className="d-btn d-btn-ghost d-btn-sm" href={`/orders${q}`}>Lihat semua <ArrowRight weight="bold" aria-hidden="true" /></Link>}
            >
              {orders === undefined ? (
                <div style={{ padding: 20 }}><Skeleton lines={4} /></div>
              ) : orders.length === 0 ? (
                <EmptyState icon={Receipt} title="Belum ada pesanan">
                  Pesanan dari website toko akan muncul di sini. Pastikan produk aktif dan QRIS sudah diatur.
                </EmptyState>
              ) : (
                <div className="d-table-wrap">
                  <table className="d-table">
                    <thead><tr><th>Pesanan</th><th className="d-hide-sm">Pelanggan</th><th>Status</th><th className="d-num">Total</th></tr></thead>
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

          <h2 className="d-section-title">Pintasan</h2>
          <div className="d-quick">
            {ctx.can("products.create") && (
              <Link className="d-quick-item" href={`/products${q}&new=1`}>
                <Package aria-hidden="true" weight="bold" /><span>Tambah produk</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            {ctx.can("conversations.read") && (
              <Link className="d-quick-item" href={`/conversations${q}`}>
                <ChatCircleDots aria-hidden="true" weight="bold" /><span>Balas percakapan</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            {ctx.can("inventory.read") && (
              <Link className="d-quick-item" href={`/inventory${q}`}>
                <Stack aria-hidden="true" weight="bold" /><span>Catat stok masuk</span><ArrowRight aria-hidden="true" weight="bold" />
              </Link>
            )}
            <Link className="d-quick-item" href={`/business${q}`}>
              <Buildings aria-hidden="true" weight="bold" /><span>Ubah jam buka</span><ArrowRight aria-hidden="true" weight="bold" />
            </Link>
          </div>
        </div>

        {!allDone && (
          <Card
            title="Siapkan toko Anda"
            description={setup ? `${done} dari ${known.length} langkah selesai` : "Memeriksa…"}
            label="Penyiapan toko"
            className="d-split-first"
          >
            <div className="d-progress" aria-hidden="true" style={{ marginBottom: 12 }}>
              <span style={{ transform: `scaleX(${known.length ? done / known.length : 0})` }} />
            </div>
            {!setup ? <Skeleton lines={5} /> : (
              <ol className="d-steps">
                {known.map((s) => (
                  <li key={s.key} data-done={setup[s.key] ? "true" : "false"}>
                    <Link href={s.href}>
                      <span className="d-step-dot" aria-hidden="true"><Check weight="bold" /></span>
                      <span className="d-step-text">
                        <strong>{s.title}</strong>
                        <span>{setup[s.key] ? "Selesai" : s.desc}</span>
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
    </>
  );
}
