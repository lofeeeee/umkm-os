"use client";
/* eslint-disable @next/next/no-html-link-for-pages, @next/next/no-location-assign-relative-destination --
   situs toko: SEMUA path dilayani shell site.html oleh Caddy (ADR-0014). next/link akan melakukan
   navigasi klien ke rute Next (mis. "/" = dashboard), jadi tautan HTML biasa yang benar. */

// Halaman toko (P3): produk, keranjang + checkout, status pesanan + pembayaran QRIS statis
// (ADR-0004), akun customer opsional (ADR-0005). Semua teks dirender sebagai teks (React).

import { createContext, useCallback, useContext, useEffect, useMemo, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";

import { prepareImage } from "@/lib/image";
import { ApiError } from "@/lib/api";
import { PRIVACY, TERMS } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";
import { inlineProduct } from "@/renderer/inline";
import { onImgError, sized, srcSet } from "@/renderer/safe";

import {
  STATUS_TEXT, fileUrl, loadCustomer, rupiah, setCustomerCsrf, shopApi,
  type CheckoutConfig, type CustomerSession, type OrderSummary, type PublicOrder, type PublicProduct,
} from "./api";
import { addToCart, cartCount, clearCart, displaySubtotalSen, getCart, saveOrder, savedOrders, setQuantity, type CartItem } from "./cart";
import { t as tx, tmap, intlLocale } from "@/i18n";
import { Tr } from "@/i18n/react";
import { ShopFrame, useShopTexts } from "./frame";
import { serverMessage } from "@/i18n/server";

const ERR: Record<string, string> = tmap({
  required: "wajib diisi",
  invalid_email: "email tidak valid",
  invalid_phone: "nomor telepon tidak valid",
  email_or_phone_required: "isi email atau nomor telepon",
  invalid_quantity: "jumlah tidak valid",
  product_unavailable: "produk sudah tidak tersedia",
  fulfillment_unavailable: "metode pengambilan tidak tersedia",
  too_long: "terlalu panjang",
  weak_password: "password minimal 8 karakter dan tidak sama dengan email",
});

const FIELD: Record<string, string> = tmap({
  name: "Nama", email: "Email", phone: "Nomor telepon", delivery_address: "Alamat pengiriman",
  get note() { return tx("Catatan"); }, fulfillment: "Pengambilan", password: "Password", items: "Keranjang",
});

function explain(e: unknown): string {
  if (e instanceof ApiError) {
    const f = e.fieldErrors;
    if (f.some((x) => x.code === "fulfillment_unavailable")) return tx("Cara pengambilan yang dipilih sudah tidak tersedia. Muat ulang halaman lalu pilih yang lain.");
    if (f.length) return f.map((x) => tx("{0}: {1}", FIELD[x.path] ?? x.path, ERR[x.code] ?? x.code)).join("; ");
    const reason = (e.details as { reason?: string } | null)?.reason;
    if (reason === "insufficient_stock") return tx("Stok tidak mencukupi untuk salah satu produk. Kurangi jumlahnya.");
    if (reason === "payment_not_configured") return tx("Toko belum menerima pesanan online.");
    if (reason === "email_taken") return tx("Email sudah terdaftar di toko ini. Silakan masuk.");
    if (e.status === 429) return tx("Terlalu banyak percobaan. Coba lagi beberapa menit lagi.");
    return serverMessage(e.code, e.message);
  }
  return tx("Tidak dapat menghubungi toko. Periksa koneksi Anda.");
}

/** Kerangka halaman toko = bingkai situs (navigasi, footer, brand kit) — lihat `./frame`. */
const Shell = ShopFrame;

/** Pratinjau Studio: data contoh (tanpa API, tanpa localStorage). */
export interface ShopPreview { cart: CartItem[]; cfg: CheckoutConfig; order: PublicOrder; orders: OrderSummary[] }
export const ShopPreviewCtx = createContext<ShopPreview | null>(null);

function Msg({ kind, children }: { kind: "error" | "ok"; children: ReactNode }) {
  if (!children) return null;
  return <p className={`shop-msg shop-msg-${kind}`} role={kind === "error" ? "alert" : "status"}>{children}</p>;
}

/** Tombol keranjang mengambang di halaman website toko. */
export function CartFab() {
  const [n, setN] = useState(0);
  useEffect(() => {
    const f = () => setN(cartCount());
    f();
    window.addEventListener("umkm-cart", f);
    window.addEventListener("storage", f);
    return () => {
      window.removeEventListener("umkm-cart", f);
      window.removeEventListener("storage", f);
    };
  }, []);
  if (n === 0) return null;
  return <a className="shop-fab" href="/keranjang"><Tr s="Keranjang ({0})" a={[n]} /></a>;
}

// ------------------------------------------------------------------ produk

export function ProductPage({ slug }: { slug: string }) {
  // Dokumen /produk/{slug} sudah memuat produknya (render pertama tanpa menunggu API).
  const [p, setP] = useState<PublicProduct | null>(() => inlineProduct(slug));
  const [err, setErr] = useState<string | null>(null);
  const [pick, setPick] = useState<string[]>(() => inlineProduct(slug)?.variants[0]?.option_values ?? []);
  const [qty, setQty] = useState(1);
  const [added, setAdded] = useState(false);
  useEffect(() => {
    const inline = inlineProduct(slug);
    if (inline) { document.title = inline.name; return; }
    shopApi<PublicProduct>("GET", `/products/${encodeURIComponent(slug)}`)
      .then((x) => {
        setP(x);
        setPick(x.variants[0]?.option_values ?? []);
        document.title = x.name;
      })
      .catch((e) => setErr(e instanceof ApiError && e.status === 404 ? tx("Produk tidak ditemukan.") : explain(e)));
  }, [slug]);
  const variant = useMemo(
    () => p?.variants.find((v) => v.option_values.length === pick.length && v.option_values.every((x, i) => x === pick[i])),
    [p, pick],
  );
  if (err) return <Shell title={tx("Produk")}><Msg kind="error">{err}</Msg></Shell>;
  if (!p) return <Shell title={tx("Produk")}><div className="shop-product shop-product-skel" aria-busy="true" aria-label={tx("Memuat…")}><span /><span /></div></Shell>;
  const sellable = p.available && !!variant;
  const clampQty = (n: number) => Math.max(1, Math.min(999, n));
  const add = () => {
    if (!variant) return;
    addToCart({ variant_id: variant.id, slug: p.slug, name: p.name, variant_name: variant.name, price: variant.promo_price ?? variant.price, image: p.images[0] }, qty);
    setAdded(true);
  };
  const price = variant && (
    <p className="shop-price">
      {variant.promo_price ? <><s>{rupiah(variant.price)}</s> <strong>{rupiah(variant.promo_price)}</strong></> : <strong>{rupiah(variant.price)}</strong>}
    </p>
  );
  return (
    <Shell title={p.name} heading={false}>
      <div className="shop-product">
        <div className="shop-gallery">
          {p.images.length === 0 && <span className="shop-noimg" aria-hidden="true" />}
          {p.images.map((src, i) => (
            // eslint-disable-next-line @next/next/no-img-element -- static export, gambar bertanda tangan
            <img key={i} src={sized(fileUrl(src), 960)} srcSet={srcSet(fileUrl(src))} sizes="(min-width: 720px) 50vw, 100vw"
              alt={i === 0 ? p.name : ""} width={960} height={960} loading={i === 0 ? "eager" : "lazy"} fetchPriority={i === 0 ? "high" : undefined} decoding="async" />
          ))}
        </div>
        <div className="shop-info">
          <h1>{p.name}</h1>
          {price}
          {variant?.promo_price && <p className="shop-save">{tx("Hemat {0}", rupiah(String(Number(variant.price) - Number(variant.promo_price))))}</p>}
          {p.description && <div className="shop-desc">{p.description.split("\n").map((l, i) => <p key={i}>{l}</p>)}</div>}
          {p.options.map((o, i) => (
            <fieldset key={o.name} className="shop-options">
              <legend>{o.name}</legend>
              {o.values.map((v) => (
                <button key={v} type="button" className="shop-chip" aria-pressed={pick[i] === v}
                  onClick={() => setPick((x) => x.map((cur, j) => (j === i ? v : cur)))}>{v}</button>
              ))}
            </fieldset>
          ))}
          {!variant && p.options.length > 0 && <Msg kind="error">{tx("Kombinasi ini tidak tersedia.")}</Msg>}
          {!p.available && <Msg kind="error">{tx("Produk sedang tidak tersedia.")}</Msg>}
          <div className="shop-buy">
            <div className="shop-qty">
              <button type="button" onClick={() => setQty((q) => clampQty(q - 1))} disabled={qty <= 1} aria-label={tx("Kurangi jumlah")}>−</button>
              <input type="number" inputMode="numeric" min={1} max={999} value={qty} aria-label={tx("Jumlah")}
                onChange={(e) => setQty(clampQty(Number(e.target.value) || 1))} />
              <button type="button" onClick={() => setQty((q) => clampQty(q + 1))} disabled={qty >= 999} aria-label={tx("Tambah jumlah")}>+</button>
            </div>
            <button type="button" className="s-btn shop-buy-btn" disabled={!sellable} onClick={add}>
              {tx("Tambah ke keranjang")}
            </button>
          </div>
          <Msg kind="ok">{added ? <>{tx("Ditambahkan ke keranjang.")}{" "}<a href="/keranjang">{tx("Lihat keranjang")}</a></> : null}</Msg>
        </div>
      </div>
    </Shell>
  );
}

// ------------------------------------------------------------------ keranjang & checkout

const DRAFT_KEY = "umkm-checkout-draft";

/** Langkah belanja (keranjang → data → bayar): pembeli tahu di mana posisinya. */
function Steps({ at }: { at: 1 | 2 | 3 }) {
  const steps = [tx("Keranjang"), tx("Data & pengambilan"), tx("Pembayaran")];
  return (
    <ol className="shop-steps" aria-label={tx("Langkah pemesanan")}>
      {steps.map((l, i) => (
        <li key={l} data-state={i + 1 < at ? "done" : i + 1 === at ? "now" : undefined} aria-current={i + 1 === at ? "step" : undefined}>
          <span aria-hidden="true">{i + 1 < at ? "✓" : i + 1}</span>{l}
        </li>
      ))}
    </ol>
  );
}

function BagIcon() {
  return (
    <svg className="shop-empty-art" viewBox="0 0 120 120" aria-hidden="true">
      <circle cx="60" cy="60" r="56" fill="var(--s-tint, #eef2ff)" />
      <path d="M36 46h48l-4 44a6 6 0 0 1-6 5H46a6 6 0 0 1-6-5Z" fill="var(--s-primary)" opacity=".9" />
      <path d="M48 52V42a12 12 0 0 1 24 0v10" fill="none" stroke="var(--s-ink, #0f172a)" strokeWidth="4" strokeLinecap="round" />
    </svg>
  );
}

export function CartPage() {
  const pv = useContext(ShopPreviewCtx);
  const texts = useShopTexts();
  const [cart, setCart] = useState<CartItem[]>(() => pv?.cart ?? []);
  const [cfg, setCfg] = useState<CheckoutConfig | null>(() => pv?.cfg ?? null);
  const [customer, setCustomer] = useState<CustomerSession | null>(null);
  // Draf isian bertahan saat halaman dimuat ulang (UAT customer #10). sessionStorage: hilang saat tab
  // ditutup — data pribadi tidak tertinggal di perangkat bersama. Persetujuan tidak ikut disimpan.
  // (CartPage hanya dirender di browser: path diketahui setelah hidrasi.)
  const [form, setForm] = useState(() => {
    const base = { fulfillment: "pickup", name: "", email: "", phone: "", delivery_address: "", note: "", accept: false };
    if (pv) return base;
    try {
      const d = JSON.parse(sessionStorage.getItem(DRAFT_KEY) ?? "null") as Partial<typeof base> | null;
      return d ? { ...base, name: d.name ?? "", email: d.email ?? "", phone: d.phone ?? "", delivery_address: d.delivery_address ?? "", note: d.note ?? "" } : base;
    } catch { return base; }
  });
  useEffect(() => {
    if (pv) return;
    try {
      const { name, email, phone, delivery_address, note } = form;
      if (name || email || phone || delivery_address || note) sessionStorage.setItem(DRAFT_KEY, JSON.stringify({ name, email, phone, delivery_address, note }));
    } catch { /* opsional */ }
  }, [form, pv]);
  const [key, setKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (pv) return;
    const f = () => setCart(getCart());
    f();
    window.addEventListener("umkm-cart", f);
    shopApi<CheckoutConfig>("GET", "/checkout-config")
      .then((c) => {
        setCfg(c);
        setForm((x) => ({ ...x, fulfillment: c.pickup_available ? "pickup" : "delivery" }));
      })
      .catch((e) => setErr(explain(e)));
    loadCustomer().then((s) => {
      setCustomer(s);
      if (s) setForm((x) => ({ ...x, email: x.email || s.email }));
    });
    return () => window.removeEventListener("umkm-cart", f);
  }, [pv]);
  const set = (k: keyof typeof form, v: string | boolean) => {
    setForm((x) => ({ ...x, [k]: v }));
    setKey(null); // isi berubah → pesanan baru (kunci idempotensi baru)
  };
  const qty = (variant: string, q: number) => {
    const n = Math.max(0, Math.min(999, q));
    if (pv) setCart((c) => c.map((x) => (x.variant_id === variant ? { ...x, quantity: n } : x)).filter((x) => x.quantity > 0));
    else setQuantity(variant, n);
    setKey(null);
  };
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pv) return;
    setErr(null);
    setBusy(true);
    try {
      // Kunci dipertahankan untuk percobaan ulang isi yang sama (jaringan putus) → tidak dobel.
      const k = key ?? newKey();
      setKey(k);
      const r = await shopApi<{ order: PublicOrder; access_token: string }>(
        "POST",
        "/checkout",
        {
          json: {
            items: cart.map((c) => ({ variant_id: c.variant_id, quantity: c.quantity })),
            fulfillment: form.fulfillment,
            name: form.name,
            email: form.email || null,
            phone: form.phone || null,
            delivery_address: form.fulfillment === "delivery" ? form.delivery_address : null,
            note: form.note || null,
            accept_privacy: form.accept,
            accept_terms: form.accept,
          },
        },
        { "idempotency-key": k },
      );
      saveOrder({ code: r.order.code, token: r.access_token, created_at: r.order.created_at });
      clearCart();
      try { sessionStorage.removeItem(DRAFT_KEY); } catch { /* opsional */ }
      window.location.assign(`/pesanan#token=${encodeURIComponent(r.access_token)}`);
    } catch (e2) {
      setErr(explain(e2));
    } finally {
      setBusy(false);
    }
  }
  const subtotal = displaySubtotalSen(cart);
  const fee = cfg && form.fulfillment === "delivery" ? Math.round(Number(cfg.delivery_fee) * 100) : 0;
  const sen = (n: number) => rupiah(`${Math.floor(n / 100)}.${String(n % 100).padStart(2, "0")}`);
  const count = cart.reduce((n, c) => n + c.quantity, 0);
  const canCheckout = !!cfg?.checkout_enabled && (cfg.pickup_available || cfg.delivery_available);
  const title = texts.cart_title?.trim() || tx("Keranjang");
  if (cart.length === 0) {
    return (
      <Shell title={title}>
        <div className="shop-empty">
          <BagIcon />
          <p>{texts.empty_cart?.trim() || tx("Keranjang masih kosong. Yuk lihat produk kami!")}</p>
          <a className="s-btn" href="/">{tx("Lihat produk")}</a>
        </div>
      </Shell>
    );
  }
  return (
    <Shell title={title} wide>
      <Steps at={1} />
      <div className="shop-cart">
        <div className="shop-cart-main">
          <section className="shop-card" aria-label={tx("Isi keranjang")}>
            <h2>{tx("Isi keranjang")} <span className="s-muted">({tx("{0} barang", count)})</span></h2>
            <ul className="shop-items">
              {cart.map((c) => (
                <li key={c.variant_id} className="shop-item">
                  {c.image
                    // eslint-disable-next-line @next/next/no-img-element -- foto produk bertanda tangan
                    ? <img src={pv ? c.image : fileUrl(c.image)} alt="" width={72} height={72} loading="lazy" onError={onImgError} />
                    : <span className="shop-item-noimg" aria-hidden="true" />}
                  <div className="shop-item-info">
                    <a href={`/produk/${c.slug}`}>{c.name}</a>
                    {c.variant_name !== "Default" && <span className="s-muted">{c.variant_name}</span>}
                    <span className="s-muted">{rupiah(c.price)}</span>
                  </div>
                  <div className="shop-qty" role="group" aria-label={tx("Jumlah {0}", c.name)}>
                    <button type="button" aria-label={tx("Kurangi")} onClick={() => qty(c.variant_id, c.quantity - 1)}>−</button>
                    <input aria-label={tx("Jumlah {0}", c.name)} type="number" min={0} max={999} value={c.quantity} onChange={(e) => qty(c.variant_id, Number(e.target.value) || 0)} />
                    <button type="button" aria-label={tx("Tambah")} onClick={() => qty(c.variant_id, c.quantity + 1)}>+</button>
                  </div>
                  <strong className="shop-item-total">{sen(Math.round(Number(c.price) * 100) * c.quantity)}</strong>
                  <button type="button" className="shop-item-del" aria-label={tx("Hapus {0} dari keranjang", c.name)} onClick={() => qty(c.variant_id, 0)}>×</button>
                </li>
              ))}
            </ul>
          </section>
          {cfg && !cfg.checkout_enabled && <Msg kind="error">{tx("Toko belum menerima pesanan online.")}</Msg>}
          {cfg?.checkout_enabled && !cfg.pickup_available && !cfg.delivery_available && (
            <Msg kind="error">{tx("Toko belum mengatur cara pengambilan atau pengiriman, jadi pesanan online belum bisa dibuat. Silakan hubungi toko secara langsung.")}</Msg>
          )}
          {cfg && canCheckout && (
            <form id="checkout-form" className="shop-card shop-form" onSubmit={submit}>
              <h2><span className="shop-step-num" aria-hidden="true">2</span>{tx("Cara pengambilan")}</h2>
              <div className="shop-choices" role="radiogroup" aria-label={tx("Pengambilan")}>
                {cfg.pickup_available && (
                  <label className="shop-choice"><input type="radio" name="f" checked={form.fulfillment === "pickup"} onChange={() => set("fulfillment", "pickup")} />
                    <span><strong>{tx("Ambil di toko")}</strong><span className="s-muted">{tx("Gratis")}</span></span></label>
                )}
                {cfg.delivery_available && (
                  <label className="shop-choice"><input type="radio" name="f" checked={form.fulfillment === "delivery"} onChange={() => set("fulfillment", "delivery")} />
                    <span><strong>{tx("Dikirim")}</strong><span className="s-muted">{rupiah(cfg.delivery_fee)}</span></span></label>
                )}
              </div>
              <h2><span className="shop-step-num" aria-hidden="true">3</span>{tx("Data pemesan")}</h2>
              <div className="shop-grid2">
                <label className="shop-field">{tx("Nama")}<input required maxLength={120} value={form.name} onChange={(e) => set("name", e.target.value)} autoComplete="name" /></label>
                <label className="shop-field">{tx("Nomor WhatsApp/telepon")}<input type="tel" maxLength={20} value={form.phone} onChange={(e) => set("phone", e.target.value)} autoComplete="tel" /></label>
              </div>
              <label className="shop-field">{tx("Email")}<input type="email" maxLength={254} value={form.email} onChange={(e) => set("email", e.target.value)} autoComplete="email" /></label>
              {form.fulfillment === "delivery" && (
                <label className="shop-field">{tx("Alamat pengiriman")}<textarea required maxLength={500} value={form.delivery_address} onChange={(e) => set("delivery_address", e.target.value)} autoComplete="street-address" /></label>
              )}
              <label className="shop-field">{tx("Catatan (opsional)")}<textarea maxLength={500} value={form.note} onChange={(e) => set("note", e.target.value)} /></label>
              <label className="shop-check">
                <input type="checkbox" checked={form.accept} onChange={(e) => set("accept", e.target.checked)} required />
                <span>{tx("Saya menyetujui")}{" "}<a href="/kebijakan-privasi" target="_blank" rel="noopener">{tx("kebijakan privasi")}</a> dan <a href="/syarat-transaksi" target="_blank" rel="noopener">{tx("syarat transaksi")}</a>.</span>
              </label>
              {customer && <p className="s-muted"><Tr s="Masuk sebagai {0}." a={[customer.email]} /></p>}
              <Msg kind="error">{err}</Msg>
            </form>
          )}
          {!cfg && <Msg kind="error">{err}</Msg>}
        </div>
        <aside className="shop-summary" aria-label={tx("Ringkasan pesanan")}>
          <h2>{tx("Ringkasan")}</h2>
          <dl>
            <div><dt>{tx("Subtotal ({0} barang)", count)}</dt><dd>{sen(subtotal)}</dd></div>
            <div><dt>{tx("Ongkir")}</dt><dd>{fee > 0 ? sen(fee) : form.fulfillment === "pickup" ? tx("Gratis") : "—"}</dd></div>
            <div className="shop-summary-total"><dt>{tx("Perkiraan total")}</dt><dd>{sen(subtotal + fee)}</dd></div>
          </dl>
          {texts.cart_note?.trim() && <p className="shop-summary-note">{texts.cart_note}</p>}
          {canCheckout && (
            <button type="submit" form="checkout-form" className="s-btn shop-summary-cta" disabled={busy || !form.accept}>
              {busy ? tx("Memproses…") : texts.checkout_button?.trim() || tx("Buat pesanan")}
            </button>
          )}
          {canCheckout && !form.accept && <p className="s-muted shop-summary-hint">{tx("Centang persetujuan di formulir untuk melanjutkan.")}</p>}
          <p className="s-muted shop-summary-hint">{tx("Total resmi dihitung toko saat pesanan dibuat.")}</p>
        </aside>
      </div>
    </Shell>
  );
}

