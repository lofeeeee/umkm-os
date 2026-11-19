"use client";

// Bagian bilah atas Studio: status simpan, riwayat versi, layar penuh.

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

import { WarningCircle, Check, CornersIn, CornersOut } from "@phosphor-icons/react/ssr";

import { api } from "@/lib/api";
import { describe } from "@/lib/api";
import { waktu } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, Skeleton } from "@/ui/page";
import { toast } from "@/ui/toast";
import { t as tx } from "@/i18n";
import { Tr } from "@/i18n/react";
import { type SaveState } from "@/studio/editor-model";

export function SaveBadge({ state, saved, err, onRetry }: { state: SaveState; saved: boolean; err: string | null; onRetry: () => void }) {
  if (state === "readonly") return <Badge plain>{tx("Hanya lihat")}</Badge>;
  if (state === "conflict" || state === "error") {
    return (
      <span className="st-save st-save-bad" role="alert" title={err ?? undefined}>
        <WarningCircle weight="fill" aria-hidden="true" />{state === "conflict" ? tx("Konflik: muat ulang") : tx("Gagal menyimpan")}
        {state === "error" && <button type="button" className="d-link d-small" onClick={onRetry}>{tx("Coba lagi")}</button>}
      </span>
    );
  }
  return <span className="st-save" aria-live="polite">{saved ? <><Check weight="bold" aria-hidden="true" />{tx("Draf tersimpan")}</> : tx("Menyimpan…")}</span>;
}

export interface Version { version: number; created_at_unix: number; is_current: boolean }
export function Versions({ base, canPublish, published, onRestored }: { base: string; canPublish: boolean; published: number | null; onRestored: (v: number) => void }) {
  const [items, setItems] = useState<Version[] | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const load = useCallback(() => api<Version[]>("GET", `${base}/website/pages/home/versions`).then(setItems, (e) => { toast.error(describe(e)); setItems([]); }), [base]);
  useEffect(() => { void load(); }, [load, published]);
  if (!items) return <Skeleton lines={4} />;
  if (items.length === 0) return <p className="d-muted">{tx("Belum pernah diterbitkan.")}</p>;
  return (
    <ul className="d-list">
      {items.map((v) => (
        <li key={v.version}>
          <div className="d-list-main">
            <strong><Tr s="Versi {0}" a={[v.version]} />{" "}{v.is_current && <Badge tone="ok">{tx("Tampil di situs")}</Badge>}</strong>
            <span className="d-muted">{waktu(v.created_at_unix * 1000)}</span>
          </div>
          {!v.is_current && (
            <button type="button" className="d-btn d-btn-sm" disabled={!canPublish || busy !== null} onClick={async () => {
              if (!(await confirm({ title: tx("Pulihkan versi {0}?", v.version), body: tx("Situs publik akan menampilkan versi ini lagi. Draf yang sedang Anda ubah tidak berubah."), confirmLabel: tx("Pulihkan") }))) return;
              setBusy(v.version);
              try {
                await api("POST", `${base}/website/pages/home/rollback`, { json: { version: v.version } });
                toast.ok(tx("Situs kini menampilkan versi {0}.", v.version));
                onRestored(v.version);
                await load();
              } catch (e) { toast.error(describe(e)); } finally { setBusy(null); }
            }}>{tx("Pulihkan")}</button>
          )}
        </li>
      ))}
    </ul>
  );
}

/* ============================================================ Layar penuh */
export function subscribeFs(cb: () => void) {
  document.addEventListener("fullscreenchange", cb);
  return () => document.removeEventListener("fullscreenchange", cb);
}
export function FullscreenButton() {
  const on = useSyncExternalStore(subscribeFs, () => !!document.fullscreenElement, () => false);
  const toggle = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen?.().catch(() => toast.error(tx("Browser ini tidak mendukung layar penuh.")));
  };
  return (
    <button type="button" className="d-icon-btn" onClick={toggle} aria-pressed={on} aria-label={on ? tx("Keluar dari layar penuh") : tx("Layar penuh")} title={on ? tx("Keluar dari layar penuh (Esc)") : tx("Layar penuh")}>
      {on ? <CornersIn weight="bold" /> : <CornersOut weight="bold" />}
    </button>
  );
}

/* ============================================================ Galeri template */
