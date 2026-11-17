"use client";
/* eslint-disable @next/next/no-html-link-for-pages, @next/next/no-location-assign-relative-destination --
   situs toko: SEMUA path dilayani shell site.html oleh Caddy (ADR-0014). next/link akan melakukan
   navigasi klien ke rute Next (mis. "/" = dashboard), jadi tautan HTML biasa yang benar. */

// Halaman toko (P3): produk, keranjang + checkout, status pesanan + pembayaran QRIS statis
// (ADR-0004), akun customer opsional (ADR-0005). Semua teks dirender sebagai teks (React).

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";

import { prepareImage } from "@/lib/image";
import { ApiError } from "@/lib/api";
import { PRIVACY, TERMS } from "@/legal/content";
import { LegalDoc } from "@/legal/LegalDoc";
import { sized, srcSet } from "@/renderer/safe";

import {
  STATUS_TEXT, fileUrl, loadCustomer, rupiah, setCustomerCsrf, shopApi,
  type CheckoutConfig, type CustomerSession, type OrderSummary, type PublicOrder, type PublicProduct,
} from "./api";
import { addToCart, cartCount, clearCart, displaySubtotalSen, getCart, saveOrder, savedOrders, setQuantity, type CartItem } from "./cart";
import { t as tx, tmap, intlLocale } from "@/i18n";
import { Tr } from "@/i18n/react";
import { LanguageSwitch } from "@/i18n/react";
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

