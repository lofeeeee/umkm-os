// Coretan bebas (kuas) — dirender SAMA di kanvas Studio dan situs terbit.
//
// Koordinat disimpan dalam "px desain": lebar grid = 1080 (DESIGN_WIDTH). SVG memakai viewBox persegi
// selebar wadahnya + `overflow: visible`, jadi skala x dan y selalu sama (coretan tidak gepeng) dan
// coretan di bawah batas viewBox tetap tampil. Target:
// - "background"   → relatif ke pojok kiri atas grid, di belakang blok.
// - "cell:<id>"    → relatif ke pojok kiri atas blok itu, di atas isinya; ikut bergerak bersama blok.

import type { DrawingStroke } from "@/studio/DrawingLayer";
import { t } from "@/i18n";

/** Titik → path SVG halus (quadratic bezier di titik tengah). */
export function pointsToPath(points: [number, number][]): string {
  if (!points || points.length === 0) return "";
  const first = points[0];
  if (!first) return "";
  if (points.length === 1) return `M ${first[0]} ${first[1]} L ${first[0]} ${first[1]}`;
  const second = points[1];
  if (points.length === 2 && second) return `M ${first[0]} ${first[1]} Q ${first[0]} ${first[1]} ${second[0]} ${second[1]}`;
  let d = `M ${first[0]} ${first[1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const curr = points[i];
    const next = points[i + 1];
    if (curr && next) d += ` Q ${curr[0]} ${curr[1]} ${(curr[0] + next[0]) / 2} ${(curr[1] + next[1]) / 2}`;
  }
  const last = points[points.length - 1];
  const secondLast = points[points.length - 2];
  if (secondLast && last) d += ` Q ${secondLast[0]} ${secondLast[1]} ${last[0]} ${last[1]}`;
  return d;
}

export const cellTarget = (blockId: string) => `cell:${blockId}`;

/** Lapisan coretan. `viewWidth` = lebar wadah dalam px desain. */
export function InkLayer({ strokes, viewWidth, className = "", live }: { strokes: DrawingStroke[]; viewWidth: number; className?: string; live?: DrawingStroke | null }) {
  if (strokes.length === 0 && !live) return null;
  const all = live ? [...strokes, live] : strokes;
  return (
    <svg className={`sv2-ink ${className}`.trim()} viewBox={t("0 0 {0} {1}", viewWidth, viewWidth)} preserveAspectRatio="xMinYMin meet" aria-hidden="true" focusable="false">
      {all.map((s) => (
        <path key={s.id} data-stroke-id={s.id} d={pointsToPath(s.points)} stroke={s.color} strokeWidth={s.size} strokeOpacity={s.opacity}
          strokeLinecap="round" strokeLinejoin="round" fill="none" />
      ))}
    </svg>
  );
}
