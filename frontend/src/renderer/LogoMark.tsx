"use client";

// Logo toko: foto dari Profil bisnis, atau huruf awal nama bila tidak ada / gagal dimuat (tanpa ikon rusak).

import { useState } from "react";

export function LogoMark({ url, name, className, size, lazy }: { url?: string; name: string; className: string; size: number; lazy?: boolean }) {
  const [failed, setFailed] = useState(false);
  if (url && !failed) {
    // eslint-disable-next-line @next/next/no-img-element -- aset tenant bertanda tangan
    return <img src={url} alt="" className={className} width={size} height={size} decoding="async" loading={lazy ? "lazy" : undefined} onError={() => setFailed(true)} />;
  }
  return <span className={`${className} ${className}-fallback`} aria-hidden="true">{(name.trim()[0] ?? "T").toUpperCase()}</span>;
}
