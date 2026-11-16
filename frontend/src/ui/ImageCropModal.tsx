"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowCounterClockwise,
  Check,
  Crop,
  MagnifyingGlassMinus,
  MagnifyingGlassPlus,
  X,
} from "@phosphor-icons/react/ssr";

export interface ImageCropModalProps {
  open: boolean;
  imageSrc: string;
  onClose: () => void;
  onCrop: (file: File) => void;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

type AspectRatioPreset = {
  id: "free" | "1:1" | "16:9" | "4:3";
  label: string;
  desc?: string;
  ratio: number | null;
};

const ASPECT_RATIOS: AspectRatioPreset[] = [
  { id: "free", label: "Bebas", ratio: null },
  { id: "1:1", label: "1:1", desc: "Kotak", ratio: 1 },
  { id: "16:9", label: "16:9", desc: "Banner", ratio: 16 / 9 },
  { id: "4:3", label: "4:3", desc: "Standar", ratio: 4 / 3 },
];

const CANVAS_W = 600;
const CANVAS_H = 380;
const PADDING = 20;
const MIN_CROP_SIZE = 40;
const HANDLE_RADIUS = 24;

interface ImageCropContentProps {
  imageSrc: string;
  onClose: () => void;
  onCrop: (file: File) => void;
}

function ImageCropContent({ imageSrc, onClose, onCrop }: ImageCropContentProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const [imgElement, setImgElement] = useState<HTMLImageElement | null>(null);
  const [imageLoaded, setImageLoaded] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [exporting, setExporting] = useState(false);

  const [selectedRatioId, setSelectedRatioId] = useState<AspectRatioPreset["id"]>("free");
  const [zoom, setZoom] = useState<number>(1);
  const [pan, setPan] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [crop, setCrop] = useState<Rect>({ x: 80, y: 40, w: 440, h: 300 });

  // Drag interaction state
  const dragRef = useRef<{
    active: boolean;
    handle: "move" | "pan" | "tl" | "tr" | "bl" | "br";
    startX: number;
    startY: number;
    startCrop: Rect;
    startPan: { x: number; y: number };
  }>({
    active: false,
    handle: "pan",
    startX: 0,
    startY: 0,
    startCrop: { x: 0, y: 0, w: 0, h: 0 },
    startPan: { x: 0, y: 0 },
  });

  const [cursor, setCursor] = useState<string>("default");

  // Compute image layout bounds on canvas
  const getImageBounds = useCallback(
    (img: HTMLImageElement, currentZoom: number, currentPan: { x: number; y: number }): Rect => {
      const fitScale = Math.min(
        (CANVAS_W - PADDING * 2) / img.naturalWidth,
        (CANVAS_H - PADDING * 2) / img.naturalHeight
      );
      const w = img.naturalWidth * fitScale * currentZoom;
      const h = img.naturalHeight * fitScale * currentZoom;
      const x = (CANVAS_W - w) / 2 + currentPan.x;
      const y = (CANVAS_H - h) / 2 + currentPan.y;
      return { x, y, w, h };
    },
    []
  );

  // Initialize or reset crop box
  const initCrop = useCallback(
    (img: HTMLImageElement, ratio: number | null, currentZoom: number, currentPan: { x: number; y: number }) => {
      const bounds = getImageBounds(img, currentZoom, currentPan);
      const targetRatio = ratio ?? (bounds.w / bounds.h);

      let w = bounds.w * 0.85;
      let h = w / targetRatio;
      if (h > bounds.h * 0.85) {
        h = bounds.h * 0.85;
        w = h * targetRatio;
      }
      w = Math.max(MIN_CROP_SIZE, Math.min(bounds.w, w));
      h = Math.max(MIN_CROP_SIZE, Math.min(bounds.h, h));

      const x = bounds.x + (bounds.w - w) / 2;
      const y = bounds.y + (bounds.h - h) / 2;
      setCrop({ x, y, w, h });
    },
    [getImageBounds]
  );

  // Load image
  useEffect(() => {
    let active = true;

    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      if (!active) return;
      setImgElement(img);
      setImageLoaded(true);
      initCrop(img, null, 1, { x: 0, y: 0 });
    };
    img.onerror = () => {
      if (!active) return;
      setLoadError(true);
    };
    img.src = imageSrc;

    return () => {
      active = false;
    };
  }, [imageSrc, initCrop]);

