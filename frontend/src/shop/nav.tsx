"use client";

// Tautan toko di navigasi situs (Keranjang · Pesanan · Akun). SATU komponen untuk halaman website,
// halaman toko, dan kanvas Studio — navigasi tidak berubah bentuk saat pembeli berpindah halaman.

import { useSyncExternalStore } from "react";

import { t as tx } from "@/i18n";

import { cartCount } from "./cart";

const noopSub = () => () => {};

function useCartCount(preview: boolean): number {
  return useSyncExternalStore(
    preview ? noopSub : (f) => {
      window.addEventListener("umkm-cart", f);
      window.addEventListener("storage", f);
      return () => { window.removeEventListener("umkm-cart", f); window.removeEventListener("storage", f); };
    },
    preview ? () => 2 : cartCount,
    () => 0,
  );
}

function usePath(preview: boolean): string | null {
  return useSyncExternalStore(noopSub, () => (preview ? null : window.location.pathname.replace(/\/+$/, "")), () => null);
}

/** `preview` = kanvas/pratinjau Studio: tampil sama persis, tapi tidak bisa diklik & tanpa localStorage. */
export function ShopNavActions({ preview = false }: { preview?: boolean }) {
  const n = useCartCount(preview);
  const path = usePath(preview);
  const items = [
    { href: "/keranjang", label: tx("Keranjang"), badge: n, aria: n > 0 ? tx("Keranjang, {0} barang", n) : undefined },
    { href: "/pesanan", label: tx("Pesanan") },
    { href: "/akun", label: tx("Akun") },
  ];
  return (
    <nav className="shop-actions" aria-label={tx("Navigasi toko")}>
      {items.map((it) => {
        const body = <>{it.label}{!!it.badge && <span className="shop-badge" aria-hidden="true">{it.badge}</span>}</>;
        if (preview) return <span key={it.href} className="shop-action" aria-label={it.aria}>{body}</span>;
        const current = path === it.href || (it.href === "/akun" && path?.startsWith("/akun/"));
        return (
          <a key={it.href} className="shop-action" href={it.href} aria-label={it.aria} aria-current={current ? "page" : undefined}>{body}</a>
        );
      })}
    </nav>
  );
}