// ------------------------------------------------------------------ pesanan & pembayaran

/** Kunci idempotensi acak. `getRandomValues` tersedia juga di origin non-HTTPS (dev), berbeda dengan randomUUID. */
export function newKey(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

function tokenFromHash(): string | null {
  const m = /(?:^|&)token=([^&]+)/.exec(window.location.hash.replace(/^#/, ""));
  return m ? decodeURIComponent(m[1]!) : null;
}

function onHash(cb: () => void) {
  window.addEventListener("hashchange", cb);
  return () => window.removeEventListener("hashchange", cb);
}

/** Token dari fragment URL (tidak pernah dikirim ke server/log). `undefined` saat render statis. */
function useHashToken(): string | null | undefined {
  return useSyncExternalStore(onHash, tokenFromHash, () => undefined);
}

/** Perjalanan pesanan: dipesan → dibayar → diproses → siap diambil/dikirim → selesai. */
function OrderTimeline({ status, fulfillment }: { status: string; fulfillment: string }) {
  const steps = [tx("Dipesan"), tx("Dibayar"), tx("Diproses"), fulfillment === "delivery" ? tx("Dikirim") : tx("Siap diambil"), tx("Selesai")];
  const at: Record<string, number> = {
    PENDING: 1, AWAITING_PAYMENT: 1, APPROVAL_PENDING: 1, PAID: 2, APPROVED: 2, PROCESSING: 2,
    READY_FOR_PICKUP: 3, SHIPPED: 3, COMPLETED: 5,
  };
  const stopped = ["CANCELLED", "EXPIRED", "REJECTED"].includes(status);
  const now = at[status] ?? 0;
  return (
    <ol className="shop-timeline" data-stopped={stopped || undefined} aria-label={tx("Status pesanan")}>
      {steps.map((l, i) => (
        <li key={l} data-state={stopped ? undefined : i < now ? "done" : i === now ? "now" : undefined} aria-current={!stopped && i === now ? "step" : undefined}>
          <span className="shop-timeline-dot" aria-hidden="true" />
          <span>{l}</span>
        </li>
      ))}
    </ol>
  );
}

export function OrderPage() {
  const pv = useContext(ShopPreviewCtx);
  const texts = useShopTexts();
  const hashToken = useHashToken();
  const token = pv ? "preview" : hashToken;
  const [order, setOrder] = useState<PublicOrder | null>(() => pv?.order ?? null);
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);
  const [ref, setRef] = useState("");
  const [busy, setBusy] = useState(false);
  // P10: QRIS dinamis GoPay (konfirmasi otomatis). null = belum/ tidak tersedia.
  const [gopay, setGopay] = useState<{ available: boolean; total_rupiah: number | null; qr_svg: string | null } | null>(null);
  const load = useCallback(
    (t: string) =>
      shopApi<PublicOrder>("GET", "/orders/current", undefined, { "x-order-token": t }).then(
        (o) => {
          setOrder(o);
          setErr(null);
        },
        (e) => setErr(e instanceof ApiError && e.status === 404 ? tx("Pesanan tidak ditemukan.") : explain(e)),
      ),
    [],
  );
  useEffect(() => {
    if (token && !pv) void load(token);
  }, [load, token, pv]);
  // Ambil QRIS dinamis GoPay bila order menunggu bayar (idempoten; available=false bila tak dipakai).
  useEffect(() => {
    if (!token || pv || order?.status !== "AWAITING_PAYMENT") return;
    shopApi<{ available: boolean; total_rupiah: number | null; qr_svg: string | null }>(
      "POST", "/orders/current/gopay-qris", undefined, { "x-order-token": token },
    ).then(setGopay, () => setGopay(null));
  }, [token, order?.status, pv]);
  // P10: selama menunggu bayar, periksa status berkala agar auto-konfirmasi (GoPay) atau verifikasi
  // staf langsung terlihat pembeli tanpa menekan "Perbarui status". Berhenti saat status berubah.
  useEffect(() => {
    if (!token || pv || order?.status !== "AWAITING_PAYMENT") return;
    const id = setInterval(() => { void load(token); }, 5000);
    return () => clearInterval(id);
  }, [token, order?.status, load, pv]);
  if (token === undefined) return <Shell title={tx("Pesanan")}><p className="s-muted" aria-busy="true">{tx("Memuat…")}</p></Shell>;
  if (!token) {
    const saved = savedOrders();
    return (
      <Shell title={tx("Pesanan saya")}>
        {saved.length === 0 && (
          <div className="shop-empty"><BagIcon /><p>{tx("Belum ada pesanan di perangkat ini.")}</p><a className="s-btn" href="/">{tx("Mulai belanja")}</a></div>
        )}
        <ul className="shop-order-list">
          {saved.map((o) => (
            <li key={o.code}><a className="shop-card shop-order-row" href={`/pesanan#token=${encodeURIComponent(o.token)}`}>
              <strong>{o.code}</strong><span className="s-muted">{new Date(o.created_at).toLocaleDateString(intlLocale())}</span><span aria-hidden="true">→</span>
            </a></li>
          ))}
        </ul>
        <p className="s-muted">{tx("Punya akun?")}{" "}<a href="/akun">{tx("Lihat riwayat di akun")}</a>.</p>
      </Shell>
    );
  }
  async function claim() {
    if (!token) return;
    setBusy(true);
    setErr(null);
    try {
      setOrder(await shopApi<PublicOrder>("POST", "/orders/current/payment-claim", { json: { payer_reference: ref || null } }, { "x-order-token": token }));
      setOk(tx("Terima kasih. Toko akan memverifikasi pembayaran Anda."));
    } catch (e) {
      setErr(explain(e));
    } finally {
      setBusy(false);
    }
  }
  async function proof(file: File) {
    if (!token) return;
    setBusy(true);
    setErr(null);
    try {
      await shopApi("POST", "/orders/current/payment-proof", { bytes: await prepareImage(file) }, { "x-order-token": token });
      setOk(tx("Bukti pembayaran terkirim."));
      await load(token);
    } catch (e) {
      setErr(explain(e));
    } finally {
      setBusy(false);
    }
  }
  if (!order) return <Shell title={tx("Pesanan")}><Msg kind="error">{err}</Msg>{!err && <p className="s-muted" aria-busy="true">{tx("Memuat…")}</p>}</Shell>;
  const p = order.payment;
  const waiting = order.status === "AWAITING_PAYMENT" && p && (p.status === "pending" || p.status === "claimed");
  return (
    <Shell title={tx("Pesanan {0}", order.code)}>
      <section className="shop-card shop-order-head">
        <div className="shop-order-meta">
          <p className="shop-status" data-status={order.status}>{STATUS_TEXT[order.status] ?? order.status}</p>
          <span className="s-muted">{new Date(order.created_at).toLocaleString(intlLocale())}</span>
        </div>
        <OrderTimeline status={order.status} fulfillment={order.fulfillment} />
        {texts.thanks?.trim() && <p className="shop-thanks">{texts.thanks}</p>}
      </section>
      <Msg kind="ok">{ok}</Msg>
      <Msg kind="error">{err}</Msg>
      {waiting && (
        <section className="shop-pay" aria-label={tx("Pembayaran QRIS")}>
          <h2>{tx("Bayar dengan QRIS")}</h2>
          {gopay?.available && gopay.qr_svg ? (
            <>
              <p>{tx("Pindai kode QRIS berikut, lalu bayar")}{" "}<strong>tepat {rupiah(`${gopay.total_rupiah}.00`)}</strong>{tx(". Pembayaran terkonfirmasi otomatis.")}</p>
              {/* eslint-disable-next-line @next/next/no-img-element -- QR dinamis SVG inline (data URL) */}
              <img className="shop-qris" src={`data:image/svg+xml;utf8,${encodeURIComponent(gopay.qr_svg)}`} alt={tx("QRIS pembayaran")} />
              <p className="s-muted">{tx("Nominal berakhiran angka unik agar pembayaran Anda dikenali otomatis — jangan dibulatkan. Status pesanan berubah sendiri setelah dibayar.")}</p>
            </>
          ) : (
            <>
              <p>{tx("Pindai kode QRIS berikut dengan aplikasi pembayaran Anda, lalu bayar")}{" "}<strong>tepat {rupiah(p.amount)}</strong>.</p>
              {p.qris_url && (
                // eslint-disable-next-line @next/next/no-img-element -- QRIS toko, signed URL
                <img className="shop-qris" src={fileUrl(p.qris_url)} alt={tx("QRIS {0}", p.receiver_name ?? "toko")} />
              )}
              <p className="s-muted"><Tr s="Penerima: {0}. Cantumkan kode pesanan {1} bila aplikasi meminta catatan." a={[p.receiver_name, order.code]} /></p>
            </>
          )}
          <p className="s-muted">{tx("Bayar sebelum")}{" "}{new Date(order.expires_at).toLocaleString(intlLocale())}.</p>
          {p.status === "claimed" ? (
            <p>{tx("Anda sudah menandai pembayaran. Toko sedang memverifikasi.")}</p>
          ) : (
            <>
              <label className="shop-field">{tx("Nama pembayar / catatan (opsional)")}<input maxLength={120} value={ref} onChange={(e) => setRef(e.target.value)} /></label>
              <button type="button" className="s-btn" onClick={claim} disabled={busy}>{tx("Saya sudah membayar")}</button>
            </>
          )}
          <label className="shop-field">{tx("Unggah bukti pembayaran (opsional, PNG/JPEG/WebP)")}
            <input type="file" accept="image/png,image/jpeg,image/webp" disabled={busy}
              onChange={(e) => { const f = e.target.files?.[0]; if (f) void proof(f); }} />
          </label>
        </section>
      )}
      {p?.status === "rejected" && <Msg kind="error">{tx("Pembayaran ditolak toko:")}{" "}{p.rejection_reason}</Msg>}
      <section className="shop-card" aria-label={tx("Rincian pesanan")}>
        <h2>{tx("Rincian pesanan")}</h2>
        <ul className="shop-items shop-items-static">
          {order.items.map((i, k) => (
            <li key={k} className="shop-item">
              <div className="shop-item-info"><strong>{i.product_name}</strong>{i.variant_name !== "Default" && <span className="s-muted">{i.variant_name}</span>}<span className="s-muted">{i.quantity} × {rupiah(i.unit_price)}</span></div>
              <strong className="shop-item-total">{rupiah(i.line_total)}</strong>
            </li>
          ))}
        </ul>
        <dl className="shop-sum">
          {order.delivery_fee !== "0.00" && <div><dt>{tx("Ongkir")}</dt><dd>{rupiah(order.delivery_fee)}</dd></div>}
          <div className="shop-summary-total"><dt>{tx("Total")}</dt><dd>{rupiah(order.total)}</dd></div>
        </dl>
        <p className="s-muted">{order.fulfillment === "delivery" ? tx("Dikirim ke: {0}", order.delivery_address ?? "") : tx("Ambil di toko")}</p>
        {!pv && <button type="button" className="d-link" onClick={() => token && load(token)}>{tx("Perbarui status")}</button>}
      </section>
      {!pv && <OrderTickets token={token} />}
    </Shell>
  );
}

interface PublicTicket { id: string; code: string; subject: string; status: string; messages: { id: string; author_type: string; body: string; created_at: string }[] }

const TICKET_STATUS: Record<string, string> = tmap({
  OPEN: "Diterima", ASSIGNED: "Diterima", IN_PROGRESS: "Sedang ditangani", WAITING_CUSTOMER: "Menunggu balasan Anda",
  WAITING_INTERNAL: "Sedang ditangani", RESOLVED: "Selesai", CLOSED: "Ditutup",
});

/** Laporan masalah pesanan (P4-3): tiket hanya dengan balasan publik toko. */
function OrderTickets({ token }: { token: string }) {
  const [items, setItems] = useState<PublicTicket[]>([]);
  const [form, setForm] = useState({ subject: "", description: "" });
  const [reply, setReply] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);
  const h = { "x-order-token": token };
  const load = useCallback(
    () => shopApi<PublicTicket[]>("GET", "/orders/current/tickets", undefined, { "x-order-token": token }).then(setItems, () => setItems([])),
    [token],
  );
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>) {
    setErr(null);
    try { await f(); await load(); } catch (e) { setErr(explain(e)); }
  }
  return (
    <section aria-label={tx("Bantuan pesanan")}>
      <h2>{tx("Ada masalah dengan pesanan?")}</h2>
      <Msg kind="error">{err}</Msg>
      {items.map((t) => (
        <div key={t.id} className="shop-pay">
          <strong>{t.subject}</strong> <span className="s-muted">({t.code} · {TICKET_STATUS[t.status] ?? t.status})</span>
          <ul className="shop-list">
            {t.messages.map((m) => <li key={m.id}><strong>{m.author_type === "customer" ? tx("Anda") : tx("Toko")}:</strong> {m.body}</li>)}
          </ul>
          <label className="shop-field">{tx("Balas")}<textarea maxLength={5000} value={reply[t.id] ?? ""} onChange={(e) => setReply({ ...reply, [t.id]: e.target.value })} /></label>
          <button type="button" className="s-btn" disabled={!(reply[t.id] ?? "").trim()} onClick={() => run(async () => {
            await shopApi("POST", `/orders/current/tickets/${t.id}/messages`, { json: { body: reply[t.id] } }, h);
            setReply({ ...reply, [t.id]: "" });
          })}>{tx("Kirim balasan")}</button>
        </div>
      ))}
      <label className="shop-field">{tx("Judul masalah")}<input maxLength={200} value={form.subject} onChange={(e) => setForm({ ...form, subject: e.target.value })} /></label>
      <label className="shop-field">{tx("Ceritakan masalahnya")}<textarea maxLength={5000} value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} /></label>
      <button type="button" className="s-btn" disabled={!form.subject.trim()} onClick={() => run(async () => {
        await shopApi("POST", "/orders/current/tickets", { json: { subject: form.subject, description: form.description || null } }, h);
        setForm({ subject: "", description: "" });
      })}>{tx("Laporkan masalah")}</button>
    </section>
  );
}

