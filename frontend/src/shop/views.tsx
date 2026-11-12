"use client";
/* eslint-disable @next/next/no-html-link-for-pages, @next/next/no-location-assign-relative-destination --
   situs toko: SEMUA path dilayani shell site.html oleh Caddy (ADR-0014). next/link akan melakukan
   navigasi klien ke rute Next (mis. "/" = dashboard), jadi tautan HTML biasa yang benar. */

// Halaman toko (P3): produk, keranjang + checkout, status pesanan + pembayaran QRIS statis
// (ADR-0004), akun customer opsional (ADR-0005). Semua teks dirender sebagai teks (React).

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";

import { ApiError } from "@/lib/api";

import {
  STATUS_TEXT, fileUrl, loadCustomer, rupiah, setCustomerCsrf, shopApi,
  type CheckoutConfig, type CustomerSession, type OrderSummary, type PublicOrder, type PublicProduct,
} from "./api";
import { addToCart, cartCount, clearCart, displaySubtotalSen, getCart, saveOrder, savedOrders, setQuantity, type CartItem } from "./cart";

const ERR: Record<string, string> = {
  required: "wajib diisi",
  invalid_email: "email tidak valid",
  invalid_phone: "nomor telepon tidak valid",
  email_or_phone_required: "isi email atau nomor telepon",
  invalid_quantity: "jumlah tidak valid",
  product_unavailable: "produk sudah tidak tersedia",
  fulfillment_unavailable: "metode pengambilan tidak tersedia",
  too_long: "terlalu panjang",
  weak_password: "password minimal 8 karakter dan tidak sama dengan email",
};

function explain(e: unknown): string {
  if (e instanceof ApiError) {
    const f = e.fieldErrors;
    if (f.length) return f.map((x) => `${x.path}: ${ERR[x.code] ?? x.code}`).join("; ");
    const reason = (e.details as { reason?: string } | null)?.reason;
    if (reason === "insufficient_stock") return "Stok tidak mencukupi untuk salah satu produk. Kurangi jumlahnya.";
    if (reason === "payment_not_configured") return "Toko belum menerima pesanan online.";
    if (reason === "email_taken") return "Email sudah terdaftar di toko ini. Silakan masuk.";
    if (e.status === 429) return "Terlalu banyak percobaan. Coba lagi beberapa menit lagi.";
    return e.message;
  }
  return "Tidak dapat menghubungi toko. Periksa koneksi Anda.";
}

