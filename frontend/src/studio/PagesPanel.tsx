"use client";

import { useId, useState } from "react";
import {
  Crown,
  Files,
  House,
  LockSimple,
  Plus,
  Trash,
  X,
} from "@phosphor-icons/react/ssr";

import { confirm } from "@/ui/dialog";

export interface PageItem {
  slug: string;
  title: string;
  draft_revision?: number;
  published_version?: number | null;
}

export interface PagesPanelProps {
  pages: PageItem[];
  activeSlug: string;
  onSelectPage: (slug: string) => void;
  onCreatePage: (slug: string, title: string) => Promise<boolean>;
  onDeletePage: (slug: string) => Promise<boolean>;
  isPro: boolean;
  onUpgradeClick: () => void;
  defaultOpenAdd?: boolean;
  className?: string;
  onClose?: () => void;
}

export interface PagePreset {
  slug: string;
  title: string;
  description: string;
}

export const STANDARD_PRESETS: PagePreset[] = [
  {
    slug: "about",
    title: "Tentang Kami",
    description: "Ceritakan profil, visi, dan kisah usaha Anda",
  },
  {
    slug: "contact",
    title: "Kontak",
    description: "Alamat toko, jam operasional, dan saluran komunikasi",
  },
];

export function formatSlugPath(slug: string): string {
  if (!slug || slug === "home") return "/";
  return slug.startsWith("/") ? slug : `/${slug}`;
}