// ------------------------------------------------------------------ akun customer

export function AccountPage() {
  const pv = useContext(ShopPreviewCtx);
  const texts = useShopTexts();
  const [me, setMe] = useState<CustomerSession | null>(null);
  const [ready, setReady] = useState(!!pv);
  const [orders, setOrders] = useState<OrderSummary[]>([]);
  const [mode, setMode] = useState<"login" | "register">("login");
  const [f, setF] = useState({ email: "", password: "", accept: false });
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);
  const refresh = useCallback(
    () =>
      loadCustomer().then(async (s) => {
        const r = s ? await shopApi<{ items: OrderSummary[] }>("GET", "/customer/orders").catch(() => ({ items: [] })) : { items: [] };
        setMe(s);
        setOrders(r.items);
        setReady(true);
      }),
    [],
  );
  useEffect(() => {
    if (pv) return;
    refresh().catch(() => setReady(true));
  }, [refresh, pv]);
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pv) return;
    setErr(null);
    setOk(null);
    try {
      if (mode === "register") {
        await shopApi("POST", "/customer/register", { json: { email: f.email, password: f.password, accept_privacy: f.accept, accept_terms: f.accept } });
        setOk(tx("Akun dibuat. Periksa email Anda untuk verifikasi, lalu masuk."));
        setMode("login");
      } else {
        const s = await shopApi<CustomerSession>("POST", "/customer/login", { json: { email: f.email, password: f.password } });
        setCustomerCsrf(s.csrf_token);
        await refresh();
      }
    } catch (e2) {
      setErr(explain(e2));
    }
  }
  if (!ready) return <Shell title={tx("Akun")}><p className="s-muted" aria-busy="true">{tx("Memuat…")}</p></Shell>;
  if (me) {
    return (
      <Shell title={tx("Akun saya")}>
        <section className="shop-card shop-profile">
          <span className="shop-avatar" aria-hidden="true">{me.email.charAt(0).toUpperCase()}</span>
          <div><strong>{me.email}</strong><span className={`shop-verify${me.email_verified ? " is-ok" : ""}`}>{me.email_verified ? tx("Email terverifikasi") : tx("Belum terverifikasi")}</span></div>
          <button type="button" className="d-link shop-logout" onClick={async () => { await shopApi("POST", "/customer/logout").catch(() => null); setCustomerCsrf(null); setMe(null); }}>{tx("Keluar")}</button>
        </section>
        {!me.email_verified && <p className="s-muted">{tx("Verifikasi email untuk melihat pesanan yang dibuat dengan email ini sebelum Anda mendaftar.")}</p>}
        <h2>{tx("Riwayat pesanan")}</h2>
        {orders.length === 0 && <p className="s-muted">{tx("Belum ada pesanan.")}</p>}
        <ul className="shop-order-list">
          {orders.map((o) => (
            <li key={o.id} className="shop-card shop-order-row">
              <strong>{o.code}</strong>
              <span className="shop-status" data-status={o.status}>{STATUS_TEXT[o.status] ?? o.status}</span>
              <strong>{rupiah(o.total)}</strong>
            </li>
          ))}
        </ul>
      </Shell>
    );
  }
  return (
    <Shell title={mode === "login" ? tx("Masuk") : tx("Daftar akun")}>
      <div className="shop-account">
      <div className="shop-card shop-account-why">
        <h2>{tx("Kenapa punya akun?")}</h2>
        <ul>
          <li>{tx("Lihat semua riwayat pesanan di satu tempat")}</li>
          <li>{tx("Isi data pemesanan lebih cepat")}</li>
          <li>{tx("Tetap opsional — Anda bisa memesan tanpa akun")}</li>
        </ul>
        {texts.account_note?.trim() && <p className="shop-summary-note">{texts.account_note}</p>}
      </div>
      <form className="shop-card shop-form" onSubmit={submit}>
        <label className="shop-field">{tx("Email")}<input type="email" required value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} autoComplete="email" /></label>
        <label className="shop-field">{tx("Password")}<input type="password" required minLength={8} value={f.password} onChange={(e) => setF({ ...f, password: e.target.value })} autoComplete={mode === "login" ? "current-password" : "new-password"} /></label>
        {mode === "register" && (
          <label className="shop-check">
            <input type="checkbox" checked={f.accept} onChange={(e) => setF({ ...f, accept: e.target.checked })} required />
            {tx("Saya menyetujui")}{" "}<a href="/kebijakan-privasi" target="_blank" rel="noopener">{tx("kebijakan privasi")}</a> dan <a href="/syarat-transaksi" target="_blank" rel="noopener">syarat</a>.
          </label>
        )}
        <Msg kind="error">{err}</Msg>
        <Msg kind="ok">{ok}</Msg>
        <button type="submit" className="s-btn">{mode === "login" ? tx("Masuk") : tx("Daftar")}</button>
        <button type="button" className="d-link" onClick={() => setMode(mode === "login" ? "register" : "login")}>
          {mode === "login" ? tx("Belum punya akun? Daftar") : tx("Sudah punya akun? Masuk")}
        </button>
      </form>
      </div>
    </Shell>
  );
}

