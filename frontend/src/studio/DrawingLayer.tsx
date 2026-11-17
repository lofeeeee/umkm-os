// Bentuk coretan (kuas) di dokumen Studio (`drawings`, schema_version 2). Gambar & interaksinya ada di
// src/renderer/ink.tsx (dirender sama di kanvas dan situs terbit) dan src/studio/LiveCanvas.tsx (kuas).

export { pointsToPath } from "@/renderer/ink";

export interface DrawingStroke {
  id: string;
  /** "background" atau "cell:<id blok>" (aturan validator server schema_v2.rs). */
  target: "background" | string;
  color: string;
  size: number;
  opacity: number;
  /** Titik bulat ≥ 0 dalam px desain (lebar grid = 1080). Maks 1000 titik. */
  points: [number, number][];
}
