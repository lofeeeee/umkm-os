"use client";

// Katalog produk (P3-1): daftar, editor (varian/opsi/foto), status, kategori.

import { useCallback, useEffect, useState } from "react";

import { rupiah, useUploads } from "@/dashboard/assets";
import { Assets, ImagePicker } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Variant { id?: string; sku: string; option_values: string[]; price: string; promo_price: string | null; is_active: boolean; name?: string }
interface Product {
  id: string; name: string; slug: string; description: string | null; category_id: string | null; status: string;
  available: boolean; options: { name: string; values: string[] }[]; variants: Variant[]; images: string[];
}
interface Category { id: string; parent_id: string | null; name: string; position: number }

const STATUS: Record<string, string> = { draft: "Draf", active: "Aktif", archived: "Arsip" };

export default function ProductsPage() {
  return <WithTenant>{(ctx) => <Catalog ctx={ctx} />}</WithTenant>;
}

function emptyProduct(): Product {
  return {
    id: "", name: "", slug: "", description: null, category_id: null, status: "draft", available: true,
    options: [], variants: [{ sku: "", option_values: [], price: "", promo_price: null, is_active: true }], images: [],
  };
}

function Catalog({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [items, setItems] = useState<Product[]>([]);
  const [cats, setCats] = useState<Category[]>([]);
  const [q, setQ] = useState("");
  const [status, setStatus] = useState("");
  const [edit, setEdit] = useState<Product | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const assets = useUploads(ctx.tenant.id, ctx.can("products.update") || ctx.can("products.create"));

  const load = useCallback(() => {
    const p = new URLSearchParams({ limit: "100" });
    if (q) p.set("q", q);
    if (status) p.set("status", status);
    return Promise.all([
      api<{ items: Product[] }>("GET", `${base}/products?${p}`),
      api<Category[]>("GET", `${base}/categories`),
    ]).then(([r, c]) => { setItems(r.items); setCats(c); }, (e) => setErr(describe(e)));
  }, [base, q, status]);
  useEffect(() => { void load(); }, [load]);

  async function setProductStatus(p: Product, to: string) {
    setErr(null);
    try {
      await api("POST", `${base}/products/${p.id}/status`, { json: { status: to } });
      await load();
    } catch (e) { setErr(describe(e)); }
  }

  return (
    <Assets.Provider value={assets}>
      <h1>Produk</h1>
      <Alert kind="error">{err}</Alert>
      {edit ? (
        <Editor ctx={ctx} base={base} initial={edit} cats={cats} onDone={async () => { setEdit(null); await load(); }} />
      ) : (
        <>
          <div className="d-row" style={{ marginBottom: 12 }}>
            <input aria-label="Cari produk" placeholder="Cari nama / SKU" value={q} onChange={(e) => setQ(e.target.value)} />
            <select aria-label="Filter status" value={status} onChange={(e) => setStatus(e.target.value)}>
              <option value="">Semua status</option>
              <option value="draft">Draf</option>
              <option value="active">Aktif</option>
              <option value="archived">Arsip</option>
            </select>
            {ctx.can("products.create") && <button className="d-btn d-btn-primary" onClick={() => setEdit(emptyProduct())}>Tambah produk</button>}
          </div>
          <div className="d-card" style={{ overflowX: "auto" }}>
            <table className="d-table">
              <thead><tr><th>Produk</th><th>Harga</th><th>Status</th><th>Aksi</th></tr></thead>
              <tbody>
                {items.map((p) => (
                  <tr key={p.id}>
                    <td>{p.name}<div className="d-muted">{p.variants.map((v) => v.sku).join(", ")}</div></td>
                    <td>{rupiah(p.variants[0]?.promo_price ?? p.variants[0]?.price)}</td>
                    <td>{STATUS[p.status] ?? p.status}{!p.available && " · tidak tersedia"}</td>
                    <td className="d-row">
                      {ctx.can("products.update") && <button className="d-btn d-btn-sm" onClick={() => setEdit(p)}>Ubah</button>}
                      {p.status !== "active" && ctx.can("products.update") && <button className="d-btn d-btn-sm" onClick={() => setProductStatus(p, "active")}>Aktifkan</button>}
                      {p.status === "active" && ctx.can("products.archive") && <button className="d-btn d-btn-sm d-btn-danger" onClick={() => setProductStatus(p, "archived")}>Arsipkan</button>}
                    </td>
                  </tr>
                ))}
                {items.length === 0 && <tr><td colSpan={4} className="d-muted">Belum ada produk.</td></tr>}
              </tbody>
            </table>
          </div>
          <Categories ctx={ctx} base={base} cats={cats} reload={load} />
        </>
      )}
    </Assets.Provider>
  );
}

function Editor({ ctx, base, initial, cats, onDone }: { ctx: TenantCtx; base: string; initial: Product; cats: Category[]; onDone: () => void }) {
  const [p, setP] = useState<Product>(initial);
  const [optText, setOptText] = useState(initial.options.map((o) => `${o.name}: ${o.values.join(", ")}`).join("\n"));
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const options = optText
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => {
      const [name, vals = ""] = l.split(":");
      return { name: (name ?? "").trim(), values: vals.split(",").map((v) => v.trim()).filter(Boolean) };
    });
  const setVariant = (i: number, v: Partial<Variant>) => setP({ ...p, variants: p.variants.map((x, j) => (j === i ? { ...x, ...v } : x)) });

  async function save() {
    setBusy(true);
    setErr(null);
    const body = {
      name: p.name, slug: p.slug || null, description: p.description, category_id: p.category_id, available: p.available,
      options,
      variants: p.variants.map((v) => ({
        id: v.id, sku: v.sku, option_values: options.map((_, i) => v.option_values[i] ?? ""), price: v.price,
        promo_price: v.promo_price || null, is_active: v.is_active,
      })),
      images: p.images,
    };
    try {
      if (p.id) await api("PUT", `${base}/products/${p.id}`, { json: body });
      else await api("POST", `${base}/products`, { json: body });
      onDone();
    } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }

  return (
    <div className="d-card">
      <h2>{p.id ? "Ubah produk" : "Produk baru (draf)"}</h2>
      <Alert kind="error">{err}</Alert>
      <div className="d-grid">
        <label className="d-field"><span>Nama</span><input value={p.name} maxLength={120} onChange={(e) => setP({ ...p, name: e.target.value })} /></label>
        <label className="d-field"><span>Slug URL (kosong = otomatis)</span><input value={p.slug} maxLength={80} onChange={(e) => setP({ ...p, slug: e.target.value })} /></label>
        <label className="d-field"><span>Kategori</span>
          <select value={p.category_id ?? ""} onChange={(e) => setP({ ...p, category_id: e.target.value || null })}>
            <option value="">Tanpa kategori</option>
            {cats.map((c) => <option key={c.id} value={c.id}>{c.parent_id ? `— ${c.name}` : c.name}</option>)}
          </select>
        </label>
        <label className="d-check"><input type="checkbox" checked={p.available} onChange={(e) => setP({ ...p, available: e.target.checked })} /> Tersedia untuk dijual</label>
      </div>
      <label className="d-field"><span>Deskripsi</span><textarea rows={4} maxLength={5000} value={p.description ?? ""} onChange={(e) => setP({ ...p, description: e.target.value || null })} /></label>
      <label className="d-field">
        <span>Opsi varian (satu per baris, mis. <code>Ukuran: S, M, L</code>; kosong = satu varian)</span>
        <textarea rows={3} value={optText} onChange={(e) => setOptText(e.target.value)} />
      </label>
      <h3>Varian</h3>
      <div style={{ overflowX: "auto" }}>
        <table className="d-table">
          <thead><tr><th>SKU</th>{options.map((o) => <th key={o.name}>{o.name}</th>)}<th>Harga</th><th>Harga promo</th><th>Aktif</th><th /></tr></thead>
          <tbody>
            {p.variants.map((v, i) => (
              <tr key={v.id ?? `new-${i}`}>
                <td><input aria-label="SKU" value={v.sku} maxLength={64} onChange={(e) => setVariant(i, { sku: e.target.value })} /></td>
                {options.map((o, k) => (
                  <td key={o.name}>
                    <select aria-label={o.name} value={v.option_values[k] ?? ""} onChange={(e) => {
                      const vals = options.map((_, j) => v.option_values[j] ?? "");
                      vals[k] = e.target.value;
                      setVariant(i, { option_values: vals });
                    }}>
                      <option value="">—</option>
                      {o.values.map((x) => <option key={x} value={x}>{x}</option>)}
                    </select>
                  </td>
                ))}
                <td><input aria-label="Harga" inputMode="decimal" value={v.price} onChange={(e) => setVariant(i, { price: e.target.value })} /></td>
                <td><input aria-label="Harga promo" inputMode="decimal" value={v.promo_price ?? ""} onChange={(e) => setVariant(i, { promo_price: e.target.value || null })} /></td>
                <td><input aria-label="Aktif" type="checkbox" checked={v.is_active} onChange={(e) => setVariant(i, { is_active: e.target.checked })} /></td>
                <td>{p.variants.length > 1 && <button className="d-btn d-btn-sm d-btn-danger" onClick={() => setP({ ...p, variants: p.variants.filter((_, j) => j !== i) })}>Hapus</button>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {options.length > 0 && (
        <button className="d-btn d-btn-sm" onClick={() => setP({ ...p, variants: [...p.variants, { sku: "", option_values: [], price: p.variants[0]?.price ?? "", promo_price: null, is_active: true }] })}>
          Tambah varian
        </button>
      )}
      <h3>Foto (maks 10)</h3>
      {p.images.map((r, i) => (
        <ImagePicker key={`${r}-${i}`} label={`Foto ${i + 1}`} value={r}
          onChange={(v) => setP({ ...p, images: v ? p.images.map((x, j) => (j === i ? v : x)) : p.images.filter((_, j) => j !== i) })} />
      ))}
      {p.images.length < 10 && <ImagePicker label="Tambah foto" value={undefined} onChange={(v) => v && setP({ ...p, images: [...p.images, v] })} />}
      <div className="d-row" style={{ marginTop: 16 }}>
        <button className="d-btn d-btn-primary" onClick={save} disabled={busy || !(ctx.can("products.update") || (!p.id && ctx.can("products.create")))}>Simpan</button>
        <button className="d-btn" onClick={onDone}>Batal</button>
      </div>
    </div>
  );
}

function Categories({ ctx, base, cats, reload }: { ctx: TenantCtx; base: string; cats: Category[]; reload: () => Promise<unknown> }) {
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const can = ctx.can("products.update");
  async function run(f: () => Promise<unknown>) {
    setErr(null);
    try { await f(); await reload(); } catch (e) { setErr(describe(e)); }
  }
  return (
    <div className="d-card">
      <h2>Kategori</h2>
      <Alert kind="error">{err}</Alert>
      <ul>
        {cats.map((c) => (
          <li key={c.id}>
            {c.parent_id ? "— " : ""}{c.name}{" "}
            {can && <button className="d-btn d-btn-sm d-btn-danger" onClick={() => run(() => api("DELETE", `${base}/categories/${c.id}`))}>Hapus</button>}
          </li>
        ))}
      </ul>
      {can && (
        <div className="d-row">
          <input aria-label="Nama kategori" placeholder="Nama kategori" value={name} maxLength={60} onChange={(e) => setName(e.target.value)} />
          <select aria-label="Induk" value={parent} onChange={(e) => setParent(e.target.value)}>
            <option value="">Kategori utama</option>
            {cats.filter((c) => !c.parent_id).map((c) => <option key={c.id} value={c.id}>Sub dari {c.name}</option>)}
          </select>
          <button className="d-btn" onClick={() => run(async () => { await api("POST", `${base}/categories`, { json: { name, parent_id: parent || null } }); setName(""); })}>Tambah</button>
        </div>
      )}
    </div>
  );
}
