"use client";

// Alias `/admin` -> `/platform`.
//
// Dashboard admin platform SUDAH ada di `/platform`. Rute ini hanya meneruskan, supaya pemilik
// bisa mengetik `/admin` tanpa perlu tahu nama internalnya.
//
// PENTING: alias ini TIDAK menambah gerbang sendiri, dan itu memang tujuannya. Otorisasi tetap
// sepenuhnya di `/platform` + `PlatformAdmin` (apps/api/src/platform.rs):
//   1. sesi login valid;
//   2. `users.platform_role = 'super_admin'`;
//   3. sesi lolos MFA (`mfa_verified`).
// Alias tanpa otorisasi sendiri berarti satu-satunya jalur masuk tetap yang berlapis itu; tidak
// ada endpoint pendaftaran publik yang bisa dipakai menaikkan hak akses.

import { useRouter } from "next/navigation";
import { useEffect } from "react";

import Link from "@/ui/link";
import { PageSkeleton } from "@/ui/page";

export default function AdminAliasPage() {
  const router = useRouter();
  useEffect(() => {
    router.replace("/platform");
  }, [router]);
  return (
    <div aria-busy="true">
      <PageSkeleton />
      <p className="d-muted d-small">
        Mengalihkan ke dashboard admin platform… Bila tidak berpindah, buka{" "}
        <Link href="/platform">/platform</Link>.
      </p>
    </div>
  );
}
