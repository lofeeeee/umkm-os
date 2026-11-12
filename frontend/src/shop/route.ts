// Routing situs toko dari path (shell statis tunggal, ADR-0014).

import { pageSlug } from "@/lib/slug";

export type Route =
  | { kind: "page"; slug: string }
  | { kind: "product"; slug: string }
  | { kind: "cart" | "orders" | "account" | "verify" | "privacy" | "terms" }
  | { kind: "missing" };

export function route(pathname: string): Route {
  const p = pathname.replace(/\/+$/, "");
  const product = /^\/produk\/([a-z0-9]+(?:-[a-z0-9]+)*)$/.exec(p);
  if (product) return { kind: "product", slug: product[1]! };
  switch (p) {
    case "/keranjang": return { kind: "cart" };
    case "/pesanan": return { kind: "orders" };
    case "/akun": return { kind: "account" };
    case "/akun/verifikasi": return { kind: "verify" };
    case "/kebijakan-privasi": return { kind: "privacy" };
    case "/syarat-transaksi": return { kind: "terms" };
  }
  const slug = pageSlug(pathname);
  return slug ? { kind: "page", slug } : { kind: "missing" };
}

