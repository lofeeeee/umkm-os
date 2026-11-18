// Keranjang & token order di localStorage (per origin = per toko). Hanya kenyamanan pengguna:
// harga di sini untuk tampilan, server selalu menghitung ulang (AC-ORD-002). Storage bisa gagal
// (mode privat, diblokir) → semua akses dibungkus try/catch dan halaman tetap berfungsi.

export interface CartItem {
  variant_id: string;
  slug: string;
  name: string;
  variant_name: string;
  /** Harga tampilan (desimal string) — bukan sumber kebenaran */
  price: string;
  quantity: number;
  /** Foto produk (rujukan berkas publik) untuk kartu keranjang; opsional (keranjang lama tidak punya). */
  image?: string;
}

export interface SavedOrder { code: string; token: string; created_at: string }

const CART = "umkm_cart_v1";
const ORDERS = "umkm_orders_v1";
export const MAX_QTY = 999;

function read<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, v: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* storage tidak tersedia: abaikan */
  }
  window.dispatchEvent(new Event("umkm-cart"));
}

export function getCart(): CartItem[] {
  const c = read<CartItem[]>(CART, []);
  return Array.isArray(c) ? c.filter((x) => x && typeof x.variant_id === "string" && x.quantity > 0) : [];
}

export function addToCart(item: Omit<CartItem, "quantity">, quantity: number) {
  const c = getCart();
  const hit = c.find((x) => x.variant_id === item.variant_id);
  if (hit) hit.quantity = Math.min(MAX_QTY, hit.quantity + quantity);
  else c.push({ ...item, quantity: Math.min(MAX_QTY, quantity) });
  write(CART, c);
}

export function setQuantity(variant: string, quantity: number) {
  const c = getCart()
    .map((x) => (x.variant_id === variant ? { ...x, quantity: Math.min(MAX_QTY, quantity) } : x))
    .filter((x) => x.quantity > 0);
  write(CART, c);
}

export function clearCart() {
  write(CART, []);
}

export function cartCount(): number {
  return getCart().reduce((n, x) => n + x.quantity, 0);
}

/** Subtotal tampilan dalam sen (estimasi; total resmi dari server). */
export function displaySubtotalSen(c: CartItem[]): number {
  return c.reduce((n, x) => n + Math.round(Number(x.price) * 100) * x.quantity, 0);
}

export function savedOrders(): SavedOrder[] {
  const o = read<SavedOrder[]>(ORDERS, []);
  return Array.isArray(o) ? o.slice(0, 20) : [];
}

export function saveOrder(o: SavedOrder) {
  write(ORDERS, [o, ...savedOrders().filter((x) => x.code !== o.code)].slice(0, 20));
}
