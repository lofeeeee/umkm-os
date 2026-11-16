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

import type { ReactNode } from "react";

export type DeviceViewport = "mobile" | "tablet" | "desktop";

/** Lebar bingkai per perangkat (px). `null` = mengikuti lebar induk (desktop). */
export const DEVICE_WIDTH: Record<DeviceViewport, number | null> = {
  mobile: 390,
  tablet: 768,
  desktop: null,
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
  children: ReactNode;
}

export function DeviceFrame({ device, surface = "light", children }: DeviceFrameProps) {
  const width = DEVICE_WIDTH[device];
  return (
    <div className="sf-frame-outer" data-device={device}>
      <div
        className="st-frame sf-frame"
        data-surface={surface}
        data-device={device}
        style={width === null ? undefined : { width, maxWidth: "100%" }}
      >
        {children}
      </div>
    </div>
  );
}
