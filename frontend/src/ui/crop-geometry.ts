// Geometri & gambar bingkai potong gambar (murni, tanpa React — diuji crop-geometry.test.ts).
import { t } from "@/i18n";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type AspectRatioPreset = {
  id: "free" | "1:1" | "16:9" | "4:3";
  label: string;
  desc?: string;
  ratio: number | null;
};

export const ASPECT_RATIOS: AspectRatioPreset[] = [
  { id: "free", get label() { return t("Bebas"); }, ratio: null },
  { id: "1:1", label: "1:1", get desc() { return t("Kotak"); }, ratio: 1 },
  { id: "16:9", label: "16:9", get desc() { return t("Banner"); }, ratio: 16 / 9 },
  { id: "4:3", label: "4:3", get desc() { return t("Standar"); }, ratio: 4 / 3 },
];

export const CANVAS_W = 600;
export const CANVAS_H = 380;
export const PADDING = 20;
export const MIN_CROP_SIZE = 40;
export const HANDLE_RADIUS = 24;

export type Handle = "tl" | "tr" | "bl" | "br";

/** Gelapkan area di luar kotak potong, garis sepertiga, garis tepi, dan pegangan sudut. */
export function drawCropOverlay(ctx: CanvasRenderingContext2D, crop: Rect) {
  // Darkened overlay outside crop rect
  ctx.fillStyle = "rgba(0, 0, 0, 0.62)";
  // Top
  if (crop.y > 0) ctx.fillRect(0, 0, CANVAS_W, Math.max(0, crop.y));
  // Bottom
  const bottomY = crop.y + crop.h;
  if (bottomY < CANVAS_H) ctx.fillRect(0, bottomY, CANVAS_W, Math.max(0, CANVAS_H - bottomY));
  // Left
  if (crop.x > 0) ctx.fillRect(0, crop.y, Math.max(0, crop.x), crop.h);
  // Right
  const rightX = crop.x + crop.w;
  if (rightX < CANVAS_W) ctx.fillRect(rightX, crop.y, Math.max(0, CANVAS_W - rightX), crop.h);

  // Rule of thirds grid lines
  ctx.strokeStyle = "rgba(255, 255, 255, 0.28)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  const xThird1 = crop.x + crop.w / 3;
  const xThird2 = crop.x + (crop.w * 2) / 3;
  const yThird1 = crop.y + crop.h / 3;
  const yThird2 = crop.y + (crop.h * 2) / 3;

  ctx.moveTo(xThird1, crop.y);
  ctx.lineTo(xThird1, crop.y + crop.h);
  ctx.moveTo(xThird2, crop.y);
  ctx.lineTo(xThird2, crop.y + crop.h);

  ctx.moveTo(crop.x, yThird1);
  ctx.lineTo(crop.x + crop.w, yThird1);
  ctx.moveTo(crop.x, yThird2);
  ctx.lineTo(crop.x + crop.w, yThird2);
  ctx.stroke();

  // Crop box outline
  ctx.strokeStyle = "#ffffff";
  ctx.lineWidth = 1.5;
  ctx.strokeRect(crop.x, crop.y, crop.w, crop.h);

  // Corner accent brackets & handles
  const cornerSize = 16;
  ctx.lineWidth = 3;
  ctx.strokeStyle = "#ffffff";
  ctx.beginPath();
  // Top-left
  ctx.moveTo(crop.x, crop.y + cornerSize);
  ctx.lineTo(crop.x, crop.y);
  ctx.lineTo(crop.x + cornerSize, crop.y);
  // Top-right
  ctx.moveTo(crop.x + crop.w - cornerSize, crop.y);
  ctx.lineTo(crop.x + crop.w, crop.y);
  ctx.lineTo(crop.x + crop.w, crop.y + cornerSize);
  // Bottom-left
  ctx.moveTo(crop.x, crop.y + crop.h - cornerSize);
  ctx.lineTo(crop.x, crop.y + crop.h);
  ctx.lineTo(crop.x + cornerSize, crop.y + crop.h);
  // Bottom-right
  ctx.moveTo(crop.x + crop.w - cornerSize, crop.y + crop.h);
  ctx.lineTo(crop.x + crop.w, crop.y + crop.h);
  ctx.lineTo(crop.x + crop.w, crop.y + crop.h - cornerSize);
  ctx.stroke();

  // Corner circle grip dots
  ctx.fillStyle = "#ffffff";
  const dotR = 3.5;
  const corners = [
    { x: crop.x, y: crop.y },
    { x: crop.x + crop.w, y: crop.y },
    { x: crop.x, y: crop.y + crop.h },
    { x: crop.x + crop.w, y: crop.y + crop.h },
  ];
  for (const c of corners) {
    ctx.beginPath();
    ctx.arc(c.x, c.y, dotR, 0, Math.PI * 2);
    ctx.fill();
  }
}

