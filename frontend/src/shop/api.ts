// Klien API situs toko (ADR-0014): same-origin ke /_api/public/* di host toko. Tenant dari Host.
// Akun customer (ADR-0005): cookie host-only __Host-umkm_cust + header CSRF (disimpan di memori saja).

import { API_ORIGIN, ApiError } from "@/lib/api";

let csrf: string | null = null;

export function setCustomerCsrf(t: string | null) {
  csrf = t;
}

type Body = { json: unknown } | { bytes: Blob } | undefined;

export async function shopApi<T>(method: string, path: string, body?: Body, headers: Record<string, string> = {}): Promise<T> {
  const h: Record<string, string> = { ...headers };
  let payload: BodyInit | undefined;
  if (body && "json" in body) {
    h["content-type"] = "application/json";
    payload = JSON.stringify(body.json);
  } else if (body && "bytes" in body) {
    h["content-type"] = body.bytes.type || "application/octet-stream";
    payload = body.bytes;
  }
  if (method !== "GET" && csrf) h["x-csrf-token"] = csrf;
  const res = await fetch(`/_api/public${path}`, { method, headers: h, body: payload, credentials: "same-origin" });
  if (res.status === 204) return undefined as T;
  const data = await res.json().catch(() => null);
  if (!res.ok || !data?.success) {
    const e = data?.error ?? {};
    throw new ApiError(res.status, e.code ?? "UNKNOWN", e.message ?? `HTTP ${res.status}`, e.details ?? null);
  }
  return data.data as T;
}

/** Signed path `/files/...` dari API → URL absolut (file dilayani host API). */
export function fileUrl(p: string): string {
  return `${API_ORIGIN}${p}`;
}

export interface PublicVariant { id: string; name: string; option_values: string[]; price: string; promo_price: string | null }
export interface PublicProduct {
  id: string; name: string; slug: string; description: string | null; category_id: string | null;
  available: boolean; options: { name: string; values: string[] }[]; variants: PublicVariant[]; images: string[];
}
export interface CheckoutConfig {
  checkout_enabled: boolean; pickup_available: boolean; delivery_available: boolean;
  delivery_fee: string; payment_method: string; privacy_version: string; terms_version: string;
}
export interface OrderItem { product_name: string; variant_name: string; sku: string; unit_price: string; quantity: number; line_total: string }
export interface PublicOrder {
  code: string; status: string; fulfillment: string; name: string | null; delivery_address: string | null;
  items: OrderItem[]; subtotal: string; delivery_fee: string; total: string; created_at: string; expires_at: string;
  payment: { method: string; status: string; amount: string; qris_url: string | null; receiver_name: string | null; rejection_reason: string | null } | null;
}
export interface CustomerSession { account_id: string; email: string; email_verified: boolean; csrf_token: string }
export interface OrderSummary { id: string; code: string; status: string; fulfillment: string; total: string; payment_status: string | null; created_at: string }

/** Pulihkan sesi customer (token CSRF) bila ada cookie. */
export async function loadCustomer(): Promise<CustomerSession | null> {
  try {
    const s = await shopApi<CustomerSession>("GET", "/customer/session");
    setCustomerCsrf(s.csrf_token);
    return s;
  } catch {
    setCustomerCsrf(null);
    return null;
  }
}

/** `"15000.50"` → `"Rp15.000,50"`. */
export function rupiah(v: string | null | undefined): string {
  if (!v) return "";
  const [i, f = "00"] = v.split(".");
  const int = (i ?? "0").replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `Rp${int}${f === "00" ? "" : `,${f}`}`;
}

export const STATUS_TEXT: Record<string, string> = {
  PENDING: "Menunggu",
  AWAITING_PAYMENT: "Menunggu pembayaran",
  PAID: "Dibayar",
  APPROVAL_PENDING: "Menunggu persetujuan toko",
  APPROVED: "Disetujui",
  PROCESSING: "Diproses",
  READY_FOR_PICKUP: "Siap diambil",
  SHIPPED: "Dikirim",
  COMPLETED: "Selesai",
  REJECTED: "Ditolak toko",
  CANCELLED: "Dibatalkan",
  EXPIRED: "Kedaluwarsa",
  REFUNDED: "Dana dikembalikan",
};