  // Handle aspect ratio selection
  const handleRatioSelect = (preset: AspectRatioPreset) => {
    setSelectedRatioId(preset.id);
    if (!imgElement) return;

    const bounds = getImageBounds(imgElement, zoom, pan);
    const targetRatio = preset.ratio;

    if (!targetRatio) {
      // Free: keep current crop
      return;
    }

    const currentCenter = {
      x: crop.x + crop.w / 2,
      y: crop.y + crop.h / 2,
    };

    let w = crop.w;
    let h = w / targetRatio;

    if (h > bounds.h) {
      h = bounds.h * 0.9;
      w = h * targetRatio;
    }
    if (w > bounds.w) {
      w = bounds.w * 0.9;
      h = w / targetRatio;
    }

    let x = currentCenter.x - w / 2;
    let y = currentCenter.y - h / 2;

    // Clamp within image bounds
    x = Math.max(bounds.x, Math.min(bounds.x + bounds.w - w, x));
    y = Math.max(bounds.y, Math.min(bounds.y + bounds.h - h, y));

    setCrop({ x, y, w, h });
  };

  // Reset zoom, pan, and crop box
  const handleReset = () => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
    if (imgElement) {
      const activePreset = ASPECT_RATIOS.find((r) => r.id === selectedRatioId);
      initCrop(imgElement, activePreset?.ratio ?? null, 1, { x: 0, y: 0 });
    }
  };

  // Render canvas frame
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, CANVAS_W, CANVAS_H);

    // Dark backdrop background
    ctx.fillStyle = "#111114";
    ctx.fillRect(0, 0, CANVAS_W, CANVAS_H);

    if (!imgElement || !imageLoaded) return;

    const bounds = getImageBounds(imgElement, zoom, pan);

    // Draw the source image
    ctx.drawImage(imgElement, bounds.x, bounds.y, bounds.w, bounds.h);

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
  }, [imgElement, imageLoaded, zoom, pan, crop, getImageBounds]);

  // Pointer event helpers for hit-testing and dragging
  const getCanvasCoords = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  };

  const getHitHandle = (pt: { x: number; y: number }): "move" | "pan" | "tl" | "tr" | "bl" | "br" => {
    if (Math.hypot(pt.x - crop.x, pt.y - crop.y) <= HANDLE_RADIUS) return "tl";
    if (Math.hypot(pt.x - (crop.x + crop.w), pt.y - crop.y) <= HANDLE_RADIUS) return "tr";
    if (Math.hypot(pt.x - crop.x, pt.y - (crop.y + crop.h)) <= HANDLE_RADIUS) return "bl";
    if (Math.hypot(pt.x - (crop.x + crop.w), pt.y - (crop.y + crop.h)) <= HANDLE_RADIUS) return "br";

    if (pt.x >= crop.x && pt.x <= crop.x + crop.w && pt.y >= crop.y && pt.y <= crop.y + crop.h) {
      return "move";
    }
    return "pan";
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!imageLoaded) return;
    const pt = getCanvasCoords(e);
    const handle = getHitHandle(pt);

    dragRef.current = {
      active: true,
      handle,
      startX: pt.x,
      startY: pt.y,
      startCrop: { ...crop },
      startPan: { ...pan },
    };

    e.currentTarget.setPointerCapture(e.pointerId);

    if (handle === "move") setCursor("move");
    else if (handle === "tl" || handle === "br") setCursor("nwse-resize");
    else if (handle === "tr" || handle === "bl") setCursor("nesw-resize");
    else setCursor("grabbing");
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const pt = getCanvasCoords(e);

    if (!dragRef.current.active) {
      // Hover cursor
      const handle = getHitHandle(pt);
      if (handle === "move") setCursor("move");
      else if (handle === "tl" || handle === "br") setCursor("nwse-resize");
      else if (handle === "tr" || handle === "bl") setCursor("nesw-resize");
      else setCursor("grab");
      return;
    }

    const { handle, startX, startY, startCrop, startPan } = dragRef.current;
    const dx = pt.x - startX;
    const dy = pt.y - startY;

    if (!imgElement) return;
    const bounds = getImageBounds(imgElement, zoom, handle === "pan" ? { x: startPan.x + dx, y: startPan.y + dy } : pan);

    const activePreset = ASPECT_RATIOS.find((r) => r.id === selectedRatioId);
    const lockedRatio = activePreset?.ratio ?? null;

    if (handle === "pan") {
      setPan({
        x: startPan.x + dx,
        y: startPan.y + dy,
      });
      return;
    }

    if (handle === "move") {
      let nx = startCrop.x + dx;
      let ny = startCrop.y + dy;

      // Clamp within image bounds
      nx = Math.max(bounds.x, Math.min(bounds.x + bounds.w - startCrop.w, nx));
      ny = Math.max(bounds.y, Math.min(bounds.y + bounds.h - startCrop.h, ny));

      setCrop({
        x: nx,
        y: ny,
        w: startCrop.w,
        h: startCrop.h,
      });
      return;
    }

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

    setCrop({ x: nx, y: ny, w: nw, h: nh });
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    dragRef.current.active = false;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      // Ignored if capture already released
    }
    setCursor("default");
  };

  // Export cropped region to offscreen canvas at full native resolution
  const handleApplyCrop = () => {
    if (!imgElement || !imageLoaded || exporting) return;

    setExporting(true);
    try {
      const bounds = getImageBounds(imgElement, zoom, pan);

      // Scale factor mapping from display canvas coords to native image coords
      const scaleX = imgElement.naturalWidth / bounds.w;
      const scaleY = imgElement.naturalHeight / bounds.h;

      let srcX = (crop.x - bounds.x) * scaleX;
      let srcY = (crop.y - bounds.y) * scaleY;
      let srcW = crop.w * scaleX;
      let srcH = crop.h * scaleY;

      // Clamp to original image boundaries
      srcX = Math.max(0, Math.min(imgElement.naturalWidth, srcX));
      srcY = Math.max(0, Math.min(imgElement.naturalHeight, srcY));
      srcW = Math.max(1, Math.min(imgElement.naturalWidth - srcX, srcW));
      srcH = Math.max(1, Math.min(imgElement.naturalHeight - srcY, srcH));

      const offscreen = document.createElement("canvas");
      offscreen.width = Math.round(srcW);
      offscreen.height = Math.round(srcH);

      const offCtx = offscreen.getContext("2d");
      if (!offCtx) {
        setExporting(false);
        return;
      }

      offCtx.imageSmoothingEnabled = true;
      offCtx.imageSmoothingQuality = "high";
      offCtx.drawImage(
        imgElement,
        Math.round(srcX),
        Math.round(srcY),
        Math.round(srcW),
        Math.round(srcH),
        0,
        0,
        offscreen.width,
        offscreen.height
      );

      offscreen.toBlob(
        (blob) => {
          setExporting(false);
          if (!blob) return;
          const file = new File([blob], "cropped.png", { type: "image/png" });
          onCrop(file);
          onClose();
        },
        "image/png"
      );
    } catch {
      setExporting(false);
    }
  };

  return (
    <>
      {/* Modal Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "var(--sp-4) var(--sp-5)",
          borderBottom: "1px solid var(--line)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <Crop weight="bold" style={{ fontSize: "1.125rem", color: "var(--ink)" }} />
          <div>
            <h2
              id="d-crop-title"
              style={{ margin: 0, fontSize: "1rem", fontWeight: 650, color: "var(--ink)" }}
            >
              Potong Gambar
            </h2>
            <p style={{ margin: "2px 0 0", fontSize: "0.8125rem", color: "var(--muted)" }}>
              Sesuaikan area dan rasio pemotongan gambar.
            </p>
          </div>
        </div>
        <button
          type="button"
          className="d-icon-btn"
          aria-label="Tutup"
          onClick={onClose}
          style={{ color: "var(--muted)" }}
        >
          <X weight="bold" />
        </button>
      </div>

      {/* Modal Body */}
      <div
        className="d-dialog-body"
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "14px",
          padding: "var(--sp-4) var(--sp-5)",
        }}
      >
        {/* Preset aspect ratio buttons */}
        <div
          style={{
            display: "flex",
            gap: "8px",
            alignItems: "center",
            flexWrap: "wrap",
          }}
        >
          <span
            style={{
              fontSize: "0.8125rem",
              fontWeight: 600,
              color: "var(--ink-2)",
              marginRight: "4px",
            }}
          >
            Rasio:
          </span>
          {ASPECT_RATIOS.map((r) => {
            const active = selectedRatioId === r.id;
            return (
              <button
                key={r.id}
                type="button"
                className={`d-btn d-btn-sm ${active ? "d-btn-primary" : "d-btn-ghost"}`}
                style={!active ? { border: "1px solid var(--line)" } : undefined}
                onClick={() => handleRatioSelect(r)}
              >
                {r.label}
                {r.desc && (
                  <span style={{ opacity: 0.75, fontSize: "0.75rem", marginLeft: "4px" }}>
                    ({r.desc})
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {/* Canvas viewport container */}
        <div
          style={{
            position: "relative",
            width: "100%",
            background: "#111114",
            borderRadius: "var(--radius-lg)",
            overflow: "hidden",
            border: "1px solid var(--line)",
            boxShadow: "inset 0 1px 3px rgba(0, 0, 0, 0.4)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <canvas
            ref={canvasRef}
            width={CANVAS_W}
            height={CANVAS_H}
            style={{
              display: "block",
              width: "100%",
              height: "auto",
              maxHeight: "360px",
              touchAction: "none",
              cursor,
            }}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
          />

          {!imageLoaded && !loadError && (
            <div
              style={{
                position: "absolute",
                inset: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--muted)",
                fontSize: "0.875rem",
              }}
            >
              Memuat gambar…
            </div>
          )}

          {loadError && (
            <div
              style={{
                position: "absolute",
                inset: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--danger)",
                fontSize: "0.875rem",
                padding: "var(--sp-4)",
                textAlign: "center",
              }}
            >
              Gagal memuat gambar untuk dipotong.
            </div>
          )}
        </div>

        {/* Zoom scale slider and reset */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "12px",
            padding: "4px 0",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1 }}>
            <MagnifyingGlassMinus
              weight="bold"
              style={{ color: "var(--subtle)", flexShrink: 0 }}
              aria-hidden="true"
            />
            <input
              type="range"
              min={1}
              max={3}
              step={0.05}
              value={zoom}
              disabled={!imageLoaded}
              onChange={(e) => setZoom(parseFloat(e.target.value))}
              style={{ flex: 1, accentColor: "var(--brand-solid)" }}
              aria-label="Perbesaran gambar"
            />
            <MagnifyingGlassPlus
              weight="bold"
              style={{ color: "var(--subtle)", flexShrink: 0 }}
              aria-hidden="true"
            />
            <span
              style={{
                fontSize: "0.8125rem",
                fontVariantNumeric: "tabular-nums",
                width: "36px",
                textAlign: "right",
                color: "var(--ink-2)",
              }}
            >
              {zoom.toFixed(1)}x
            </span>
          </div>

          <button
            type="button"
            className="d-btn d-btn-sm d-btn-ghost"
            style={{ border: "1px solid var(--line)", flexShrink: 0 }}
            onClick={handleReset}
            disabled={!imageLoaded}
            title="Reset zoom dan posisi"
          >
            <ArrowCounterClockwise weight="bold" aria-hidden="true" />
            Reset
          </button>
        </div>
      </div>

      {/* Modal Footer */}
      <div
        className="d-dialog-foot"
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <span
          style={{
            fontSize: "0.75rem",
            color: "var(--muted)",
          }}
        >
          Geser untuk memposisikan, tarik sudut untuk mengubah ukuran.
        </span>
        <div style={{ display: "flex", gap: "8px" }}>
          <button type="button" className="d-btn" onClick={onClose} disabled={exporting}>
            Batal
          </button>
          <button
            type="button"
            className="d-btn d-btn-primary"
            onClick={handleApplyCrop}
            disabled={exporting || !imageLoaded}
          >
            <Check weight="bold" aria-hidden="true" />
            {exporting ? "Memproses…" : "Selesai"}
          </button>
        </div>
      </div>
    </>
  );
}

export function ImageCropModal({
  open,
  imageSrc,
  onClose,
  onCrop,
}: ImageCropModalProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  // Dialog native open/close + body scroll lock
  useEffect(() => {
    const d = dialogRef.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const prev = document.documentElement.style.overflow;
    document.documentElement.style.overflow = "hidden";
    return () => {
      document.documentElement.style.overflow = prev;
    };
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className="d-dialog"
      style={{
        width: "min(640px, calc(100vw - 32px))",
        maxWidth: "640px",
      }}
      aria-labelledby="d-crop-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {open && (
        <ImageCropContent
          key={`${imageSrc}-${open}`}
          imageSrc={imageSrc}
          onClose={onClose}
          onCrop={onCrop}
        />
      )}
    </dialog>
  );
}

export default ImageCropModal;