/** Geser kotak potong, tetap di dalam batas gambar. */
export function moveCrop(start: Rect, dx: number, dy: number, bounds: Rect): Rect {
  const x = Math.max(bounds.x, Math.min(bounds.x + bounds.w - start.w, start.x + dx));
  const y = Math.max(bounds.y, Math.min(bounds.y + bounds.h - start.h, start.y + dy));
  return { x, y, w: start.w, h: start.h };
}

/** Ubah ukuran dari satu sudut (rasio dikunci bila `lockedRatio`), dibatasi gambar & ukuran minimum. */
export function resizeCrop(handle: Handle, startCrop: Rect, dx: number, dy: number, bounds: Rect, lockedRatio: number | null): Rect {
  // Corner handle resize
  let nx = startCrop.x;
  let ny = startCrop.y;
  let nw = startCrop.w;
  let nh = startCrop.h;

  if (handle === "br") {
    nw = Math.max(MIN_CROP_SIZE, startCrop.w + dx);
    nh = lockedRatio ? nw / lockedRatio : Math.max(MIN_CROP_SIZE, startCrop.h + dy);

    if (lockedRatio && nh > bounds.y + bounds.h - ny) {
      nh = bounds.y + bounds.h - ny;
      nw = nh * lockedRatio;
    }
    if (nx + nw > bounds.x + bounds.w) {
      nw = bounds.x + bounds.w - nx;
      if (lockedRatio) nh = nw / lockedRatio;
    }
  } else if (handle === "bl") {
    nw = Math.max(MIN_CROP_SIZE, startCrop.w - dx);
    nh = lockedRatio ? nw / lockedRatio : Math.max(MIN_CROP_SIZE, startCrop.h + dy);
    nx = startCrop.x + startCrop.w - nw;

    if (nx < bounds.x) {
      nx = bounds.x;
      nw = startCrop.x + startCrop.w - nx;
      if (lockedRatio) nh = nw / lockedRatio;
    }
    if (ny + nh > bounds.y + bounds.h) {
      nh = bounds.y + bounds.h - ny;
      if (lockedRatio) {
        nw = nh * lockedRatio;
        nx = startCrop.x + startCrop.w - nw;
      }
    }
  } else if (handle === "tr") {
    nw = Math.max(MIN_CROP_SIZE, startCrop.w + dx);
    nh = lockedRatio ? nw / lockedRatio : Math.max(MIN_CROP_SIZE, startCrop.h - dy);
    ny = startCrop.y + startCrop.h - nh;

    if (ny < bounds.y) {
      ny = bounds.y;
      nh = startCrop.y + startCrop.h - ny;
      if (lockedRatio) nw = nh * lockedRatio;
    }
    if (nx + nw > bounds.x + bounds.w) {
      nw = bounds.x + bounds.w - nx;
      if (lockedRatio) {
        nh = nw / lockedRatio;
        ny = startCrop.y + startCrop.h - nh;
      }
    }
  } else if (handle === "tl") {
    nw = Math.max(MIN_CROP_SIZE, startCrop.w - dx);
    nh = lockedRatio ? nw / lockedRatio : Math.max(MIN_CROP_SIZE, startCrop.h - dy);
    nx = startCrop.x + startCrop.w - nw;
    ny = startCrop.y + startCrop.h - nh;

    if (nx < bounds.x) {
      nx = bounds.x;
      nw = startCrop.x + startCrop.w - nx;
      if (lockedRatio) nh = nw / lockedRatio;
    }
    if (ny < bounds.y) {
      ny = bounds.y;
      nh = startCrop.y + startCrop.h - ny;
      if (lockedRatio) {
        nw = nh * lockedRatio;
        nx = startCrop.x + startCrop.w - nw;
      }
    }
  }

  nw = Math.max(MIN_CROP_SIZE, nw);
  nh = Math.max(MIN_CROP_SIZE, nh);

  return { x: nx, y: ny, w: nw, h: nh };
}