function Shell({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="shop">
      <nav className="shop-nav" aria-label="Navigasi toko">
        <a href="/">← Kembali ke toko</a>
        <span className="shop-nav-links">
          <a href="/keranjang">Keranjang</a>
          <a href="/pesanan">Pesanan</a>
          <a href="/akun">Akun</a>
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
  return <a className="shop-fab" href="/keranjang">Keranjang ({n})</a>;
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
      .catch((e) => setErr(e instanceof ApiError && e.status === 404 ? "Produk tidak ditemukan." : explain(e)));
  }, [slug]);
  const variant = useMemo(
    () => p?.variants.find((v) => v.option_values.length === pick.length && v.option_values.every((x, i) => x === pick[i])),
    [p, pick],
  );
  if (err) return <Shell title="Produk"><Msg kind="error">{err}</Msg></Shell>;
  if (!p) return <Shell title="Produk"><p className="s-muted" aria-busy="true">Memuat…</p></Shell>;
  const sellable = p.available && !!variant;
  return (
    <Shell title={p.name}>
      <div className="shop-product">
        <div className="shop-gallery">
          {p.images.map((src, i) => (
            // eslint-disable-next-line @next/next/no-img-element -- static export, gambar bertanda tangan
            <img key={i} src={fileUrl(src)} alt={i === 0 ? p.name : ""} loading={i === 0 ? "eager" : "lazy"} />
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
          {!variant && p.options.length > 0 && <Msg kind="error">Kombinasi ini tidak tersedia.</Msg>}
          {!p.available && <Msg kind="error">Produk sedang tidak tersedia.</Msg>}
          <label className="shop-field">
            Jumlah
            <input type="number" min={1} max={999} value={qty} onChange={(e) => setQty(Math.max(1, Math.min(999, Number(e.target.value) || 1)))} />
          </label>
          <button
            className="s-btn"
            disabled={!sellable}
            onClick={() => {
              if (!variant) return;
              addToCart({ variant_id: variant.id, slug: p.slug, name: p.name, variant_name: variant.name, price: variant.promo_price ?? variant.price }, qty);
              setAdded(true);
            }}
          >
            Tambah ke keranjang
          </button>
          <Msg kind="ok">{added ? <>Ditambahkan. <a href="/keranjang">Lihat keranjang</a></> : null}</Msg>
        </div>
      </div>
    </Shell>
  );
}

// ------------------------------------------------------------------ keranjang & checkout

export function CartPage() {
  const [cart, setCart] = useState<CartItem[]>([]);
  const [cfg, setCfg] = useState<CheckoutConfig | null>(null);
  const [customer, setCustomer] = useState<CustomerSession | null>(null);
  const [form, setForm] = useState({ fulfillment: "pickup", name: "", email: "", phone: "", delivery_address: "", note: "", accept: false });
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
    <Shell title="Keranjang">
      {cart.length === 0 ? (
        <p className="s-muted">Keranjang masih kosong. <a href="/">Lihat produk</a></p>
      ) : (
        <>
          <table className="shop-table">
            <thead><tr><th>Produk</th><th>Jumlah</th><th>Harga</th></tr></thead>
            <tbody>
              {cart.map((c) => (
                <tr key={c.variant_id}>
                  <td><a href={`/produk/${c.slug}`}>{c.name}</a>{c.variant_name !== "Default" && <div className="s-muted">{c.variant_name}</div>}</td>
                  <td>
                    <input aria-label={`Jumlah ${c.name}`} type="number" min={0} max={999} value={c.quantity}
                      onChange={(e) => { setQuantity(c.variant_id, Math.max(0, Number(e.target.value) || 0)); setKey(null); }} />
                  </td>
                  <td>{rupiah(c.price)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="shop-total">Perkiraan total: <strong>{sen(subtotal + fee)}</strong>{fee > 0 && <span className="s-muted"> (termasuk ongkir {sen(fee)})</span>}</p>
          <p className="s-muted">Total resmi dihitung toko saat pesanan dibuat.</p>
          {cfg && !cfg.checkout_enabled && <Msg kind="error">Toko belum menerima pesanan online.</Msg>}
          {cfg?.checkout_enabled && (
            <form className="shop-form" onSubmit={submit}>
              <fieldset>
                <legend>Pengambilan</legend>
                {cfg.pickup_available && (
                  <label className="shop-check"><input type="radio" name="f" checked={form.fulfillment === "pickup"} onChange={() => set("fulfillment", "pickup")} /> Ambil di toko</label>
                )}
                {cfg.delivery_available && (
                  <label className="shop-check"><input type="radio" name="f" checked={form.fulfillment === "delivery"} onChange={() => set("fulfillment", "delivery")} /> Dikirim ({rupiah(cfg.delivery_fee)})</label>
                )}
              </fieldset>
              <label className="shop-field">Nama<input required maxLength={120} value={form.name} onChange={(e) => set("name", e.target.value)} autoComplete="name" /></label>
              <label className="shop-field">Email<input type="email" maxLength={254} value={form.email} onChange={(e) => set("email", e.target.value)} autoComplete="email" /></label>
              <label className="shop-field">Nomor WhatsApp/telepon<input type="tel" maxLength={20} value={form.phone} onChange={(e) => set("phone", e.target.value)} autoComplete="tel" /></label>
              {form.fulfillment === "delivery" && (
                <label className="shop-field">Alamat pengiriman<textarea required maxLength={500} value={form.delivery_address} onChange={(e) => set("delivery_address", e.target.value)} autoComplete="street-address" /></label>
              )}
              <label className="shop-field">Catatan (opsional)<textarea maxLength={500} value={form.note} onChange={(e) => set("note", e.target.value)} /></label>
              <label className="shop-check">
                <input type="checkbox" checked={form.accept} onChange={(e) => set("accept", e.target.checked)} required />
                Saya menyetujui <a href="/kebijakan-privasi" target="_blank" rel="noopener">kebijakan privasi</a> dan <a href="/syarat-transaksi" target="_blank" rel="noopener">syarat transaksi</a>.
              </label>
              {customer && <p className="s-muted">Masuk sebagai {customer.email}.</p>}
              <Msg kind="error">{err}</Msg>
              <button className="s-btn" disabled={busy || !form.accept}>{busy ? "Memproses…" : "Buat pesanan"}</button>
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
  const load = useCallback(
    (t: string) =>
      shopApi<PublicOrder>("GET", "/orders/current", undefined, { "x-order-token": t }).then(
        (o) => {
          setOrder(o);
          setErr(null);
        },
        (e) => setErr(e instanceof ApiError && e.status === 404 ? "Pesanan tidak ditemukan." : explain(e)),
      ),
    [],
  );
  useEffect(() => {
    if (token) void load(token);
  }, [load, token]);
  if (token === undefined) return <Shell title="Pesanan"><p className="s-muted" aria-busy="true">Memuat…</p></Shell>;
  if (!token) {
    const saved = savedOrders();
    return (
      <Shell title="Pesanan saya">
        {saved.length === 0 && <p className="s-muted">Belum ada pesanan di perangkat ini.</p>}
        <ul className="shop-list">
          {saved.map((o) => (
            <li key={o.code}><a href={`/pesanan#token=${encodeURIComponent(o.token)}`}>{o.code}</a> <span className="s-muted">{o.created_at.slice(0, 10)}</span></li>
          ))}
        </ul>
        <p className="s-muted">Punya akun? <a href="/akun">Lihat riwayat di akun</a>.</p>
      </Shell>
    );
  }
  async function claim() {
    if (!token) return;
    setBusy(true);
    setErr(null);
    try {
      setOrder(await shopApi<PublicOrder>("POST", "/orders/current/payment-claim", { json: { payer_reference: ref || null } }, { "x-order-token": token }));
      setOk("Terima kasih. Toko akan memverifikasi pembayaran Anda.");
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
      await shopApi("POST", "/orders/current/payment-proof", { bytes: file }, { "x-order-token": token });
      setOk("Bukti pembayaran terkirim.");
      await load(token);
    } catch (e) {
      setErr(explain(e));
    } finally {
      setBusy(false);
    }
  }
  if (!order) return <Shell title="Pesanan"><Msg kind="error">{err}</Msg>{!err && <p className="s-muted" aria-busy="true">Memuat…</p>}</Shell>;
  const p = order.payment;
  const waiting = order.status === "AWAITING_PAYMENT" && p && (p.status === "pending" || p.status === "claimed");
  return (
    <Shell title={`Pesanan ${order.code}`}>
      <p className="shop-status" data-status={order.status}>{STATUS_TEXT[order.status] ?? order.status}</p>
      <Msg kind="ok">{ok}</Msg>
      <Msg kind="error">{err}</Msg>
      {waiting && (
        <section className="shop-pay" aria-label="Pembayaran QRIS">
          <h2>Bayar dengan QRIS</h2>
          <p>Pindai kode QRIS berikut dengan aplikasi pembayaran Anda, lalu bayar <strong>tepat {rupiah(p.amount)}</strong>.</p>
          {p.qris_url && (
            // eslint-disable-next-line @next/next/no-img-element -- QRIS toko, signed URL
            <img className="shop-qris" src={fileUrl(p.qris_url)} alt={`QRIS ${p.receiver_name ?? "toko"}`} />
          )}
          <p className="s-muted">Penerima: {p.receiver_name}. Cantumkan kode pesanan {order.code} bila aplikasi meminta catatan.</p>
          <p className="s-muted">Bayar sebelum {new Date(order.expires_at).toLocaleString("id-ID")}.</p>
          {p.status === "claimed" ? (
            <p>Anda sudah menandai pembayaran. Toko sedang memverifikasi.</p>
          ) : (
            <>
              <label className="shop-field">Nama pembayar / catatan (opsional)<input maxLength={120} value={ref} onChange={(e) => setRef(e.target.value)} /></label>
              <button className="s-btn" onClick={claim} disabled={busy}>Saya sudah membayar</button>
            </>
          )}
          <label className="shop-field">Unggah bukti pembayaran (opsional, PNG/JPEG/WebP)
            <input type="file" accept="image/png,image/jpeg,image/webp" disabled={busy}
              onChange={(e) => { const f = e.target.files?.[0]; if (f) void proof(f); }} />
          </label>
        </section>
      )}
      {p?.status === "rejected" && <Msg kind="error">Pembayaran ditolak toko: {p.rejection_reason}</Msg>}
      <table className="shop-table">
        <thead><tr><th>Produk</th><th>Jumlah</th><th>Subtotal</th></tr></thead>
        <tbody>
          {order.items.map((i, k) => (
            <tr key={k}><td>{i.product_name}{i.variant_name !== "Default" && <div className="s-muted">{i.variant_name}</div>}</td><td>{i.quantity}</td><td>{rupiah(i.line_total)}</td></tr>
          ))}
        </tbody>
      </table>
      {order.delivery_fee !== "0.00" && <p>Ongkir: {rupiah(order.delivery_fee)}</p>}
      <p className="shop-total">Total: <strong>{rupiah(order.total)}</strong></p>
      <p className="s-muted">{order.fulfillment === "delivery" ? `Dikirim ke: ${order.delivery_address ?? ""}` : "Ambil di toko"}</p>
      <button className="d-link" onClick={() => token && load(token)}>Perbarui status</button>
    </Shell>
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
        setOk("Akun dibuat. Periksa email Anda untuk verifikasi, lalu masuk.");
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
  if (!ready) return <Shell title="Akun"><p className="s-muted" aria-busy="true">Memuat…</p></Shell>;
  if (me) {
    return (
      <Shell title="Akun saya">
        <p>{me.email} {me.email_verified ? "(terverifikasi)" : "(belum terverifikasi)"}</p>
        {!me.email_verified && <p className="s-muted">Verifikasi email untuk melihat pesanan yang dibuat dengan email ini sebelum Anda mendaftar.</p>}
        <h2>Riwayat pesanan</h2>
        {orders.length === 0 && <p className="s-muted">Belum ada pesanan.</p>}
        <ul className="shop-list">
          {orders.map((o) => <li key={o.id}>{o.code} — {STATUS_TEXT[o.status] ?? o.status} — {rupiah(o.total)}</li>)}
        </ul>
        <button className="s-btn" onClick={async () => { await shopApi("POST", "/customer/logout").catch(() => null); setCustomerCsrf(null); setMe(null); }}>Keluar</button>
      </Shell>
    );
  }
  return (
    <Shell title={mode === "login" ? "Masuk" : "Daftar akun"}>
      <p className="s-muted">Akun bersifat opsional: Anda tetap bisa memesan tanpa akun.</p>
      <form className="shop-form" onSubmit={submit}>
        <label className="shop-field">Email<input type="email" required value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} autoComplete="email" /></label>
        <label className="shop-field">Password<input type="password" required minLength={8} value={f.password} onChange={(e) => setF({ ...f, password: e.target.value })} autoComplete={mode === "login" ? "current-password" : "new-password"} /></label>
        {mode === "register" && (
          <label className="shop-check">
            <input type="checkbox" checked={f.accept} onChange={(e) => setF({ ...f, accept: e.target.checked })} required />
            Saya menyetujui <a href="/kebijakan-privasi" target="_blank" rel="noopener">kebijakan privasi</a> dan <a href="/syarat-transaksi" target="_blank" rel="noopener">syarat</a>.
          </label>
        )}
        <Msg kind="error">{err}</Msg>
        <Msg kind="ok">{ok}</Msg>
        <button className="s-btn">{mode === "login" ? "Masuk" : "Daftar"}</button>
      </form>
      <button className="d-link" onClick={() => setMode(mode === "login" ? "register" : "login")}>
        {mode === "login" ? "Belum punya akun? Daftar" : "Sudah punya akun? Masuk"}
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
    <Shell title="Verifikasi email">
      {state === "busy" && <p className="s-muted" aria-busy="true">Memverifikasi…</p>}
      {state === "ok" && <Msg kind="ok">Email terverifikasi{linked > 0 ? ` dan ${linked} riwayat pesanan ditautkan` : ""}. <a href="/akun">Masuk ke akun</a></Msg>}
      {state === "bad" && <Msg kind="error">Tautan verifikasi tidak valid atau kedaluwarsa.</Msg>}
    </Shell>
  );
}

/** Teks hukum transaksi (D-12): placeholder sampai teks final hasil review legal (sebelum go-live). */
export function LegalPage({ kind }: { kind: "privacy" | "terms" }) {
  return (
    <Shell title={kind === "privacy" ? "Kebijakan privasi" : "Syarat transaksi"}>
      <p><strong>Draf — belum berlaku final.</strong> Teks ini placeholder dan akan diganti versi yang telah ditinjau sebelum layanan dibuka untuk umum.</p>
      {kind === "privacy" ? (
        <p>Data yang Anda isi saat memesan (nama, email/telepon, alamat) dipakai toko untuk memproses dan mengonfirmasi pesanan, dan dihapus mengikuti kebijakan retensi.</p>
      ) : (
        <p>Pembayaran dilakukan langsung ke QRIS milik toko. Pesanan diproses setelah toko memverifikasi pembayaran. Pesanan yang tidak dibayar sebelum batas waktu dibatalkan otomatis.</p>
      )}
    </Shell>
  );
}
