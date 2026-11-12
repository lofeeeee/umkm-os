"use client";

// Shell situs publik tenant (ADR-0014). Caddy menyajikan file ini untuk SEMUA path di
// {slug}.ROOT_DOMAIN; data diambil same-origin dari /_api/public/site (tenant dari Host).

import { useEffect, useState } from "react";

import { API_ORIGIN } from "@/lib/api";
import { pageSlug } from "@/lib/slug";
import { Site } from "@/renderer/Site";
import type { PublicSite } from "@/renderer/types";

export default function PublicSitePage() {
  const [site, setSite] = useState<PublicSite | null>(null);
  const [state, setState] = useState<"loading" | "missing" | "error">("loading");
  useEffect(() => {
    const slug = pageSlug(window.location.pathname);
    const load = slug
      ? fetch(`/_api/public/site?page=${encodeURIComponent(slug)}`, { credentials: "omit" })
      : Promise.resolve(null);
    load
      .then(async (r) => {
        if (!r || r.status === 404) return setState("missing");
        const body = await r.json();
        if (!r.ok || !body?.success) return setState("error");
        setSite(body.data as PublicSite);
        document.title = (body.data as PublicSite).business.profile.name;
      })
      .catch(() => setState("error"));
  }, []);
  if (site) {
    return (
      <main>
        <Site
          content={site.page.content}
          business={site.business}
          status={site.status}
          assetUrl={(r) => (site.assets[r] ? `${API_ORIGIN}${site.assets[r]}` : undefined)}
        />
      </main>
    );
  }
  return (
    <main className="d-narrow" aria-busy={state === "loading"}>
      {state === "loading" && <p className="d-muted">Memuat…</p>}
      {state === "missing" && <><h1>Halaman tidak ditemukan</h1><p className="d-muted">Situs ini belum tersedia.</p></>}
      {state === "error" && <><h1>Terjadi gangguan</h1><p className="d-muted">Coba muat ulang beberapa saat lagi.</p></>}
    </main>
  );
}
