"use client";

// Stok (P3-2): level per varian, mutasi (masuk/keluar/opname), ambang low-stock, riwayat.
// Kelola satu varian di panel samping supaya daftar tetap terlihat.

import Link from "@/ui/link";
import { useCallback, useEffect, useState } from "react";

import { ArrowDown, ArrowUp, Scales, Stack } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { STOCK_MOVEMENT, angka, waktu } from "@/lib/labels";
import { Sheet } from "@/ui/dialog";
import { Badge, BusyButton, EmptyState, PageHeader, SearchInput, Skeleton, Tabs } from "@/ui/page";
import { toast } from "@/ui/toast";
import { t as tx } from "@/i18n";

interface Row {
  variant_id: string; product_name: string; variant_name: string; sku: string;
  on_hand: number; reserved: number; available: number; allow_negative: boolean; min_stock: number | null; is_low: boolean;
}
interface Movement { id: string; kind: string; delta: number; on_hand_after: number; reason: string; reference: string | null; created_at: string }

export default function InventoryPage() {
  return <WithTenant>{(ctx) => <Stock ctx={ctx} />}</WithTenant>;
}

const variantLabel = (r: Row) => (r.variant_name && r.variant_name !== "Default" ? r.variant_name : null);

function Stock({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/inventory`;
  const [rows, setRows] = useState<Row[] | null>(null);
  const [low, setLow] = useState<"all" | "low">("all");
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Row[] }>("GET", `${base}?limit=100&low=${low === "low"}${q ? `&q=${encodeURIComponent(q)}` : ""}`)
      .then((r) => { setRows(r.items); setErr(null); }, (e) => setErr(describe(e))),
    [base, low, q],
  );
  useEffect(() => {
    const t = setTimeout(() => void load(), q ? 250 : 0);
    return () => clearTimeout(t);
  }, [load, q]);
  const row = rows?.find((r) => r.variant_id === open);
  return (
    <>
      <PageHeader title={tx("Stok")} description={tx("Jumlah fisik, yang sedang dipesan, dan yang tersedia per varian. Catat stok masuk/keluar agar riwayatnya rapi.")} />
      <Alert kind="error">{err}</Alert>
      <Tabs label={tx("Filter stok")} value={low} onChange={setLow} items={[{ value: "all", label: tx("Semua varian") }, { value: "low", label: tx("Stok menipis") }]} />
      <div className="d-toolbar">
        <SearchInput label={tx("Cari")} placeholder={tx("Cari produk atau SKU…")} value={q} onChange={setQ} />
      </div>
      <div className="d-table-wrap">
        {rows === null ? <div style={{ padding: 20 }}><Skeleton lines={5} /></div> : rows.length === 0 ? (
          <EmptyState
            icon={Stack}
            title={low === "low" ? tx("Tidak ada stok menipis") : q ? tx("Tidak ada yang cocok") : tx("Belum ada data stok")}
            action={low === "all" && !q ? <Link className="d-btn d-btn-primary" href={`/products?tenant=${ctx.tenant.id}`}>{tx("Buka Produk")}</Link> : undefined}
          >
            {low === "low" ? tx("Semua varian masih di atas ambang minimum.") : q ? tx("Coba kata kunci lain.") : tx("Stok muncul setelah Anda menambahkan produk. Setiap varian punya catatan stoknya sendiri.")}
          </EmptyState>
        ) : (
          <table className="d-table d-table-stack">
            <thead><tr><th>{tx("Produk")}</th><th className="d-num">{tx("Fisik")}</th><th className="d-num">{tx("Dipesan")}</th><th className="d-num">{tx("Tersedia")}</th><th className="d-num d-hide-sm">{tx("Ambang")}</th><th><span className="sr-only">{tx("Aksi")}</span></th></tr></thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.variant_id}>
                  <td><span className="d-strong">{r.product_name}</span><span className="d-sub">{[variantLabel(r), r.sku].filter(Boolean).join(" · ")}</span></td>
                  <td className="d-num">{r.on_hand}</td>
                  <td className="d-num d-muted">{r.reserved}</td>
                  <td className="d-num">{r.is_low ? <Badge tone="warn">{r.available}</Badge> : <span className="d-strong">{r.available}</span>}</td>
                  <td className="d-num d-hide-sm d-muted">{r.min_stock ?? "—"}</td>
                  <td className="d-actions"><button className="d-btn d-btn-sm" onClick={() => setOpen(r.variant_id)}>{tx("Kelola")}</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <Sheet open={!!row} onClose={() => setOpen(null)} title={row?.product_name ?? ""} description={row ? [variantLabel(row), row.sku].filter(Boolean).join(" · ") : undefined}>
        {row && <Manage key={row.variant_id} ctx={ctx} base={base} row={row} onChange={load} />}
      </Sheet>
    </>
  );
}

const KINDS = [
  { value: "stock_in", get label() { return tx("Stok masuk"); }, icon: ArrowDown, get hint() { return tx("Barang datang dari pemasok/produksi."); } },
  { value: "stock_out", get label() { return tx("Stok keluar"); }, icon: ArrowUp, get hint() { return tx("Rusak, hilang, atau dipakai sendiri."); } },
  { value: "adjustment", get label() { return tx("Opname"); }, icon: Scales, get hint() { return tx("Isi jumlah fisik hasil hitung ulang."); } },
] as const;

function Manage({ ctx, base, row, onChange }: { ctx: TenantCtx; base: string; row: Row; onChange: () => Promise<unknown> }) {
  const can = ctx.can("inventory.adjust");
  const [moves, setMoves] = useState<Movement[] | null>(null);
  const [m, setM] = useState({ kind: "stock_in", quantity: "", reason: "", reference: "" });
  const [s, setS] = useState({ min_stock: row.min_stock?.toString() ?? "", allow_negative: row.allow_negative });
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const loadMoves = useCallback(() => api<Movement[]>("GET", `${base}/${row.variant_id}/movements`).then(setMoves, () => setMoves([])), [base, row.variant_id]);
  useEffect(() => { void loadMoves(); }, [loadMoves]);
  async function run(key: string, f: () => Promise<unknown>, ok: string) {
    setErr(null);
    setBusy(key);
    try { await f(); toast.ok(ok); await onChange(); await loadMoves(); } catch (e) { setErr(describe(e)); } finally { setBusy(null); }
  }
  const kind = KINDS.find((k) => k.value === m.kind)!;
  return (
    <>
      <dl className="d-stat-grid" style={{ margin: "0 0 20px" }}>
        <div className="d-stat"><dt className="d-stat-label">{tx("Fisik")}</dt><dd className="d-stat-value" style={{ margin: 0 }}>{angka(row.on_hand)}</dd></div>
        <div className="d-stat"><dt className="d-stat-label">{tx("Dipesan")}</dt><dd className="d-stat-value" style={{ margin: 0 }}>{angka(row.reserved)}</dd></div>
        <div className={`d-stat${row.is_low ? " d-stat-warn" : ""}`}><dt className="d-stat-label">{tx("Tersedia")}</dt><dd className="d-stat-value" style={{ margin: 0 }}>{angka(row.available)}</dd></div>
      </dl>
      <Alert kind="error">{err}</Alert>
      {can && (
        <>
          <section className="d-form-section">
            <h3>{tx("Catat perubahan stok")}</h3>
            <p className="d-muted">{kind.hint}</p>
            <div className="d-seg" role="group" aria-label={tx("Jenis perubahan")} style={{ marginBottom: 16 }}>
              {KINDS.map((k) => (
                <button key={k.value} type="button" aria-pressed={m.kind === k.value} onClick={() => setM({ ...m, kind: k.value })}>{k.label}</button>
              ))}
            </div>
            <form onSubmit={(e) => {
              e.preventDefault();
              void run("move", () => api("POST", `${base}/${row.variant_id}/movements`, {
                json: { kind: m.kind, quantity: Number(m.quantity), reason: m.reason, reference: m.reference || null },
              }), tx("Mutasi tercatat.")).then(() => setM((x) => ({ ...x, quantity: "", reason: "", reference: "" })));
            }}>
              <div className="d-grid">
                <label className="d-field"><span>{m.kind === "adjustment" ? tx("Jumlah fisik") : tx("Jumlah")}</span>
                  <input aria-label={tx("Jumlah")} inputMode="numeric" value={m.quantity} onChange={(e) => setM({ ...m, quantity: e.target.value })} />
                </label>
                <label className="d-field"><span>{tx("Alasan")}</span>
                  <input aria-label={tx("Alasan")} placeholder={tx("mis. Kiriman pemasok")} maxLength={200} value={m.reason} onChange={(e) => setM({ ...m, reason: e.target.value })} />
                </label>
              </div>
              <label className="d-field"><span>{tx("Referensi")}{" "}<span className="d-opt">{tx("(opsional, mis. no. nota)")}</span></span>
                <input aria-label={tx("Referensi")} maxLength={100} value={m.reference} onChange={(e) => setM({ ...m, reference: e.target.value })} />
              </label>
              <BusyButton busy={busy === "move"} disabled={!m.quantity || !m.reason.trim()}>{tx("Catat")}</BusyButton>
            </form>
          </section>
          <section className="d-form-section">
            <h3>{tx("Peringatan stok")}</h3>
            <p className="d-muted">{tx("Anda diberi tahu saat stok tersedia turun di bawah ambang.")}</p>
            <label className="d-field" style={{ maxWidth: 200 }}><span>{tx("Ambang menipis")}</span>
              <input aria-label={tx("Ambang stok menipis")} placeholder={tx("mis. 5")} inputMode="numeric" value={s.min_stock} onChange={(e) => setS({ ...s, min_stock: e.target.value })} />
            </label>
            <label className="d-switch">
              <input type="checkbox" checked={s.allow_negative} onChange={(e) => setS({ ...s, allow_negative: e.target.checked })} />
              <span><strong>{tx("Izinkan stok negatif")}</strong><span className="d-muted">{tx("Pesanan tetap diterima meski stok habis (pre-order).")}</span></span>
            </label>
            <BusyButton type="button" className="d-btn" busy={busy === "settings"} onClick={() => run("settings", () => api("PUT", `${base}/${row.variant_id}/settings`, {
              json: { min_stock: s.min_stock === "" ? null : Number(s.min_stock), allow_negative: s.allow_negative },
            }), tx("Pengaturan tersimpan."))}>{tx("Simpan pengaturan")}</BusyButton>
          </section>
        </>
      )}
      <section className="d-form-section">
        <h3>{tx("Riwayat")}</h3>
        {moves === null ? <Skeleton lines={3} /> : moves.length === 0 ? <p className="d-muted">{tx("Belum ada mutasi.")}</p> : (
          <div className="d-table-wrap">
            <table className="d-table">
              <thead><tr><th>{tx("Waktu")}</th><th>{tx("Jenis")}</th><th className="d-num">{tx("Perubahan")}</th><th className="d-num">{tx("Setelah")}</th></tr></thead>
              <tbody>
                {moves.map((x) => (
                  <tr key={x.id}>
                    <td className="d-nowrap">{waktu(x.created_at)}<span className="d-sub">{x.reason}{x.reference && tx(" · {0}", x.reference)}</span></td>
                    <td>{STOCK_MOVEMENT[x.kind] ?? x.kind}</td>
                    <td className="d-num" style={{ color: x.delta > 0 ? "var(--ok)" : x.delta < 0 ? "var(--danger)" : undefined }}>{x.delta > 0 ? `+${x.delta}` : x.delta}</td>
                    <td className="d-num">{x.on_hand_after}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
