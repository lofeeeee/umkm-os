"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState, type ReactNode } from "react";

import { api, ApiError, loadSession, setCsrf, tenantParam, type Session, type TenantView } from "@/lib/api";

export function TopBar({ tenant }: { tenant?: TenantView | null }) {
  const can = (p: string) => !!tenant?.my_permissions.includes(p);
  const router = useRouter();
  const q = tenant ? `?tenant=${tenant.id}` : "";
  async function logout() {
    try {
      await api("POST", "/api/v1/auth/logout");
    } finally {
      setCsrf(null);
      router.replace("/login");
    }
  }
  return (
    <header className="d-top">
      <strong>UMKM OS{tenant ? ` · ${tenant.name}` : ""}</strong>
      <nav aria-label="Navigasi dashboard">
        <Link href="/">Toko saya</Link>
        {tenant && <Link href={`/website${q}`}>Website</Link>}
        {tenant && <Link href={`/business${q}`}>Profil bisnis</Link>}
        {can("products.read") && <Link href={`/products${q}`}>Produk</Link>}
        {can("inventory.read") && <Link href={`/inventory${q}`}>Stok</Link>}
        {can("orders.read") && <Link href={`/orders${q}`}>Pesanan</Link>}
        {can("customers.read") && <Link href={`/customers${q}`}>Pelanggan</Link>}
        {can("tenant.read") && <Link href={`/shop-settings${q}`}>Toko online</Link>}
        <button type="button" className="d-btn d-btn-sm" onClick={logout}>Keluar</button>
      </nav>
    </header>
  );
}

export interface TenantCtx {
  session: Session;
  tenant: TenantView;
  can: (permission: string) => boolean;
}

/** Halaman yang butuh sesi + `?tenant=`. Belum login -> /login. */
export function WithTenant({ children }: { children: (ctx: TenantCtx) => ReactNode }) {
  const router = useRouter();
  const [ctx, setCtx] = useState<TenantCtx | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    (async () => {
      const session = await loadSession();
      if (!session) {
        router.replace("/login");
        return;
      }
      const id = tenantParam();
      if (!id) {
        router.replace("/");
        return;
      }
      try {
        const tenant = await api<TenantView>("GET", `/api/v1/tenants/${id}`);
        setCtx({ session, tenant, can: (p) => tenant.my_permissions.includes(p) });
      } catch (e) {
        setError(e instanceof ApiError && e.status === 404 ? "Toko tidak ditemukan." : "Gagal memuat toko.");
      }
    })().catch(() => setError("Tidak dapat menghubungi server."));
  }, [router]);
  if (error) {
    return (
      <>
        <TopBar />
        <main className="d-main"><div className="d-alert d-alert-error" role="alert">{error}</div></main>
      </>
    );
  }
  if (!ctx) return <main className="d-main d-muted" aria-busy="true">Memuat…</main>;
  return (
    <>
      <TopBar tenant={ctx.tenant} />
      <main className="d-main">{children(ctx)}</main>
    </>
  );
}

export function Alert({ kind, children }: { kind: "error" | "ok"; children: ReactNode }) {
  if (!children) return null;
  return <div className={`d-alert d-alert-${kind}`} role={kind === "error" ? "alert" : "status"}>{children}</div>;
}
