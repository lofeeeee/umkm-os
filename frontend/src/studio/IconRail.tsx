"use client";

import { Files, PaintBrush, Palette, PuzzlePiece } from "@phosphor-icons/react/ssr";

export type StudioTab = "pages" | "elements" | "theme" | "draw";

export interface IconRailProps {
  activeTab: StudioTab | null;
  onSelectTab: (tab: StudioTab) => void;
}

export function IconRail({ activeTab, onSelectTab }: IconRailProps) {
  const tabs: { id: StudioTab; label: string; icon: typeof Files }[] = [
    { id: "pages", label: "Halaman", icon: Files },
    { id: "elements", label: "Elemen", icon: PuzzlePiece },
    { id: "theme", label: "Desain", icon: Palette },
    { id: "draw", label: "Coretan", icon: PaintBrush },
  ];

  return (
    <nav className="st-rail" aria-label="Bilah Alat Studio">
      {tabs.map((t) => {
        const Icon = t.icon;
        const active = activeTab === t.id;
        return (
          <button
            key={t.id}
            type="button"
            className={`st-rail-btn${active ? " active" : ""}`}
            onClick={() => onSelectTab(t.id)}
            aria-pressed={active}
            title={t.label}
          >
            <Icon weight={active ? "fill" : "bold"} size={22} />
            <span>{t.label}</span>
          </button>
        );
      })}
    </nav>
  );
}
