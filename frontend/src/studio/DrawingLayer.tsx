"use client";

import { useCallback, useId, useRef, useState } from "react";

export interface DrawingStroke {
  id: string;
  target: "background" | string;
  color: string;
  size: number;
  opacity: number;
  points: [number, number][];
}

export interface DrawingLayerProps {
  target: "background" | string;
  strokes: DrawingStroke[];
  width: number;
  height: number;
  activeTool: null | "brush" | "eraser";
  currentConfig?: {
    color: string;
    size: number;
    opacity: number;
  };
  onAddStroke?: (stroke: DrawingStroke) => void;
  onEraseStroke?: (id: string) => void;
  className?: string;
}

/**
 * Konversi array titik koordinat [x, y] menjadi SVG path smooth
 * menggunakan quadratic bezier curve smoothing (M x0 y0 Q ...).
 */
export function pointsToPath(points: [number, number][]): string {
  if (!points || points.length === 0) return "";
  const first = points[0];
  if (!first) return "";
  if (points.length === 1) {
    return `M ${first[0]} ${first[1]} L ${first[0]} ${first[1]}`;
  }
  const second = points[1];
  if (points.length === 2 && second) {
    return `M ${first[0]} ${first[1]} Q ${first[0]} ${first[1]} ${second[0]} ${second[1]}`;
  }
  let d = `M ${first[0]} ${first[1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const curr = points[i];
    const next = points[i + 1];
    if (curr && next) {
      const xc = (curr[0] + next[0]) / 2;
      const yc = (curr[1] + next[1]) / 2;
      d += ` Q ${curr[0]} ${curr[1]} ${xc} ${yc}`;
    }
  }
  const last = points[points.length - 1];
  const secondLast = points[points.length - 2];
  if (secondLast && last) {
    d += ` Q ${secondLast[0]} ${secondLast[1]} ${last[0]} ${last[1]}`;
  }
  return d;
}

/**
 * Jarak kuadrat titik ke segmen garis untuk deteksi sapuan penghapus (eraser sweep).
 */
function distSqToSegment(p: [number, number], v: [number, number], w: [number, number]): number {
  const l2 = (v[0] - w[0]) ** 2 + (v[1] - w[1]) ** 2;
  if (l2 === 0) return (p[0] - v[0]) ** 2 + (p[1] - v[1]) ** 2;
  let t = ((p[0] - v[0]) * (w[0] - v[0]) + (p[1] - v[1]) * (w[1] - v[1])) / l2;
  t = Math.max(0, Math.min(1, t));
  return (p[0] - (v[0] + t * (w[0] - v[0]))) ** 2 + (p[1] - (v[1] + t * (w[1] - v[1]))) ** 2;
}

function isPointNearStroke(pt: [number, number], points: [number, number][], radius: number): boolean {
  const r2 = radius * radius;
  if (points.length === 1) {
    const p0 = points[0];
    if (!p0) return false;
    return (pt[0] - p0[0]) ** 2 + (pt[1] - p0[1]) ** 2 <= r2;
  }
  for (let i = 0; i < points.length - 1; i++) {
    const pA = points[i];
    const pB = points[i + 1];
    if (pA && pB && distSqToSegment(pt, pA, pB) <= r2) {
      return true;
    }
  }
  return false;
}

export function DrawingLayer({
  target,
  strokes,
  width,
  height,
  activeTool,
  currentConfig = { color: "#000000", size: 4, opacity: 1 },
  onAddStroke,
  onEraseStroke,
  className = "",
}: DrawingLayerProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [currentPoints, setCurrentPoints] = useState<[number, number][] | null>(null);
  const [isErasing, setIsErasing] = useState(false);

  const getCoordinates = useCallback(
    (e: React.PointerEvent<SVGSVGElement>): [number, number] => {
      const svg = svgRef.current;
      if (svg && typeof svg.getScreenCTM === "function" && typeof svg.createSVGPoint === "function") {
        const ctm = svg.getScreenCTM();
        if (ctm) {
          try {
            const inverse = ctm.inverse();
            let p = svg.createSVGPoint();
            p.x = e.clientX;
            p.y = e.clientY;
            p = p.matrixTransform(inverse);
            return [
              Math.max(0, Math.min(width, Math.round(p.x))),
              Math.max(0, Math.min(height, Math.round(p.y))),
            ];
          } catch {
            // Fallback ke hitungan bounding client rect bila inverse matrix gagal
          }
        }
      }
      const rect = e.currentTarget.getBoundingClientRect();
      const scaleX = rect.width ? width / rect.width : 1;
      const scaleY = rect.height ? height / rect.height : 1;
      return [
        Math.max(0, Math.min(width, Math.round((e.clientX - rect.left) * scaleX))),
        Math.max(0, Math.min(height, Math.round((e.clientY - rect.top) * scaleY))),
      ];
    },
    [width, height]
  );

  // Filter coretan yang targetnya cocok dengan target layer ini
  const targetStrokes = strokes.filter((s) => s.target === target);

  const checkErasure = useCallback(
    (pt: [number, number]) => {
      if (!onEraseStroke) return;
      for (const stroke of targetStrokes) {
        const eraseRadius = Math.max(stroke.size / 2, 16);
        if (isPointNearStroke(pt, stroke.points, eraseRadius)) {
          onEraseStroke(stroke.id);
        }
      }
    },
    [targetStrokes, onEraseStroke]
  );

  const handlePointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!activeTool) return;

    if (activeTool === "brush") {
      try {
        e.currentTarget.setPointerCapture?.(e.pointerId);
      } catch {
        // Abaikan jika tidak didukung di lingkungan non-DOM
      }
      const pt = getCoordinates(e);
      setCurrentPoints([pt]);
    } else if (activeTool === "eraser") {
      setIsErasing(true);
      const pt = getCoordinates(e);
      checkErasure(pt);
    }
  };

  const handlePointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!activeTool) return;

    if (activeTool === "brush" && currentPoints) {
      const pt = getCoordinates(e);
      // Cegah penambahan titik duplikat persis
      const last = currentPoints[currentPoints.length - 1];
      if (!last || last[0] !== pt[0] || last[1] !== pt[1]) {
        setCurrentPoints((prev) => (prev ? [...prev, pt] : [pt]));
      }
    } else if (activeTool === "eraser" && (isErasing || e.buttons === 1)) {
      const pt = getCoordinates(e);
      checkErasure(pt);
    }
  };

  const handlePointerUp = () => {
    if (activeTool === "brush" && currentPoints && currentPoints.length > 0) {
      const newStroke: DrawingStroke = {
        id: `stroke-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        target,
        color: currentConfig.color,
        size: currentConfig.size,
        opacity: currentConfig.opacity,
        points: currentPoints,
      };
      onAddStroke?.(newStroke);
      setCurrentPoints(null);
    }
    if (activeTool === "eraser") {
      setIsErasing(false);
    }
  };

  const pointerEventsMode = activeTool ? "all" : "none";

  return (
    <svg
      ref={svgRef}
      className={`st-drawing-layer ${className}`.trim()}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: "100%",
        height: "100%",
        pointerEvents: pointerEventsMode,
        touchAction: activeTool ? "none" : "auto",
        zIndex: target === "background" ? (activeTool ? 40 : 1) : 25,
        cursor: activeTool === "brush" ? "crosshair" : activeTool === "eraser" ? "crosshair" : "default",
      }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      {/* Coretan yang sudah ada */}
      {targetStrokes.map((s) => {
        const pathData = pointsToPath(s.points);
        return (
          <g key={s.id} data-stroke-id={s.id} data-target={s.target}>
            {/* Jalur coretan terlihat */}
            <path
              d={pathData}
              stroke={s.color}
              strokeWidth={s.size}
              strokeOpacity={s.opacity}
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
              style={{ pointerEvents: "none" }}
            />
            {/* Area hit tak terlihat untuk memudahkan klik/sapuan penghapus */}
            {activeTool === "eraser" && (
              <path
                d={pathData}
                stroke="transparent"
                strokeWidth={Math.max(s.size, 20)}
                strokeLinecap="round"
                strokeLinejoin="round"
                fill="none"
                style={{ pointerEvents: "stroke", cursor: "crosshair" }}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  onEraseStroke?.(s.id);
                }}
                onPointerEnter={(e) => {
                  if (e.buttons === 1 || isErasing) {
                    onEraseStroke?.(s.id);
                  }
                }}
              />
            )}
          </g>
        );
      })}

      {/* Coretan langsung saat kuas sedang aktif menggambar */}
      {activeTool === "brush" && currentPoints && currentPoints.length > 0 && (
        <path
          d={pointsToPath(currentPoints)}
          stroke={currentConfig.color}
          strokeWidth={currentConfig.size}
          strokeOpacity={currentConfig.opacity}
          strokeLinecap="round"
          strokeLinejoin="round"
          fill="none"
          style={{ pointerEvents: "none" }}
        />
      )}
    </svg>
  );
}
