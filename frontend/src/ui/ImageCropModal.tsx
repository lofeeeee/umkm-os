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
import { t } from "@/i18n";
import {
  ASPECT_RATIOS, CANVAS_H, CANVAS_W, HANDLE_RADIUS, MIN_CROP_SIZE, PADDING, drawCropOverlay, moveCrop, resizeCrop,
  type AspectRatioPreset, type Rect,
} from "@/ui/crop-geometry";

export interface ImageCropModalProps {
  open: boolean;
  imageSrc: string;
  onClose: () => void;
  onCrop: (file: File) => void;
}


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

    drawCropOverlay(ctx, crop);
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

    setCrop(handle === "move" ? moveCrop(startCrop, dx, dy, bounds) : resizeCrop(handle, startCrop, dx, dy, bounds, lockedRatio));
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
        className="icm-head"
      >
        <div className="icm-title">
          <Crop weight="bold" className="icm-title-icon" />
          <div>
            <h2
              id="d-crop-title"
              className="icm-h"
            >
              {t("Potong gambar")}
            </h2>
            <p className="icm-sub">
              {t("Sesuaikan area dan rasio pemotongan gambar.")}
            </p>
          </div>
        </div>
        <button
          type="button"
          className="d-icon-btn u-muted"
          aria-label={t("Tutup jendela")}
          onClick={onClose}
        >
          <X weight="bold" />
        </button>
      </div>

      {/* Modal Body */}
      <div
        className="d-dialog-body icm-body"

      >
        {/* Preset aspect ratio buttons */}
        <div
          className="icm-ratios"
        >
          <span
            className="icm-label"
          >
            {t("Rasio:")}
          </span>
          {ASPECT_RATIOS.map((r) => {
            const active = selectedRatioId === r.id;
            return (
              <button
                key={r.id}
                type="button"
                className={`d-btn d-btn-sm ${active ? "d-btn-primary" : "d-btn-ghost icm-outline"}`}
                onClick={() => handleRatioSelect(r)}
              >
                {r.label}
                {r.desc && (
                  <span className="icm-ratio-hint">
                    ({r.desc})
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {/* Canvas viewport container */}
        <div
          className="icm-stage"
        >
          <canvas
            ref={canvasRef}
            width={CANVAS_W}
            height={CANVAS_H}
            className="icm-canvas" style={{ cursor }}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
          />

          {!imageLoaded && !loadError && (
            <div
              className="icm-overlay"
            >
              {t("Memuat gambar…")}
            </div>
          )}

          {loadError && (
            <div
              className="icm-overlay icm-overlay-error"
            >
              {t("Gagal memuat gambar untuk dipotong.")}
            </div>
          )}
        </div>

        {/* Zoom scale slider and reset */}
        <div
          className="icm-zoom"
        >
          <div className="icm-zoom-track">
            <MagnifyingGlassMinus
              weight="bold"
              className="icm-zoom-icon"
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
              className="icm-zoom-range"
              aria-label={t("Perbesaran gambar")}
            />
            <MagnifyingGlassPlus
              weight="bold"
              className="icm-zoom-icon"
              aria-hidden="true"
            />
            <span
              className="icm-zoom-val"
            >
              {zoom.toFixed(1)}x
            </span>
          </div>

          <button
            type="button"
            className="d-btn d-btn-sm d-btn-ghost icm-outline u-shrink-0"
            onClick={handleReset}
            disabled={!imageLoaded}
            title={t("Reset zoom dan posisi")}
          >
            <ArrowCounterClockwise weight="bold" aria-hidden="true" />
            {t("Reset")}
          </button>
        </div>
      </div>

      {/* Modal Footer */}
      <div
        className="d-dialog-foot icm-foot"

      >
        <span
          className="icm-hint"
        >
          {t("Geser untuk memposisikan, tarik sudut untuk mengubah ukuran.")}
        </span>
        <div className="icm-actions">
          <button type="button" className="d-btn" onClick={onClose} disabled={exporting}>
            {t("Batal")}
          </button>
          <button
            type="button"
            className="d-btn d-btn-primary"
            onClick={handleApplyCrop}
            disabled={exporting || !imageLoaded}
          >
            <Check weight="bold" aria-hidden="true" />
            {exporting ? t("Memproses…") : t("Terapkan potongan")}
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
      className="d-dialog icm-dialog"

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
