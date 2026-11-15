"use client";

// Katalog produk (P3-1): daftar, editor (varian/opsi/foto), status, kategori.
// Alur: daftar tetap terlihat; tambah/ubah produk di panel samping (Sheet). `?new=1` membuka
// panel produk baru (dipakai Beranda & ⌘K).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Image as ImageIcon, Package, Plus, Tag, Trash, X } from "@phosphor-icons/react/ssr";

import { useUploads } from "@/dashboard/assets";
import { Assets, ImagePicker } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { PRODUCT_STATUS, rupiah, status } from "@/lib/labels";
import { confirm, Sheet } from "@/ui/dialog";
import { RowMenu } from "@/ui/menu";
import { Badge, BusyButton, Card, EmptyState, PageHeader, SearchInput, Skeleton, Tabs } from "@/ui/page";
import { sized } from "@/renderer/safe";
import { useFieldErrors } from "@/ui/form";
import { toast } from "@/ui/toast";

interface Variant { id?: string; sku: string; option_values: string[]; price: string; promo_price: string | null; is_active: boolean; name?: string }
interface Option { name: string; values: string[] }
interface Product {
  id: string; name: string; slug: string; description: string | null; category_id: string | null; status: string;
  available: boolean; options: Option[]; variants: Variant[]; images: string[];
}
interface Category { id: string; parent_id: string | null; name: string; position: number }

type StatusFilter = "" | "active" | "draft" | "archived";

export default function ProductsPage() {
  return <WithTenant>{(ctx) => <Catalog ctx={ctx} />}</WithTenant>;
}

function emptyProduct(): Product {
  return {
    id: "", name: "", slug: "", description: null, category_id: null, status: "draft", available: true,
    options: [], variants: [{ sku: "", option_values: [], price: "", promo_price: null, is_active: true }], images: [],
  };
}

const wantsNew = () => typeof window !== "undefined" && new URLSearchParams(window.location.search).get("new") === "1";

function priceRange(p: Product): string {
  const prices = p.variants.filter((v) => v.is_active).map((v) => Number(v.promo_price ?? v.price)).filter(Number.isFinite);
  if (prices.length === 0) return "—";
  const lo = Math.min(...prices);
  const hi = Math.max(...prices);
  return lo === hi ? rupiah(lo) : `${rupiah(lo)} – ${rupiah(hi)}`;
}

