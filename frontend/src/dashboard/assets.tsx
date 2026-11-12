"use client";

// Pustaka gambar tenant untuk pemilih gambar (produk, QRIS). ADR-0009: URL bertanda tangan.

import { useEffect, useMemo, useState } from "react";

import type { AssetsCtx, Upload } from "@/dashboard/editor/fields";
import { api, apiUrl } from "@/lib/api";

export function useUploads(tenantId: string, canEdit: boolean, extra: Record<string, string> = {}): AssetsCtx {
  const [uploads, setUploads] = useState<Upload[]>([]);
  useEffect(() => {
    api<Upload[]>("GET", `/api/v1/tenants/${tenantId}/uploads`).then(setUploads, () => setUploads([]));
  }, [tenantId]);
  const extraKey = JSON.stringify(extra);
  return useMemo(() => {
    const map: Record<string, string> = { ...(JSON.parse(extraKey) as Record<string, string>) };
    for (const u of uploads) map[u.ref] = u.url;
    return {
      uploads,
      canEdit,
      url: (r: string) => (map[r] ? apiUrl(map[r]!) : undefined),
      upload: async (file: File) => {
        try {
          const u = await api<Upload>("POST", `/api/v1/tenants/${tenantId}/uploads`, { bytes: file });
          setUploads((x) => [u, ...x]);
          return u;
        } catch {
          return null;
        }
      },
    };
  }, [uploads, canEdit, tenantId, extraKey]);
}

/** Rupiah untuk dashboard: `"15000.00"` → `"Rp15.000"`. */
export { rupiah, STATUS_TEXT } from "@/shop/api";