function Shell({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="shop">
      <nav className="shop-nav" aria-label={tx("Navigasi toko")}>
        <a href="/">{tx("← Kembali ke toko")}</a>
        <span className="shop-nav-links">
          <a href="/keranjang">{tx("Keranjang")}</a>
          <a href="/pesanan">{tx("Pesanan")}</a>
          <a href="/akun">{tx("Akun")}</a>
          <LanguageSwitch className="s-lang" />
        </span>
      </nav>
      <main className="shop-main">
        <h1>{title}</h1>
        {children}
      </main>
    </div>
  );
}

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
  const [p, setP] = useState<PublicProduct | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [pick, setPick] = useState<string[]>([]);
  const [qty, setQty] = useState(1);
  const [added, setAdded] = useState(false);
  useEffect(() => {
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
  if (!p) return <Shell title={tx("Produk")}><p className="s-muted" aria-busy="true">{tx("Memuat…")}</p></Shell>;
  const sellable = p.available && !!variant;
  return (
    <Shell title={p.name}>
      <div className="shop-product">
        <div className="shop-gallery">
          {p.images.map((src, i) => (
            // eslint-disable-next-line @next/next/no-img-element -- static export, gambar bertanda tangan
            <img key={i} src={sized(fileUrl(src), 960)} srcSet={srcSet(fileUrl(src))} sizes="(min-width: 720px) 50vw, 100vw"
              alt={i === 0 ? p.name : ""} width={960} height={960} loading={i === 0 ? "eager" : "lazy"} decoding="async" />
          ))}
        </div>
        <div>
          {variant && (
            <p className="shop-price">
              {variant.promo_price ? <><s>{rupiah(variant.price)}</s> <strong>{rupiah(variant.promo_price)}</strong></> : <strong>{rupiah(variant.price)}</strong>}
            </p>
          )}
          {p.description && p.description.split("\n").map((l, i) => <p key={i}>{l}</p>)}
          {p.options.map((o, i) => (
            <label key={o.name} className="shop-field">
              {o.name}
              <select value={pick[i] ?? ""} onChange={(e) => setPick((x) => x.map((v, j) => (j === i ? e.target.value : v)))}>
                {o.values.map((v) => <option key={v} value={v}>{v}</option>)}
              </select>
            </label>
          ))}
          {!variant && p.options.length > 0 && <Msg kind="error">{tx("Kombinasi ini tidak tersedia.")}</Msg>}
          {!p.available && <Msg kind="error">{tx("Produk sedang tidak tersedia.")}</Msg>}
          <label className="shop-field">
            {tx("Jumlah")}
            <input type="number" min={1} max={999} value={qty} onChange={(e) => setQty(Math.max(1, Math.min(999, Number(e.target.value) || 1)))} />
          </label>
          <button type="button"
            className="s-btn"
            disabled={!sellable}
            onClick={() => {
              if (!variant) return;
              addToCart({ variant_id: variant.id, slug: p.slug, name: p.name, variant_name: variant.name, price: variant.promo_price ?? variant.price }, qty);
              setAdded(true);
            }}
          >
            {tx("Tambah ke keranjang")}
          </button>
          <Msg kind="ok">{added ? <>{tx("Ditambahkan.")}{" "}<a href="/keranjang">{tx("Lihat keranjang")}</a></> : null}</Msg>
        </div>
      </div>
    </Shell>
  );
}

// ------------------------------------------------------------------ keranjang & checkout

const DRAFT_KEY = "umkm-checkout-draft";

export function CartPage() {
  const [cart, setCart] = useState<CartItem[]>([]);
  const [cfg, setCfg] = useState<CheckoutConfig | null>(null);
  const [customer, setCustomer] = useState<CustomerSession | null>(null);
  // Draf isian bertahan saat halaman dimuat ulang (UAT customer #10). sessionStorage: hilang saat tab
  // ditutup — data pribadi tidak tertinggal di perangkat bersama. Persetujuan tidak ikut disimpan.
  // (CartPage hanya dirender di browser: path diketahui setelah hidrasi.)
  const [form, setForm] = useState(() => {
    const base = { fulfillment: "pickup", name: "", email: "", phone: "", delivery_address: "", note: "", accept: false };
    try {
      const d = JSON.parse(sessionStorage.getItem(DRAFT_KEY) ?? "null") as Partial<typeof base> | null;
      return d ? { ...base, name: d.name ?? "", email: d.email ?? "", phone: d.phone ?? "", delivery_address: d.delivery_address ?? "", note: d.note ?? "" } : base;
    } catch { return base; }
  });
  useEffect(() => {
    try {
      const { name, email, phone, delivery_address, note } = form;
      if (name || email || phone || delivery_address || note) sessionStorage.setItem(DRAFT_KEY, JSON.stringify({ name, email, phone, delivery_address, note }));
    } catch { /* opsional */ }
  }, [form]);
  const [key, setKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
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
  }, []);
  const set = (k: keyof typeof form, v: string | boolean) => {
    setForm((x) => ({ ...x, [k]: v }));
    setKey(null); // isi berubah → pesanan baru (kunci idempotensi baru)
  };
  async function submit(e: FormEvent) {
    e.preventDefault();
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
  return (
    <Shell title={tx("Keranjang")}>
      {cart.length === 0 ? (
        <p className="s-muted">{tx("Keranjang masih kosong.")}{" "}<a href="/">{tx("Lihat produk")}</a></p>
      ) : (
        <>
          <table className="shop-table">
            <thead><tr><th>{tx("Produk")}</th><th>{tx("Jumlah")}</th><th>{tx("Harga")}</th></tr></thead>
            <tbody>
              {cart.map((c) => (
                <tr key={c.variant_id}>
                  <td><a href={`/produk/${c.slug}`}>{c.name}</a>{c.variant_name !== "Default" && <div className="s-muted">{c.variant_name}</div>}</td>
                  <td>
                    <input aria-label={tx("Jumlah {0}", c.name)} type="number" min={0} max={999} value={c.quantity}
                      onChange={(e) => { setQuantity(c.variant_id, Math.max(0, Number(e.target.value) || 0)); setKey(null); }} />
                  </td>
                  <td>{rupiah(c.price)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="shop-total">{tx("Perkiraan total:")}{" "}<strong>{sen(subtotal + fee)}</strong>{fee > 0 && <span className="s-muted">{" "}<Tr s="(termasuk ongkir {0})" a={[sen(fee)]} /></span>}</p>
          <p className="s-muted">{tx("Total resmi dihitung toko saat pesanan dibuat.")}</p>
          {cfg && !cfg.checkout_enabled && <Msg kind="error">{tx("Toko belum menerima pesanan online.")}</Msg>}
          {cfg?.checkout_enabled && !cfg.pickup_available && !cfg.delivery_available && (
            <Msg kind="error">{tx("Toko belum mengatur cara pengambilan atau pengiriman, jadi pesanan online belum bisa dibuat. Silakan hubungi toko secara langsung.")}</Msg>
          )}
          {cfg?.checkout_enabled && (cfg.pickup_available || cfg.delivery_available) && (
            <form className="shop-form" onSubmit={submit}>
              <fieldset>
                <legend>{tx("Pengambilan")}</legend>
                {cfg.pickup_available && (
                  <label className="shop-check"><input type="radio" name="f" checked={form.fulfillment === "pickup"} onChange={() => set("fulfillment", "pickup")} />{" "}{tx("Ambil di toko")}</label>
                )}
                {cfg.delivery_available && (
                  <label className="shop-check"><input type="radio" name="f" checked={form.fulfillment === "delivery"} onChange={() => set("fulfillment", "delivery")} />{" "}<Tr s="Dikirim ({0})" a={[rupiah(cfg.delivery_fee)]} /></label>
                )}
              </fieldset>
              <label className="shop-field">{tx("Nama")}<input required maxLength={120} value={form.name} onChange={(e) => set("name", e.target.value)} autoComplete="name" /></label>
              <label className="shop-field">{tx("Email")}<input type="email" maxLength={254} value={form.email} onChange={(e) => set("email", e.target.value)} autoComplete="email" /></label>
              <label className="shop-field">{tx("Nomor WhatsApp/telepon")}<input type="tel" maxLength={20} value={form.phone} onChange={(e) => set("phone", e.target.value)} autoComplete="tel" /></label>
              {form.fulfillment === "delivery" && (
                <label className="shop-field">{tx("Alamat pengiriman")}<textarea required maxLength={500} value={form.delivery_address} onChange={(e) => set("delivery_address", e.target.value)} autoComplete="street-address" /></label>
              )}
              <label className="shop-field">{tx("Catatan (opsional)")}<textarea maxLength={500} value={form.note} onChange={(e) => set("note", e.target.value)} /></label>
              <label className="shop-check">
                <input type="checkbox" checked={form.accept} onChange={(e) => set("accept", e.target.checked)} required />
                {tx("Saya menyetujui")}{" "}<a href="/kebijakan-privasi" target="_blank" rel="noopener">{tx("kebijakan privasi")}</a> dan <a href="/syarat-transaksi" target="_blank" rel="noopener">{tx("syarat transaksi")}</a>.
              </label>
              {customer && <p className="s-muted"><Tr s="Masuk sebagai {0}." a={[customer.email]} /></p>}
              <Msg kind="error">{err}</Msg>
              <button type="submit" className="s-btn" disabled={busy || !form.accept}>{busy ? tx("Memproses…") : tx("Buat pesanan")}</button>
            </form>
          )}
          {!cfg && <Msg kind="error">{err}</Msg>}
        </>
      )}
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

export function OrderPage() {
  const token = useHashToken();
  const [order, setOrder] = useState<PublicOrder | null>(null);
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
    if (token) void load(token);
  }, [load, token]);
  // Ambil QRIS dinamis GoPay bila order menunggu bayar (idempoten; available=false bila tak dipakai).
  useEffect(() => {
    if (!token || order?.status !== "AWAITING_PAYMENT") return;
    shopApi<{ available: boolean; total_rupiah: number | null; qr_svg: string | null }>(
      "POST", "/orders/current/gopay-qris", undefined, { "x-order-token": token },
    ).then(setGopay, () => setGopay(null));
  }, [token, order?.status]);
  // P10: selama menunggu bayar, periksa status berkala agar auto-konfirmasi (GoPay) atau verifikasi
  // staf langsung terlihat pembeli tanpa menekan "Perbarui status". Berhenti saat status berubah.
  useEffect(() => {
    if (!token || order?.status !== "AWAITING_PAYMENT") return;
    const id = setInterval(() => { void load(token); }, 5000);
    return () => clearInterval(id);
  }, [token, order?.status, load]);
  if (token === undefined) return <Shell title={tx("Pesanan")}><p className="s-muted" aria-busy="true">{tx("Memuat…")}</p></Shell>;
  if (!token) {
    const saved = savedOrders();
    return (
      <Shell title={tx("Pesanan saya")}>
        {saved.length === 0 && <p className="s-muted">{tx("Belum ada pesanan di perangkat ini.")}</p>}
        <ul className="shop-list">
          {saved.map((o) => (
            <li key={o.code}><a href={`/pesanan#token=${encodeURIComponent(o.token)}`}>{o.code}</a> <span className="s-muted">{o.created_at.slice(0, 10)}</span></li>
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
      <p className="shop-status" data-status={order.status}>{STATUS_TEXT[order.status] ?? order.status}</p>
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
      <table className="shop-table">
        <thead><tr><th>{tx("Produk")}</th><th>{tx("Jumlah")}</th><th>{tx("Subtotal")}</th></tr></thead>
        <tbody>
          {order.items.map((i, k) => (
            <tr key={k}><td>{i.product_name}{i.variant_name !== "Default" && <div className="s-muted">{i.variant_name}</div>}</td><td>{i.quantity}</td><td>{rupiah(i.line_total)}</td></tr>
          ))}
        </tbody>
      </table>
      {order.delivery_fee !== "0.00" && <p>{tx("Ongkir:")}{" "}{rupiah(order.delivery_fee)}</p>}
      <p className="shop-total">{tx("Total:")}{" "}<strong>{rupiah(order.total)}</strong></p>
      <p className="s-muted">{order.fulfillment === "delivery" ? tx("Dikirim ke: {0}", order.delivery_address ?? "") : tx("Ambil di toko")}</p>
      <button type="button" className="d-link" onClick={() => token && load(token)}>{tx("Perbarui status")}</button>
      <OrderTickets token={token} />
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
  const [me, setMe] = useState<CustomerSession | null>(null);
  const [ready, setReady] = useState(false);
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
    refresh().catch(() => setReady(true));
  }, [refresh]);
  async function submit(e: FormEvent) {
    e.preventDefault();
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
        <p>{me.email} {me.email_verified ? tx("(terverifikasi)") : tx("(belum terverifikasi)")}</p>
        {!me.email_verified && <p className="s-muted">{tx("Verifikasi email untuk melihat pesanan yang dibuat dengan email ini sebelum Anda mendaftar.")}</p>}
        <h2>{tx("Riwayat pesanan")}</h2>
        {orders.length === 0 && <p className="s-muted">{tx("Belum ada pesanan.")}</p>}
        <ul className="shop-list">
          {orders.map((o) => <li key={o.id}>{o.code} — {STATUS_TEXT[o.status] ?? o.status} — {rupiah(o.total)}</li>)}
        </ul>
        <button type="button" className="s-btn" onClick={async () => { await shopApi("POST", "/customer/logout").catch(() => null); setCustomerCsrf(null); setMe(null); }}>{tx("Keluar")}</button>
      </Shell>
    );
  }
  return (
    <Shell title={mode === "login" ? tx("Masuk") : tx("Daftar akun")}>
      <p className="s-muted">{tx("Akun bersifat opsional: Anda tetap bisa memesan tanpa akun.")}</p>
      <form className="shop-form" onSubmit={submit}>
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
      </form>
      <button type="button" className="d-link" onClick={() => setMode(mode === "login" ? "register" : "login")}>
        {mode === "login" ? tx("Belum punya akun? Daftar") : tx("Sudah punya akun? Masuk")}
      </button>
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
