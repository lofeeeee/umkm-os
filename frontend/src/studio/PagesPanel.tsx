"use client";

import { useId, useState } from "react";
import {
  Crown,
  Files,
  House,
  Plus,
  Trash,
  X,
} from "@phosphor-icons/react/ssr";

import { confirm } from "@/ui/dialog";
import { SYSTEM_PAGES, type SystemPage } from "./SystemPreview";
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
  /** Halaman sistem (keranjang, pesanan, akun) yang sedang dipratinjau; null = halaman website. */
  systemPage?: SystemPage | null;
  onSelectSystem?: (kind: SystemPage) => void;
  className?: string;
  onClose?: () => void;
}

/** Standar halaman: Halaman 1 = Beranda (tetap), halaman berikutnya bernama bebas. Paket membatasi jumlahnya
 *  (sama dengan `umkm_domain::website::max_pages`). */
export const STARTER_MAX_PAGES = 5;
export const PRO_MAX_PAGES = 30;
export const maxPages = (isPro: boolean) => (isPro ? PRO_MAX_PAGES : STARTER_MAX_PAGES);

/** Saran nama (mengisi kolom nama; tetap bisa diubah). */
export const NAME_SUGGESTIONS = (): string[] => [t("Tentang Kami"), t("Kontak"), t("Layanan"), t("Galeri"), t("Promo"), t("Blog")];

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
  systemPage = null,
  onSelectSystem,
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

  const max = maxPages(isPro);
  const full = pages.length >= max;

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

  const handleCreateCustom = async (e: React.FormEvent) => {
    e.preventDefault();
    if (full) {
      if (!isPro) onUpgradeClick();
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
          <p>{t("Halaman 1 selalu Beranda; halaman berikutnya bebas Anda beri nama. Semua tampil di menu navigasi.")}</p>
          <p className="st-page-count" data-full={full || undefined}>{t("{0} dari {1} halaman", String(pages.length), String(max))}</p>
        </div>
        {onClose && (
          <button type="button" className="d-icon-btn d-icon-btn-sm" onClick={onClose} title={t("Tutup panel")} aria-label={t("Tutup panel")}>
            <X weight="bold" />
          </button>
        )}
      </div>

      <ul className="st-page-list" aria-label={t("Daftar halaman")}>
        {[...pages].sort((a, b) => (a.slug === "home" ? -1 : b.slug === "home" ? 1 : 0)).map((page, i) => {
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
                  <span className="st-page-num">{t("Halaman {0}", String(i + 1))}</span>
                  <strong>{isHome ? t("Beranda") : page.title}</strong>
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

      {full ? (
        <div className="st-pro-note">
          <p>{isPro ? t("Batas {0} halaman tercapai. Hapus halaman yang tidak dipakai untuk menambah yang baru.", String(max)) : t("Paket Starter maksimal {0} halaman. Naik ke Pro untuk hingga {1} halaman.", String(max), String(PRO_MAX_PAGES))}</p>
          {!isPro && (
            <button type="button" className="d-btn d-btn-sm" onClick={onUpgradeClick}>
              <Crown weight="fill" aria-hidden="true" />
              {t("Tingkatkan ke Pro")}
            </button>
          )}
        </div>
      ) : !isAdding ? (
        <button type="button" className="st-add-page" onClick={() => setIsAdding(true)}>
          <Plus weight="bold" aria-hidden="true" />
          {t("Tambah halaman {0}", String(pages.length + 1))}
        </button>
      ) : (
        <form onSubmit={handleCreateCustom} className="st-add-card">
          <div className="st-add-card-head">
            <strong>{t("Halaman {0}", String(pages.length + 1))}</strong>
            <button type="button" className="d-icon-btn d-icon-btn-sm" onClick={() => { setIsAdding(false); setErrorMsg(null); }} title={t("Batal")} aria-label={t("Batal")}>
              <X weight="bold" />
            </button>
          </div>
          <label className="d-field" htmlFor={titleInputId}>
            <span>{t("Nama halaman")}</span>
            <input id={titleInputId} data-testid="input-custom-title" type="text" disabled={isSubmitting} autoFocus
              value={customTitle} onChange={(e) => handleTitleChange(e.target.value)} placeholder={t("Contoh: Layanan Kami")} maxLength={60} />
          </label>
          <div className="st-name-chips" role="group" aria-label={t("Saran nama")}>
            {NAME_SUGGESTIONS().filter((n) => !pages.some((p) => p.title === n)).map((n) => (
              <button key={n} type="button" className="st-chip" disabled={isSubmitting} onClick={() => handleTitleChange(n)}>{n}</button>
            ))}
          </div>
          <label className="d-field" htmlFor={slugInputId}>
            <span>{t("Alamat (URL)")}</span>
            <span className="st-slug">
              <span aria-hidden="true">/</span>
              <input id={slugInputId} data-testid="input-custom-slug" type="text" disabled={isSubmitting}
                value={customSlug} onChange={(e) => handleSlugChange(e.target.value)} placeholder="layanan-kami" maxLength={40} />
            </span>
          </label>
          {errorMsg && <p className="st-add-err" role="alert">{errorMsg}</p>}
          <div className="st-add-actions">
            <button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => { setIsAdding(false); setErrorMsg(null); }}>{t("Batal")}</button>
            <button type="submit" className="d-btn d-btn-sm d-btn-primary" disabled={isSubmitting || !customTitle.trim() || !customSlug.trim()}>
              <Plus weight="bold" aria-hidden="true" />
              {t("Buat halaman")}
            </button>
          </div>
        </form>
      )}
      {onSelectSystem && (
        <div className="st-add-sec u-mt-3">
          <span className="st-add-label">{t("Halaman sistem")}</span>
          <p className="d-hint u-m-0">{t("Keranjang, pesanan, dan akun dibuat otomatis dan mengikuti brand kit. Pratinjau & ubah teksnya di sini.")}</p>
          <ul className="st-page-list" aria-label={t("Halaman sistem")}>
            {SYSTEM_PAGES().map((p) => (
              <li key={p.kind} className="st-page" data-active={systemPage === p.kind ? "true" : "false"}>
                <button type="button" className="st-page-main" onClick={() => onSelectSystem(p.kind)} aria-current={systemPage === p.kind ? "page" : undefined}>
                  <span className="st-page-icon" aria-hidden="true"><Files weight={systemPage === p.kind ? "fill" : "duotone"} /></span>
                  <span className="st-page-text"><strong>{p.title}</strong><code>{p.path}</code></span>
                  <span className="st-page-badge">{t("Sistem")}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
