"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";

import { TopBar } from "@/dashboard/shell";
import { api, loadSession, type Membership } from "@/lib/api";

export default function Home() {
  const router = useRouter();
  const [items, setItems] = useState<Membership[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    (async () => {
      if (!(await loadSession())) {
        router.replace("/login");
        return;
      }
      setItems(await api<Membership[]>("GET", "/api/v1/tenants"));
    })().catch(() => setError("Tidak dapat menghubungi server."));
  }, [router]);
  return (
    <>
      <TopBar />
      <main className="d-main">
        <h1>Toko saya</h1>
        {error && <div className="d-alert d-alert-error" role="alert">{error}</div>}
        {!items && !error && <p className="d-muted" aria-busy="true">Memuat…</p>}
        {items?.length === 0 && <p className="d-muted">Anda belum menjadi anggota toko mana pun.</p>}
        {items?.map((m) => (
          <div className="d-card" key={m.tenant_id}>
            <h2>{m.name}</h2>
            <p className="d-muted">{m.slug} · {m.role}</p>
            <div className="d-row">
              <Link className="d-btn d-btn-primary" href={`/website?tenant=${m.tenant_id}`}>Kelola website</Link>
              <Link className="d-btn" href={`/business?tenant=${m.tenant_id}`}>Profil bisnis</Link>
            </div>
          </div>
        ))}
      </main>
    </>
  );
}