export function cleanSlugString(raw: string): string {
  return raw
    .toLowerCase()
    .trim()
    .replace(/^\/+|\/+$/g, "")
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export async function confirmDeletePage(
  page: PageItem,
  onDeletePage: (slug: string) => Promise<boolean>,
  confirmFn: (opts: {
    title: string;
    body?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
  }) => Promise<boolean> = confirm
): Promise<boolean> {
  if (page.slug === "home") {
    return false;
  }

  let ok = false;
  try {
    ok = await confirmFn({
      title: "Hapus Halaman?",
      body: `Halaman "${page.title}" (${formatSlugPath(
        page.slug
      )}) akan dihapus permanen. Tindakan ini tidak dapat dibatalkan.`,
      confirmLabel: "Hapus Halaman",
      cancelLabel: "Batal",
      danger: true,
    });
  } catch {
    if (typeof window !== "undefined" && window.confirm) {
      ok = window.confirm(`Apakah Anda yakin ingin menghapus halaman "${page.title}"?`);
    }
  }

  if (ok) {
    await onDeletePage(page.slug);
    return true;
  }

  return false;
}

export function PagesPanel({
  pages,
  activeSlug,
  onSelectPage,
  onCreatePage,
  onDeletePage,
  isPro,
  onUpgradeClick,
  defaultOpenAdd = false,
  className = "",
  onClose,
}: PagesPanelProps) {
  const [isAdding, setIsAdding] = useState(defaultOpenAdd);
  const [customTitle, setCustomTitle] = useState("");
  const [customSlug, setCustomSlug] = useState("");
  const [slugManuallyEdited, setSlugManuallyEdited] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const titleInputId = useId();
  const slugInputId = useId();

  const availablePresets = STANDARD_PRESETS.filter(
    (preset) => !pages.some((p) => p.slug === preset.slug)
  );

  const handleTitleChange = (val: string) => {
    setCustomTitle(val);
    setErrorMsg(null);
    if (!slugManuallyEdited) {
      setCustomSlug(cleanSlugString(val));
    }
  };

  const handleSlugChange = (val: string) => {
    setSlugManuallyEdited(true);
    setCustomSlug(cleanSlugString(val));
    setErrorMsg(null);
  };

  const handleCreatePreset = async (preset: PagePreset) => {
    setErrorMsg(null);
    setIsSubmitting(true);
    try {
      const ok = await onCreatePage(preset.slug, preset.title);
      if (ok) {
        setIsAdding(false);
      }
    } catch (err: unknown) {
      setErrorMsg(err instanceof Error ? err.message : "Gagal membuat halaman preset");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCreateCustom = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isPro) {
      onUpgradeClick();
      return;
    }

    const title = customTitle.trim();
    const slug = cleanSlugString(customSlug);

    if (!title) {
      setErrorMsg("Judul halaman wajib diisi.");
      return;
    }
    if (!slug) {
      setErrorMsg("Slug URL wajib diisi.");
      return;
    }
    if (pages.some((p) => p.slug === slug)) {
      setErrorMsg(`Slug URL "${slug}" sudah digunakan oleh halaman lain.`);
      return;
    }

    setErrorMsg(null);
    setIsSubmitting(true);
    try {
      const ok = await onCreatePage(slug, title);
      if (ok) {
        setCustomTitle("");
        setCustomSlug("");
        setSlugManuallyEdited(false);
        setIsAdding(false);
      }
    } catch (err: unknown) {
      setErrorMsg(err instanceof Error ? err.message : "Gagal membuat halaman");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async (e: React.MouseEvent, page: PageItem) => {
    e.stopPropagation();
    await confirmDeletePage(page, onDeletePage, confirm);
  };

  return (
    <div
      className={`st-pages-panel bg-white border border-neutral-200 rounded-xl shadow-lg p-4 w-80 text-neutral-800 space-y-4 ${className}`.trim()}
      style={{ fontFamily: "inherit" }}
    >
      {/* Header Panel */}
      <div className="flex items-center justify-between border-b border-neutral-100 pb-2">
        <div className="flex items-center gap-2">
          <span className="p-1.5 rounded-lg bg-indigo-50 text-indigo-600">
            <Files weight="bold" className="w-4 h-4" />
          </span>
          <div>
            <h3 className="font-semibold text-sm text-neutral-900 leading-tight">
              Halaman Website
            </h3>
            <p className="text-[11px] text-neutral-500 leading-tight">
              Kelola struktur dan navigasi multi-halaman toko
            </p>
          </div>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded-md text-neutral-400 hover:text-neutral-600 hover:bg-neutral-100 transition-colors"
            title="Tutup panel"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* List of Pages */}
      <div className="space-y-1.5" role="list" aria-label="Daftar Halaman">
        {pages.map((page) => {
          const isActive = page.slug === activeSlug;
          const isHome = page.slug === "home";
          const path = formatSlugPath(page.slug);

          return (
            <div
              key={page.slug}
              role="listitem"
              data-page-slug={page.slug}
              data-active={isActive ? "true" : "false"}
              onClick={() => onSelectPage(page.slug)}
              className={`group flex items-center justify-between p-2 rounded-lg border text-left cursor-pointer transition-all ${
                isActive
                  ? "border-indigo-600 bg-indigo-50/50 text-indigo-900 shadow-xs"
                  : "border-neutral-200 hover:bg-neutral-50 hover:border-neutral-300 text-neutral-700"
              }`}
              aria-current={isActive ? "page" : undefined}
            >
              <div className="flex items-center gap-2.5 min-w-0 flex-1 mr-2">
                <span
                  className={`p-1 rounded ${
                    isActive ? "text-indigo-600 bg-white" : "text-neutral-400"
                  }`}
                >
                  {isHome ? (
                    <House weight={isActive ? "fill" : "bold"} className="w-4 h-4" />
                  ) : (
                    <Files weight={isActive ? "fill" : "regular"} className="w-4 h-4" />
                  )}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-semibold truncate block">
                      {page.title}
                    </span>
                    {isHome && (
                      <span className="px-1.5 py-0.2 rounded text-[9px] font-semibold tracking-wider uppercase bg-neutral-200/80 text-neutral-700">
                        Utama
                      </span>
                    )}
                  </div>
                  <span className="text-[10px] text-neutral-400 font-mono block truncate">
                    {path}
                  </span>
                </div>
              </div>

              {/* Action Buttons */}
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  data-delete-slug={page.slug}
                  disabled={isHome}
                  onClick={(e) => handleDelete(e, page)}
                  className={`p-1.5 rounded text-neutral-400 transition-colors ${
                    isHome
                      ? "opacity-25 cursor-not-allowed"
                      : "hover:text-rose-600 hover:bg-rose-50 cursor-pointer"
                  }`}
                  title={
                    isHome
                      ? "Halaman utama tidak dapat dihapus"
                      : `Hapus halaman ${page.title}`
                  }
                  aria-label={
                    isHome
                      ? "Halaman utama tidak dapat dihapus"
                      : `Hapus halaman ${page.title}`
                  }
                >
                  <Trash className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Button to toggle Add Page Dialog/Section */}
      {!isAdding ? (
        <button
          type="button"
          onClick={() => setIsAdding(true)}
          className="w-full flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg border border-dashed border-indigo-300 text-indigo-600 hover:bg-indigo-50/50 hover:border-indigo-400 text-xs font-semibold transition-all cursor-pointer"
        >
          <Plus weight="bold" className="w-3.5 h-3.5" />
          <span>Tambah Halaman</span>
        </button>
      ) : (
        <div className="border border-neutral-200 bg-neutral-50/60 rounded-lg p-3 space-y-3.5">
          <div className="flex items-center justify-between border-b border-neutral-200 pb-2">
            <h4 className="text-xs font-semibold text-neutral-800">
              Tambah Halaman Baru
            </h4>
            <button
              type="button"
              onClick={() => {
                setIsAdding(false);
                setErrorMsg(null);
              }}
              className="text-neutral-400 hover:text-neutral-600 p-0.5 rounded"
              title="Batal"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Standard Presets Section */}
          {availablePresets.length > 0 && (
            <div>
              <label className="block text-[11px] font-medium text-neutral-500 mb-1.5">
                Halaman Standar Cepat
              </label>
              <div className="grid grid-cols-1 gap-1.5">
                {availablePresets.map((preset) => (
                  <button
                    key={preset.slug}
                    type="button"
                    data-preset-slug={preset.slug}
                    disabled={isSubmitting}
                    onClick={() => handleCreatePreset(preset)}
                    className="flex items-center justify-between p-2 rounded-lg border border-neutral-200 bg-white hover:border-indigo-300 hover:bg-indigo-50/30 text-left transition-colors cursor-pointer disabled:opacity-50"
                  >
                    <div>
                      <span className="text-xs font-semibold text-neutral-800 block">
                        {preset.title}
                      </span>
                      <span className="text-[10px] text-neutral-400 font-mono">
                        /{preset.slug}
                      </span>
                    </div>
                    <span className="flex items-center gap-1 text-[11px] font-medium text-indigo-600">
                      <Plus weight="bold" className="w-3 h-3" />
                      <span>Tambah</span>
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Custom Page Section */}
          <form onSubmit={handleCreateCustom} className="space-y-2.5">
            <div className="flex items-center justify-between">
              <label
                htmlFor={titleInputId}
                className="text-[11px] font-medium text-neutral-600"
              >
                Halaman Kustom
              </label>
              {!isPro && (
                <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-amber-100 text-amber-800 border border-amber-200">
                  <Crown weight="fill" className="w-3 h-3 text-amber-600" />
                  <span>Paket Pro</span>
                </span>
              )}
            </div>

            {!isPro && (
              <div className="p-2.5 rounded-lg bg-amber-50/80 border border-amber-200 text-amber-900 space-y-2">
                <p className="text-[11px] leading-relaxed">
                  Halaman kustom memerlukan paket Pro. Starter mendapatkan Beranda,
                  Tentang Kami, dan Kontak.
                </p>
                <button
                  type="button"
                  onClick={onUpgradeClick}
                  className="w-full flex items-center justify-center gap-1.5 py-1.5 px-2.5 rounded-md bg-amber-500 hover:bg-amber-600 text-white text-xs font-semibold shadow-xs transition-colors cursor-pointer"
                >
                  <LockSimple weight="bold" className="w-3.5 h-3.5" />
                  <span>Upgrade ke Pro</span>
                </button>
              </div>
            )}

            <div>
              <label
                htmlFor={titleInputId}
                className="block text-[10px] font-medium text-neutral-500 mb-1"
              >
                Judul Halaman
              </label>
              <input
                id={titleInputId}
                data-testid="input-custom-title"
                type="text"
                disabled={!isPro || isSubmitting}
                value={customTitle}
                onChange={(e) => handleTitleChange(e.target.value)}
                placeholder="Contoh: Layanan Kami"
                className="w-full px-2.5 py-1.5 text-xs rounded-md border border-neutral-200 bg-white text-neutral-800 placeholder-neutral-400 focus:outline-hidden focus:ring-1 focus:ring-indigo-500 disabled:bg-neutral-100 disabled:text-neutral-400 disabled:cursor-not-allowed"
              />
            </div>

            <div>
              <label
                htmlFor={slugInputId}
                className="block text-[10px] font-medium text-neutral-500 mb-1"
              >
                Slug URL
              </label>
              <div className="flex items-center rounded-md border border-neutral-200 bg-white focus-within:ring-1 focus-within:ring-indigo-500 overflow-hidden">
                <span className="px-2 text-xs font-mono text-neutral-400 bg-neutral-50 border-r border-neutral-200 select-none">
                  /
                </span>
                <input
                  id={slugInputId}
                  data-testid="input-custom-slug"
                  type="text"
                  disabled={!isPro || isSubmitting}
                  value={customSlug}
                  onChange={(e) => handleSlugChange(e.target.value)}
                  placeholder="layanan-kami"
                  className="w-full px-2 py-1.5 text-xs font-mono text-neutral-800 placeholder-neutral-400 focus:outline-hidden disabled:bg-neutral-100 disabled:text-neutral-400 disabled:cursor-not-allowed"
                />
              </div>
            </div>

            {errorMsg && (
              <p className="text-[11px] text-rose-600 font-medium leading-tight">
                {errorMsg}
              </p>
            )}

            {isPro && (
              <div className="flex items-center justify-end gap-2 pt-1">
                <button
                  type="button"
                  onClick={() => {
                    setIsAdding(false);
                    setErrorMsg(null);
                  }}
                  className="px-2.5 py-1.5 text-xs font-medium text-neutral-600 hover:text-neutral-800 hover:bg-neutral-100 rounded-md transition-colors"
                >
                  Batal
                </button>
                <button
                  type="submit"
                  disabled={isSubmitting || !customTitle.trim() || !customSlug.trim()}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold bg-indigo-600 hover:bg-indigo-700 text-white rounded-md shadow-xs transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <Plus weight="bold" className="w-3.5 h-3.5" />
                  <span>Buat Halaman</span>
                </button>
              </div>
            )}
          </form>
        </div>
      )}
    </div>
  );
}