function Catalog({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const [items, setItems] = useState<Product[] | null>(null);
  const [cats, setCats] = useState<Category[]>([]);
  const [q, setQ] = useState("");
  const [tab, setTab] = useState<StatusFilter>("");
  const [edit, setEdit] = useState<Product | null>(() => (wantsNew() ? emptyProduct() : null));
  const [catsOpen, setCatsOpen] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const assets = useUploads(ctx.tenant.id, ctx.can("products.update") || ctx.can("products.create"));
  const canCreate = ctx.can("products.create");

  const load = useCallback(() => {
    const p = new URLSearchParams({ limit: "100" });
    if (q) p.set("q", q);
    return Promise.all([
      api<{ items: Product[] }>("GET", `${base}/products?${p}`),
      api<Category[]>("GET", `${base}/categories`),
    ]).then(([r, c]) => { setItems(r.items); setCats(c); setErr(null); }, (e) => setErr(describe(e)));
  }, [base, q]);
  useEffect(() => {
    const t = setTimeout(() => void load(), q ? 250 : 0);
    return () => clearTimeout(t);
  }, [load, q]);

  const counts = useMemo(() => {
    const c = { active: 0, draft: 0, archived: 0 };
    for (const p of items ?? []) if (p.status in c) c[p.status as keyof typeof c]++;
    return c;
  }, [items]);
  const shown = (items ?? []).filter((p) => !tab || p.status === tab);
  const catName = (id: string | null) => cats.find((c) => c.id === id)?.name;

  async function setProductStatus(p: Product, to: string) {
    if (to === "archived") {
      const ok = await confirm({
        title: `Arsipkan “${p.name}”?`,
        body: "Produk disembunyikan dari website dan tidak bisa dipesan. Anda bisa mengaktifkannya lagi kapan saja.",
        confirmLabel: "Arsipkan",
        danger: true,
      });
      if (!ok) return;
    }
    try {
      await api("POST", `${base}/products/${p.id}/status`, { json: { status: to } });
      await load();
      toast.ok(to === "active" ? `“${p.name}” aktif dan tampil di website.` : `“${p.name}” diarsipkan.`);
    } catch (e) { toast.error(describe(e)); }
  }

  const closeEditor = () => {
    setEdit(null);
    if (wantsNew()) {
      const url = new URL(window.location.href);
      url.searchParams.delete("new");
      window.history.replaceState(null, "", url.toString());
    }
  };

  return (
    <Assets.Provider value={assets}>
      <PageHeader
        title="Produk"
        description="Kelola katalog yang tampil di website toko. Produk berstatus Aktif bisa dipesan pembeli."
        actions={
          <>
            <button type="button" className="d-btn" onClick={() => setCatsOpen(true)}><Tag weight="bold" aria-hidden="true" />Kategori</button>
            {canCreate && <button type="button" className="d-btn d-btn-primary" onClick={() => setEdit(emptyProduct())}><Plus weight="bold" aria-hidden="true" />Tambah produk</button>}
          </>
        }
      />
      <Alert kind="error">{err}</Alert>

      <Tabs
        label="Filter status produk"
        value={tab}
        onChange={setTab}
        items={[
          { value: "", label: "Semua", count: items?.length },
          { value: "active", label: "Aktif", count: counts.active },
          { value: "draft", label: "Draf", count: counts.draft },
          { value: "archived", label: "Arsip", count: counts.archived },
        ]}
      />
      <div className="d-toolbar">
        <SearchInput label="Cari produk" placeholder="Cari nama atau SKU…" value={q} onChange={setQ} />
      </div>

      <div className="d-table-wrap">
        {items === null ? <div style={{ padding: 20 }}><Skeleton lines={5} /></div> : shown.length === 0 ? (
          <EmptyState
            icon={Package}
            title={q || tab ? "Tidak ada produk yang cocok" : "Belum ada produk"}
            action={!q && !tab && canCreate ? <button type="button" className="d-btn d-btn-primary" onClick={() => setEdit(emptyProduct())}>Tambah produk pertama</button> : undefined}
          >
            {q || tab ? "Coba kata kunci lain atau pilih tab Semua." : "Tambahkan produk agar tampil di website dan bisa dipesan pembeli."}
          </EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>Produk</th><th className="d-hide-sm">Kategori</th><th className="d-num">Harga</th><th>Status</th><th><span className="sr-only">Aksi</span></th></tr></thead>
            <tbody>
              {shown.map((p) => {
                const st = status(PRODUCT_STATUS, p.status);
                const img = p.images[0] ? sized(assets.url(p.images[0]), 320) : undefined;
                return (
                  <tr key={p.id}>
                    <td>
                      <div className="d-cell-main">
                        {/* eslint-disable-next-line @next/next/no-img-element */}
                        {img ? <img className="d-thumb" src={img} alt="" /> : <span className="d-thumb" aria-hidden="true"><ImageIcon weight="duotone" /></span>}
                        <div>
                          <button type="button" className="d-link d-strong" style={{ color: "var(--ink)" }} onClick={() => setEdit(p)} disabled={!ctx.can("products.update")}>{p.name}</button>
                          <span className="d-sub">
                            {p.variants.length > 1 ? `${p.variants.length} varian` : p.variants[0]?.sku}
                            {!p.available && " · stok habis/tidak tersedia"}
                          </span>
                        </div>
                      </div>
                    </td>
                    <td className="d-hide-sm">{catName(p.category_id) ?? <span className="d-muted">—</span>}</td>
                    <td className="d-num">{priceRange(p)}</td>
                    <td><Badge tone={st.tone}>{st.label}</Badge></td>
                    <td className="d-actions">
                      <div className="d-row">
                        {p.status !== "active" && ctx.can("products.update") && <button type="button" className="d-btn d-btn-sm" onClick={() => void setProductStatus(p, "active")}>Aktifkan</button>}
                        <RowMenu
                          items={[
                            ...(ctx.can("products.update") ? [{ label: "Ubah", run: () => setEdit(p) }] : []),
                            ...(p.status === "active" && ctx.can("products.archive") ? [{ label: "Arsipkan", run: () => void setProductStatus(p, "archived"), danger: true }] : []),
                          ]}
                          label={`Aksi untuk ${p.name}`}
                        />
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <Sheet
        open={!!edit}
        onClose={closeEditor}
        wide
        title={edit?.id ? `Ubah produk` : "Produk baru"}
        description={edit?.id ? edit.name : "Disimpan sebagai draf; aktifkan setelah siap dijual."}
      >
        {edit && <Editor key={edit.id || "new"} ctx={ctx} base={base} initial={edit} cats={cats} onCancel={closeEditor} onSaved={async (name) => { closeEditor(); await load(); toast.ok(`“${name}” tersimpan.`); }} />}
      </Sheet>

      <Sheet open={catsOpen} onClose={() => setCatsOpen(false)} title="Kategori" description="Kelompokkan produk agar pembeli mudah mencari di website.">
        <Categories ctx={ctx} base={base} cats={cats} reload={load} />
      </Sheet>
    </Assets.Provider>
  );
}

/** Semua kombinasi nilai opsi (maks. 100) — varian lama dipertahankan bila kombinasinya sama. */
function combos(options: Option[]): string[][] {
  const used = options.filter((o) => o.name.trim() && o.values.length > 0);
  if (used.length === 0) return [[]];
  let out: string[][] = [[]];
  for (const o of used) out = out.flatMap((c) => o.values.map((v) => [...c, v])).slice(0, 100);
  return out;
}

function Editor({ ctx, base, initial, cats, onCancel, onSaved }: {
  ctx: TenantCtx; base: string; initial: Product; cats: Category[]; onCancel: () => void; onSaved: (name: string) => void;
}) {
  const [p, setP] = useState<Product>(initial);
  const [hasVariants, setHasVariants] = useState(initial.options.length > 0);
  const [options, setOptions] = useState<Option[]>(initial.options.length ? initial.options : [{ name: "Ukuran", values: [] }]);
  const [optDraft, setOptDraft] = useState<Record<number, string>>({});
  const formRef = useRef<HTMLFormElement>(null);
  const f = useFieldErrors(formRef);
  const [busy, setBusy] = useState(false);
  const canSave = ctx.can("products.update") || (!p.id && ctx.can("products.create"));

  const usedOptions = useMemo(() => (hasVariants ? options.filter((o) => o.name.trim() && o.values.length > 0) : []), [hasVariants, options]);
  // Varian diturunkan dari kombinasi opsi; data harga/SKU varian lama dipakai ulang.
  const variants: Variant[] = useMemo(() => {
    if (!hasVariants || usedOptions.length === 0) {
      const v = p.variants[0] ?? { sku: "", option_values: [], price: "", promo_price: null, is_active: true };
      return [{ ...v, option_values: [] }];
    }
    return combos(usedOptions).map((vals) => {
      const old = p.variants.find((v) => v.option_values.join("|") === vals.join("|"));
      return old ?? { sku: "", option_values: vals, price: p.variants[0]?.price ?? "", promo_price: null, is_active: true };
    });
  }, [hasVariants, usedOptions, p.variants]);

  const setVariant = (i: number, patch: Partial<Variant>) => {
    const next = variants.map((x, j) => (j === i ? { ...x, ...patch } : x));
    // Simpan juga varian yang sedang tidak tampil agar tidak hilang saat opsi diubah balik.
    const keep = p.variants.filter((v) => !next.some((n) => n.option_values.join("|") === v.option_values.join("|")));
    setP({ ...p, variants: [...next, ...keep] });
  };

  async function save() {
    setBusy(true);
    f.clear();
    const body = {
      name: p.name, slug: p.slug || null, description: p.description, category_id: p.category_id, available: p.available,
      options: usedOptions,
      variants: variants.map((v) => ({
        id: v.id, sku: v.sku, option_values: v.option_values, price: v.price,
        promo_price: v.promo_price || null, is_active: v.is_active,
      })),
      images: p.images,
    };
    try {
      if (p.id) await api("PUT", `${base}/products/${p.id}`, { json: body });
      else await api("POST", `${base}/products`, { json: body });
      onSaved(p.name);
    } catch (e) {
      f.fromError(e);
    } finally { setBusy(false); }
  }

  const fieldProps = f.props;

  function addValue(i: number) {
    const raw = (optDraft[i] ?? "").split(",").map((v) => v.trim()).filter(Boolean);
    if (raw.length === 0) return;
    setOptions(options.map((o, j) => (j === i ? { ...o, values: [...new Set([...o.values, ...raw])] } : o)));
    setOptDraft({ ...optDraft, [i]: "" });
  }

  const single = variants[0]!;
  return (
    <form ref={formRef} noValidate onSubmit={(e) => { e.preventDefault(); void save(); }} style={{ display: "contents" }}>
      <Alert kind="error">{f.general}</Alert>
      {f.count > 0 && <Alert kind="error">Periksa {f.count} isian yang ditandai di bawah.</Alert>}

      <section className="d-form-section">
        <h3>Informasi produk</h3>
        <p className="d-muted">Nama dan deskripsi tampil di halaman produk website.</p>
        <label className="d-field"><span>Nama <span className="d-req">*</span></span>
          <input aria-required="true" {...fieldProps("name")} value={p.name} maxLength={120} placeholder="mis. Kopi Susu Gula Aren" onChange={(e) => setP({ ...p, name: e.target.value })} />
          {f.error("name")}
        </label>
        <div className="d-grid">
          <label className="d-field"><span>Kategori</span>
            <select value={p.category_id ?? ""} onChange={(e) => setP({ ...p, category_id: e.target.value || null })}>
              <option value="">Tanpa kategori</option>
              {cats.map((c) => <option key={c.id} value={c.id}>{c.parent_id ? `— ${c.name}` : c.name}</option>)}
            </select>
          </label>
          <label className="d-switch" style={{ alignSelf: "center" }}>
            <input type="checkbox" checked={p.available} onChange={(e) => setP({ ...p, available: e.target.checked })} />
            <span><strong>Tersedia untuk dijual</strong><span className="d-muted">Matikan bila stok sedang kosong.</span></span>
          </label>
        </div>
        <label className="d-field"><span>Deskripsi <span className="d-opt">(opsional)</span></span>
          <textarea rows={4} maxLength={5000} value={p.description ?? ""} placeholder="Bahan, ukuran, cara penyajian…" onChange={(e) => setP({ ...p, description: e.target.value || null })} />
        </label>
      </section>

      <section className="d-form-section">
        <h3>Foto</h3>
        <p className="d-muted">Foto pertama menjadi sampul di katalog. Maksimal 10 foto.</p>
        <div className="d-grid">
          {p.images.map((r, i) => (
            <ImagePicker key={`${r}-${i}`} label={i === 0 ? "Foto sampul" : `Foto ${i + 1}`} value={r}
              onChange={(v) => setP({ ...p, images: v ? p.images.map((x, j) => (j === i ? v : x)) : p.images.filter((_, j) => j !== i) })} />
          ))}
          {p.images.length < 10 && <ImagePicker key={`add-${p.images.length}`} label={p.images.length === 0 ? "Foto sampul" : "Tambah foto"} value={undefined} onChange={(v) => v && setP({ ...p, images: [...p.images, v] })} />}
        </div>
      </section>

      <section className="d-form-section">
        <h3>Harga & varian</h3>
        <p className="d-muted">Harga promo opsional; bila diisi, harga normal tampil dicoret.</p>
        <label className="d-switch">
          <input type="checkbox" checked={hasVariants} onChange={(e) => setHasVariants(e.target.checked)} />
          <span><strong>Produk ini punya varian</strong><span className="d-muted">mis. ukuran, rasa, atau warna dengan harga berbeda.</span></span>
        </label>

        {!hasVariants || usedOptions.length === 0 ? (
          <>
            {hasVariants && usedOptions.length === 0 && <p className="d-hint">Isi nama opsi dan minimal satu nilai di bawah untuk membuat varian.</p>}
            {!hasVariants && (
              <div className="d-grid">
                <label className="d-field"><span>SKU <span className="d-req">*</span></span>
                  <input aria-required="true" {...fieldProps("variants[0].sku")} value={single.sku} maxLength={64} placeholder="mis. KSA-01" onChange={(e) => setVariant(0, { sku: e.target.value })} />
                  {f.error("variants[0].sku")}
                </label>
                <label className="d-field"><span>Harga <span className="d-req">*</span></span>
                  <div className="d-input-prefix"><span aria-hidden="true">Rp</span>
                    <input aria-required="true" aria-label="Harga" inputMode="decimal" {...fieldProps("variants[0].price")} value={single.price} placeholder="0" onChange={(e) => setVariant(0, { price: e.target.value })} />
                  </div>
                  {f.error("variants[0].price")}
                </label>
                <label className="d-field"><span>Harga promo <span className="d-opt">(opsional)</span></span>
                  <div className="d-input-prefix"><span aria-hidden="true">Rp</span>
                    <input aria-label="Harga promo" inputMode="decimal" {...fieldProps("variants[0].promo_price")} value={single.promo_price ?? ""} placeholder="—" onChange={(e) => setVariant(0, { promo_price: e.target.value || null })} />
                  </div>
                  {f.error("variants[0].promo_price")}
                </label>
              </div>
            )}
          </>
        ) : null}

        {hasVariants && (
          <div className="d-stack" style={{ marginBottom: 16 }}>
            {options.map((o, i) => (
              <div className="e-list-item" key={i}>
                <div className="d-row" style={{ alignItems: "flex-end", flexWrap: "nowrap" }}>
                  <label className="d-field" style={{ flex: "0 0 160px", margin: 0 }}><span>Nama opsi</span>
                    <input value={o.name} maxLength={40} placeholder="Ukuran" onChange={(e) => setOptions(options.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))} />
                  </label>
                  <label className="d-field" style={{ flex: 1, margin: 0 }}><span>Tambah nilai (pisahkan koma, tekan Enter)</span>
                    <input value={optDraft[i] ?? ""} placeholder="S, M, L" onChange={(e) => setOptDraft({ ...optDraft, [i]: e.target.value })}
                      onKeyDown={(e) => { if (e.key === "Enter" || e.key === ",") { e.preventDefault(); addValue(i); } }} onBlur={() => addValue(i)} />
                  </label>
                  <button type="button" className="d-icon-btn" aria-label={`Hapus opsi ${o.name || i + 1}`} onClick={() => setOptions(options.filter((_, j) => j !== i))}><Trash weight="bold" /></button>
                </div>
                {o.values.length > 0 && (
                  <div className="d-chips" style={{ marginTop: 10 }}>
                    {o.values.map((v) => (
                      <span className="d-chip" key={v}>{v}
                        <button type="button" aria-label={`Hapus nilai ${v}`} onClick={() => setOptions(options.map((x, j) => (j === i ? { ...x, values: x.values.filter((y) => y !== v) } : x)))}><X weight="bold" /></button>
                      </span>
                    ))}
                  </div>
                )}
              </div>
            ))}
            {options.length < 3 && (
              <button type="button" className="d-btn d-btn-sm" style={{ justifySelf: "start" }} onClick={() => setOptions([...options, { name: "", values: [] }])}>
                <Plus weight="bold" aria-hidden="true" />Tambah opsi
              </button>
            )}
          </div>
        )}

        {hasVariants && usedOptions.length > 0 && (
          <div className="d-table-wrap">
            <table className="d-table">
              <thead><tr><th>Varian</th><th>SKU</th><th>Harga</th><th>Harga promo</th><th>Aktif</th></tr></thead>
              <tbody>
                {variants.map((v, i) => (
                  <tr key={v.option_values.join("|")}>
                    <td className="d-strong d-nowrap">{v.option_values.join(" / ")}</td>
                    <td>
                      <input aria-required="true" aria-label={`SKU varian ${i + 1}`} {...fieldProps(`variants[${i}].sku`)} value={v.sku} maxLength={64} onChange={(e) => setVariant(i, { sku: e.target.value })} />
                      {f.error(`variants[${i}].sku`)}
                    </td>
                    <td>
                      <input aria-required="true" aria-label={`Harga varian ${i + 1}`} inputMode="decimal" {...fieldProps(`variants[${i}].price`)} value={v.price} onChange={(e) => setVariant(i, { price: e.target.value })} />
                      {f.error(`variants[${i}].price`)}
                    </td>
                    <td>
                      <input aria-label={`Harga promo varian ${i + 1}`} inputMode="decimal" {...fieldProps(`variants[${i}].promo_price`)} value={v.promo_price ?? ""} onChange={(e) => setVariant(i, { promo_price: e.target.value || null })} />
                    </td>
                    <td><input aria-label={`Aktif varian ${i + 1}`} type="checkbox" checked={v.is_active} onChange={(e) => setVariant(i, { is_active: e.target.checked })} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {f.error("options")}
      </section>

      <section className="d-form-section">
        <details className="d-details">
          <summary>Pengaturan lanjutan</summary>
          <label className="d-field"><span>Slug URL <span className="d-opt">(kosongkan agar dibuat otomatis)</span></span>
            <input {...fieldProps("slug")} value={p.slug} maxLength={80} placeholder="kopi-susu-gula-aren" onChange={(e) => setP({ ...p, slug: e.target.value })} />
            {f.error("slug")}
          </label>
        </details>
      </section>

      <div className="d-sheet-foot" style={{ margin: "0 -20px -20px", position: "sticky", bottom: -20 }}>
        <span className="d-muted">{p.id ? `Status: ${status(PRODUCT_STATUS, p.status).label}` : "Produk baru disimpan sebagai draf."}</span>
        <button type="button" className="d-btn" onClick={onCancel}>Batal</button>
        <BusyButton busy={busy} disabled={!canSave}>Simpan</BusyButton>
      </div>
    </form>
  );
}

function Categories({ ctx, base, cats, reload }: { ctx: TenantCtx; base: string; cats: Category[]; reload: () => Promise<unknown> }) {
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [busy, setBusy] = useState(false);
  const can = ctx.can("products.update");
  async function add() {
    if (!name.trim()) return;
    setBusy(true);
    try {
      await api("POST", `${base}/categories`, { json: { name, parent_id: parent || null } });
      setName("");
      await reload();
      toast.ok("Kategori ditambahkan.");
    } catch (e) { toast.error(describe(e)); } finally { setBusy(false); }
  }
  async function remove(c: Category) {
    if (!(await confirm({ title: `Hapus kategori “${c.name}”?`, body: "Produk di kategori ini menjadi tanpa kategori.", confirmLabel: "Hapus", danger: true }))) return;
    try { await api("DELETE", `${base}/categories/${c.id}`); await reload(); toast.ok("Kategori dihapus."); } catch (e) { toast.error(describe(e)); }
  }
  return (
    <>
      {can && (
        <form className="d-stack" onSubmit={(e) => { e.preventDefault(); void add(); }} style={{ marginBottom: 20 }}>
          <div className="d-grid">
            <label className="d-field"><span>Nama kategori</span><input value={name} maxLength={60} placeholder="mis. Minuman" onChange={(e) => setName(e.target.value)} /></label>
            <label className="d-field"><span>Induk</span>
              <select value={parent} onChange={(e) => setParent(e.target.value)}>
                <option value="">Kategori utama</option>
                {cats.filter((c) => !c.parent_id).map((c) => <option key={c.id} value={c.id}>Sub dari {c.name}</option>)}
              </select>
            </label>
          </div>
          <BusyButton busy={busy} className="d-btn" disabled={!name.trim()}><Plus weight="bold" aria-hidden="true" />Tambah kategori</BusyButton>
        </form>
      )}
      {cats.length === 0 ? <EmptyState icon={Tag} title="Belum ada kategori">Kategori membantu pembeli menyaring katalog.</EmptyState> : (
        <ul className="d-list">
          {cats.map((c) => (
            <li key={c.id}>
              <div className="d-list-main" style={{ paddingLeft: c.parent_id ? 16 : 0 }}>
                <strong>{c.parent_id ? `↳ ${c.name}` : c.name}</strong>
              </div>
              {can && <button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => void remove(c)} aria-label={`Hapus kategori ${c.name}`}><Trash weight="bold" aria-hidden="true" />Hapus</button>}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