export function VerifyPage() {
  const token = useHashToken();
  const [result, setResult] = useState<"busy" | "ok" | "bad">("busy");
  const [linked, setLinked] = useState(0);
  useEffect(() => {
    if (!token) return;
    shopApi<{ linked_customers: number }>("POST", "/customer/verify-email", { json: { token } })
      .then((r) => { setLinked(r.linked_customers); setResult("ok"); })
      .catch(() => setResult("bad"));
  }, [token]);
  const state = token === null ? "bad" : token === undefined ? "busy" : result;
  return (
    <Shell title={tx("Verifikasi email")}>
      {state === "busy" && <p className="s-muted" aria-busy="true">{tx("Memverifikasi…")}</p>}
      {state === "ok" && <Msg kind="ok">{tx("Email terverifikasi")}{linked > 0 ? tx(" dan {0} riwayat pesanan ditautkan", linked) : ""}. <a href="/akun">{tx("Masuk ke akun")}</a></Msg>}
      {state === "bad" && <Msg kind="error">{tx("Tautan verifikasi tidak valid atau kedaluwarsa.")}</Msg>}
    </Shell>
  );
}

/** Teks hukum (D-12) di situs toko: sumber sama dengan dashboard (`@/legal/content`). */
export function LegalPage({ kind }: { kind: "privacy" | "terms" }) {
  return (
    <Shell title={kind === "privacy" ? tx("Kebijakan privasi") : tx("Syarat & ketentuan")}>
      <LegalDoc sections={kind === "privacy" ? PRIVACY : TERMS} />
    </Shell>
  );
}
