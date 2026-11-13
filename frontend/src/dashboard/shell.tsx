"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState, type ReactNode } from "react";

import { Bell } from "./bell";
import { api, ApiError, isMfaRequired, loadSession, setCsrf, tenantParam, type Session, type TenantView } from "@/lib/api";

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
        {can("tickets.read") && <Link href={`/tickets${q}`}>Tiket</Link>}
        {can("conversations.read") && <Link href={`/conversations${q}`}>Percakapan</Link>}
        {can("analytics.read") && <Link href={`/analytics${q}`}>Analitik</Link>}
        {can("approvals.read") && <Link href={`/approvals${q}`}>Persetujuan</Link>}
        {can("members.read") && <Link href={`/team${q}`}>Tim</Link>}
        {can("audit.read") && <Link href={`/audit${q}`}>Audit</Link>}
        {can("ai_config.read") && <Link href={`/ai${q}`}>Asisten AI</Link>}
        <Link href={`/security${q}`}>Keamanan</Link>
        {tenant && <Bell tenantId={tenant.id} />}
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
      if (session.mfa_enrollment_required) {
        router.replace("/security?reason=mfa");
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
        // AUTH-012: toko mewajibkan MFA untuk peran ini dan sesi belum lolos MFA.
        if (isMfaRequired(e)) {
          router.replace(`/security?tenant=${id}&reason=mfa`);
          return;
        }
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
