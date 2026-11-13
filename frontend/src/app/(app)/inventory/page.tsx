"use client";

// Stok (P3-2): level per varian, mutasi (masuk/keluar/opname), ambang low-stock, riwayat.

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Row {
  variant_id: string; product_name: string; variant_name: string; sku: string;
  on_hand: number; reserved: number; available: number; allow_negative: boolean; min_stock: number | null; is_low: boolean;
}
interface Movement { id: string; kind: string; delta: number; on_hand_after: number; reason: string; reference: string | null; created_at: string }

const KIND: Record<string, string> = { stock_in: "Masuk", stock_out: "Keluar", adjustment: "Opname", sale: "Penjualan" };

export default function InventoryPage() {
  return <WithTenant>{(ctx) => <Stock ctx={ctx} />}</WithTenant>;
}

function Stock({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/inventory`;
  const [rows, setRows] = useState<Row[]>([]);
  const [low, setLow] = useState(false);
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(
    () => api<{ items: Row[] }>("GET", `${base}?limit=100&low=${low}${q ? `&q=${encodeURIComponent(q)}` : ""}`)
      .then((r) => setRows(r.items), (e) => setErr(describe(e))),
    [base, low, q],
  );
  useEffect(() => { void load(); }, [load]);
  return (
    <>
      <h1>Stok</h1>
      <Alert kind="error">{err}</Alert>
      <div className="d-row" style={{ marginBottom: 12 }}>
        <input aria-label="Cari" placeholder="Cari produk / SKU" value={q} onChange={(e) => setQ(e.target.value)} />
        <label className="d-check"><input type="checkbox" checked={low} onChange={(e) => setLow(e.target.checked)} /> Hanya stok menipis</label>
      </div>
      <div className="d-card" style={{ overflowX: "auto" }}>
        <table className="d-table">
          <thead><tr><th>Produk</th><th>Fisik</th><th>Dipesan</th><th>Tersedia</th><th>Ambang</th><th /></tr></thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.variant_id}>
                <td>{r.product_name}{r.variant_name !== "Default" && ` · ${r.variant_name}`}<div className="d-muted">{r.sku}</div></td>
                <td>{r.on_hand}</td>
                <td>{r.reserved}</td>
                <td>{r.available}{r.is_low && <strong> · menipis</strong>}</td>
                <td>{r.min_stock ?? "—"}</td>
                <td><button className="d-btn d-btn-sm" onClick={() => setOpen(open === r.variant_id ? null : r.variant_id)} aria-expanded={open === r.variant_id}>Kelola</button></td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={6}>
                  <div className="d-empty">
                    <p className="d-empty-title">Belum ada data stok</p>
                    <p className="d-muted">Stok muncul setelah Anda menambahkan produk. Setiap varian punya catatan stoknya sendiri.</p>
                    <Link className="d-btn d-btn-primary" href={`/products?tenant=${ctx.tenant.id}`}>Buka Produk</Link>
                  </div>
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {open && <Manage ctx={ctx} base={base} row={rows.find((r) => r.variant_id === open)!} onChange={load} />}
    </>
  );
}

function Manage({ ctx, base, row, onChange }: { ctx: TenantCtx; base: string; row: Row; onChange: () => Promise<unknown> }) {
  const can = ctx.can("inventory.adjust");
  const [moves, setMoves] = useState<Movement[]>([]);
  const [m, setM] = useState({ kind: "stock_in", quantity: "", reason: "", reference: "" });
  const [s, setS] = useState({ min_stock: row.min_stock?.toString() ?? "", allow_negative: row.allow_negative });
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const loadMoves = useCallback(() => api<Movement[]>("GET", `${base}/${row.variant_id}/movements`).then(setMoves, () => setMoves([])), [base, row.variant_id]);
  useEffect(() => { void loadMoves(); }, [loadMoves]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await f(); setMsg({ ok }); await onChange(); await loadMoves(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  return (
    <div className="d-card">
      <h2>{row.product_name} · {row.sku}</h2>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {can && (
        <>
          <div className="d-row">
            <select aria-label="Jenis" value={m.kind} onChange={(e) => setM({ ...m, kind: e.target.value })}>
              <option value="stock_in">Stok masuk</option>
              <option value="stock_out">Stok keluar</option>
              <option value="adjustment">Opname (jumlah fisik)</option>
            </select>
            <input aria-label="Jumlah" inputMode="numeric" placeholder="Jumlah" value={m.quantity} onChange={(e) => setM({ ...m, quantity: e.target.value })} />
            <input aria-label="Alasan" placeholder="Alasan (wajib)" maxLength={200} value={m.reason} onChange={(e) => setM({ ...m, reason: e.target.value })} />
            <input aria-label="Referensi" placeholder="Referensi (opsional)" maxLength={100} value={m.reference} onChange={(e) => setM({ ...m, reference: e.target.value })} />
            <button className="d-btn d-btn-primary" onClick={() => run(() => api("POST", `${base}/${row.variant_id}/movements`, {
              json: { kind: m.kind, quantity: Number(m.quantity), reason: m.reason, reference: m.reference || null },
            }), "Mutasi tercatat.")}>Catat</button>
          </div>
          <div className="d-row" style={{ marginTop: 12 }}>
            <input aria-label="Ambang stok menipis" placeholder="Ambang menipis" inputMode="numeric" value={s.min_stock} onChange={(e) => setS({ ...s, min_stock: e.target.value })} />
            <label className="d-check"><input type="checkbox" checked={s.allow_negative} onChange={(e) => setS({ ...s, allow_negative: e.target.checked })} /> Izinkan stok negatif</label>
            <button className="d-btn" onClick={() => run(() => api("PUT", `${base}/${row.variant_id}/settings`, {
              json: { min_stock: s.min_stock === "" ? null : Number(s.min_stock), allow_negative: s.allow_negative },
            }), "Pengaturan tersimpan.")}>Simpan pengaturan</button>
          </div>
        </>
      )}
      <h3>Riwayat</h3>
      <table className="d-table">
        <thead><tr><th>Waktu</th><th>Jenis</th><th>Perubahan</th><th>Setelah</th><th>Alasan</th></tr></thead>
        <tbody>
          {moves.map((x) => (
            <tr key={x.id}>
              <td>{new Date(x.created_at).toLocaleString("id-ID")}</td>
              <td>{KIND[x.kind] ?? x.kind}</td>
              <td>{x.delta > 0 ? `+${x.delta}` : x.delta}</td>
              <td>{x.on_hand_after}</td>
              <td>{x.reason}{x.reference && <div className="d-muted">{x.reference}</div>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
