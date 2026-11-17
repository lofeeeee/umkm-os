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
import { t } from "@/i18n";

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
    get title() { return t("Tentang Kami"); },
    get description() { return t("Ceritakan profil, visi, dan kisah usaha Anda"); },
  },
  {
    slug: "contact",
    get title() { return t("Kontak"); },
    get description() { return t("Alamat toko, jam operasional, dan saluran komunikasi"); },
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

  const ok = await confirmFn({
    title: t("Hapus halaman?"),
    body: t("Halaman \"{0}\" ({1}) akan dihapus permanen. Tindakan ini tidak dapat dibatalkan.", page.title, formatSlugPath(
      page.slug
    )),
    confirmLabel: t("Hapus halaman"),
    cancelLabel: t("Batal"),
    danger: true,
  });

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
      setErrorMsg(err instanceof Error ? err.message : t("Gagal membuat halaman siap pakai"));
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
      setErrorMsg(t("Judul halaman wajib diisi."));
      return;
    }
    if (!slug) {
      setErrorMsg(t("Alamat URL wajib diisi."));
      return;
    }
    if (pages.some((p) => p.slug === slug)) {
      setErrorMsg(t("Alamat URL \"{0}\" sudah digunakan oleh halaman lain.", slug));
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
      setErrorMsg(err instanceof Error ? err.message : t("Gagal membuat halaman"));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDelete = async (e: React.MouseEvent, page: PageItem) => {
    e.stopPropagation();
    await confirmDeletePage(page, onDeletePage, confirm);
  };

  return (
    <div className={`st-pages ${className}`.trim()}>
      <div className="st-panel-head st-panel-head-row">
        <div>
          <h2>{t("Halaman")}</h2>
          <p>{t("Setiap halaman tampil di menu navigasi situs.")}</p>
        </div>
        {onClose && (
          <button type="button" className="d-icon-btn d-icon-btn-sm" onClick={onClose} title={t("Tutup panel")} aria-label={t("Tutup panel")}>
            <X weight="bold" />
          </button>
        )}
      </div>

      <ul className="st-page-list" aria-label={t("Daftar halaman")}>
        {pages.map((page) => {
          const isActive = page.slug === activeSlug;
          const isHome = page.slug === "home";
          const published = page.published_version != null;
          return (
            <li key={page.slug} className="st-page" data-page-slug={page.slug} data-active={isActive ? "true" : "false"}>
              <button type="button" className="st-page-main" onClick={() => onSelectPage(page.slug)} aria-current={isActive ? "page" : undefined}>
                <span className="st-page-icon" aria-hidden="true">
                  {isHome ? <House weight={isActive ? "fill" : "duotone"} /> : <Files weight={isActive ? "fill" : "duotone"} />}
                </span>
                <span className="st-page-text">
                  <strong>{page.title}</strong>
                  <code>{formatSlugPath(page.slug)}</code>
                </span>
                {isHome
                  ? <span className="st-page-badge">{t("Utama")}</span>
                  : page.published_version !== undefined && <span className={`st-page-badge${published ? " st-page-badge-ok" : ""}`}>{published ? t("Terbit") : t("Draf")}</span>}
              </button>
              <button
                type="button"
                className="st-page-del"
                data-delete-slug={page.slug}
                disabled={isHome}
                onClick={(e) => handleDelete(e, page)}
                title={isHome ? t("Halaman utama tidak dapat dihapus") : t("Hapus halaman {0}", page.title)}
                aria-label={isHome ? t("Halaman utama tidak dapat dihapus") : t("Hapus halaman {0}", page.title)}
              >
                <Trash weight="bold" />
              </button>
            </li>
          );
        })}
      </ul>

      {!isAdding ? (
        <button type="button" className="st-add-page" onClick={() => setIsAdding(true)}>
          <Plus weight="bold" aria-hidden="true" />
          {t("Tambah halaman")}
        </button>
      ) : (
        <div className="st-add-card">
          <div className="st-add-card-head">
            <strong>{t("Halaman baru")}</strong>
            <button type="button" className="d-icon-btn d-icon-btn-sm" onClick={() => { setIsAdding(false); setErrorMsg(null); }} title={t("Batal")} aria-label={t("Batal")}>
              <X weight="bold" />
            </button>
          </div>

          {availablePresets.length > 0 && (
            <div className="st-add-sec">
              <span className="st-add-label">{t("Siap pakai")}</span>
              <div className="st-presets">
                {availablePresets.map((preset) => (
                  <button key={preset.slug} type="button" className="st-preset" data-preset-slug={preset.slug} disabled={isSubmitting} onClick={() => handleCreatePreset(preset)}>
                    <span className="st-preset-text">
                      <strong>{preset.title}</strong>
                      <span>{preset.description}</span>
                    </span>
                    <span className="st-preset-add"><Plus weight="bold" aria-hidden="true" />{t("Tambah")}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <form onSubmit={handleCreateCustom} className="st-add-sec">
            <span className="st-add-label">
              {t("Halaman kustom")}
              {!isPro && <span className="st-pro-chip"><Crown weight="fill" aria-hidden="true" />{t("Paket Pro")}</span>}
            </span>

            {!isPro && (
              <div className="st-pro-note">
                <p>{t("Halaman kustom memerlukan paket Pro. Starter mendapatkan Beranda, Tentang Kami, dan Kontak.")}</p>
                <button type="button" className="d-btn d-btn-sm" onClick={onUpgradeClick}>
                  <LockSimple weight="bold" aria-hidden="true" />
                  {t("Tingkatkan ke Pro")}
                </button>
              </div>
            )}

            <label className="d-field" htmlFor={titleInputId}>
              <span>{t("Judul halaman")}</span>
              <input id={titleInputId} data-testid="input-custom-title" type="text" disabled={!isPro || isSubmitting}
                value={customTitle} onChange={(e) => handleTitleChange(e.target.value)} placeholder={t("Contoh: Layanan Kami")} maxLength={60} />
            </label>
            <label className="d-field" htmlFor={slugInputId}>
              <span>{t("Alamat (URL)")}</span>
              <span className="st-slug">
                <span aria-hidden="true">/</span>
                <input id={slugInputId} data-testid="input-custom-slug" type="text" disabled={!isPro || isSubmitting}
                  value={customSlug} onChange={(e) => handleSlugChange(e.target.value)} placeholder="layanan-kami" maxLength={40} />
              </span>
            </label>

            {errorMsg && <p className="st-add-err" role="alert">{errorMsg}</p>}

            {isPro && (
              <div className="st-add-actions">
                <button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => { setIsAdding(false); setErrorMsg(null); }}>{t("Batal")}</button>
                <button type="submit" className="d-btn d-btn-sm d-btn-primary" disabled={isSubmitting || !customTitle.trim() || !customSlug.trim()}>
                  <Plus weight="bold" aria-hidden="true" />
                  {t("Buat halaman")}
                </button>
              </div>
            )}
          </form>
        </div>
      )}
    </div>
  );
}
