"use client";

// Bingkai perangkat untuk pratinjau Studio.
//
// Lebar bingkai (bukan lebar jendela browser) yang menentukan breakpoint: `.st-frame` adalah
// `container-type: inline-size`, dan seluruh CSS toko memakai `@container`, jadi mengubah ponsel →
// tablet → desktop langsung mengubah jumlah kolom tanpa perlu mengecilkan jendela.
//
// `transform: translateZ(0)` menjadikan bingkai ini containing block untuk `position: fixed`, sehingga
// chatbot/WhatsApp/keranjang melayang terkunci di dalam bingkai alih-alih menempel ke layar. Di situs
// publik pembungkus ini tidak dipasang, jadi perilaku `fixed` di sana tidak berubah.

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

export type DeviceViewport = "mobile" | "tablet" | "desktop";

/**
 * Lebar LAYAR yang disimulasikan per perangkat (px). Desktop = 1280 (laptop umum): pada lebar ini grid
 * situs selebar penuh 1080 px, sama seperti di situs terbit — jadi pembungkusan teks, tinggi blok, dan
 * posisi coretan di Studio identik dengan situs. Bila panggung lebih sempit, bingkai diperkecil (zoom)
 * tanpa mengubah tata letaknya.
 */
export const DEVICE_WIDTH: Record<DeviceViewport, number> = {
  mobile: 390,
  tablet: 768,
  desktop: 1280,
};

export const DEVICE_LABEL: Record<DeviceViewport, string> = {
  mobile: "Ponsel",
  tablet: "Tablet",
  desktop: "Desktop",
};

export interface DeviceFrameProps {
  device: DeviceViewport;
  /** Latar permukaan tema toko, agar bingkai tidak memakai warna dasbor. */
  surface?: "light" | "cream" | "dark";
  /** Perbesaran yang dipakai (1 = ukuran asli) — untuk ditampilkan di bilah atas. */
  onZoom?: (zoom: number) => void;
  children: ReactNode;
}

/** Ruang untuk bezel ponsel/tablet (px, di luar lebar layar). */
const BEZEL = 18;

export function DeviceFrame({ device, surface = "light", onZoom, children }: DeviceFrameProps) {
  const width = DEVICE_WIDTH[device];
  const outer = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<{ w: number; h: number } | null>(null);
  useLayoutEffect(() => {
    const el = outer.current;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const pad = device === "desktop" ? 0 : BEZEL;
  const zoom = box ? Math.min(1, (box.w - pad) / width, device === "desktop" ? 1 : (box.h - pad) / 640) : 1;
  const height = box ? (box.h - pad) / zoom : 0;
  useLayoutEffect(() => { onZoom?.(zoom); }, [zoom, onZoom]);
  // Bingkai selalu selebar layar perangkat asli; `scale()` hanya mengecilkan tampilannya. `transform`
  // juga menjadikannya containing block untuk `position: fixed` (chat/WA melayang terkunci di layar).
  const style = { width, height: height || undefined, transform: `scale(${zoom})`, transformOrigin: "top left" };
  return (
    <div ref={outer} className="sf-frame-outer" data-device={device}>
      <div className="sf-frame-box" style={{ width: width * zoom, height: height * zoom || undefined }}>
        <div className="st-frame sf-frame" data-surface={surface} data-device={device} style={style}>
          {/* Area gulir TERPISAH dari bingkai: bingkai (transform) = "layar" bagi elemen fixed, jadi
              chatbot gelembung/bilah & WhatsApp melayang tetap menempel di pojok saat isi digulir —
              sama seperti di situs terbit. Kalau bingkai sendiri yang digulir, elemen fixed ikut tergulir. */}
          <div className="st-frame-scroll">{children}</div>
        </div>
      </div>
    </div>
  );
}

/** Faktor perbesaran tampilan sebuah elemen (lebar di layar ÷ lebar CSS-nya). */
export function zoomOf(el: HTMLElement): number {
  return el.offsetWidth ? el.getBoundingClientRect().width / el.offsetWidth : 1;
}
