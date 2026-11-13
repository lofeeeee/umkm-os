// Primitif tata letak halaman dasbor. Satu pola untuk semua halaman:
//   <PageHeader title description actions/>  →  <Card title actions>…</Card>  →  <EmptyState/>
// Teks memakai kamus label (src/lib/labels.ts) agar istilah konsisten.

import Link from "next/link";
import type { ReactNode } from "react";

import { ArrowLeft, MagnifyingGlass, Tray } from "@phosphor-icons/react/ssr";

export function PageHeader({
  title, description, actions, back,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  back?: { href?: string; onClick?: () => void; label: string };
}) {
  return (
    <div className="d-page-head">
      <div className="d-page-head-text">
        {back && (back.href
          ? <Link className="d-back" href={back.href}><ArrowLeft weight="bold" aria-hidden="true" />{back.label}</Link>
          : <button type="button" className="d-back" onClick={back.onClick}><ArrowLeft weight="bold" aria-hidden="true" />{back.label}</button>)}
        <h1>{title}</h1>
        {description && <p>{description}</p>}
      </div>
      {actions && <div className="d-page-head-actions">{actions}</div>}
    </div>
  );
}

/** Kartu dengan kepala opsional. `label` mengisi aria-label section (dipakai e2e: getByRole("region")). */
export function Card({
  title, description, actions, children, footer, flush, label, className, id,
}: {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  flush?: boolean;
  label?: string;
  className?: string;
  id?: string;
}) {
  const cls = `d-card${flush ? " d-card-flush" : ""}${className ? ` ${className}` : ""}`;
  return (
    <section className={cls} aria-label={label} id={id}>
      {(title || actions) && (
        <header className="d-card-head">
          <div className="d-card-head-text">
            {title && <h2>{title}</h2>}
            {description && <p>{description}</p>}
          </div>
          {actions && <div className="d-card-head-actions">{actions}</div>}
        </header>
      )}
      {children}
      {footer && <div className="d-card-foot">{footer}</div>}
    </section>
  );
}

export function EmptyState({
  icon: Icon = Tray, title, children, action,
}: {
  icon?: typeof Tray;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="d-empty">
      <span className="d-empty-icon" aria-hidden="true"><Icon weight="duotone" /></span>
      <p className="d-empty-title">{title}</p>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}

export type Tone = "ok" | "warn" | "danger" | "info" | "brand" | "neutral";
export function Badge({ tone = "neutral", children, plain }: { tone?: Tone; children: ReactNode; plain?: boolean }) {
  const t = tone === "neutral" ? "" : ` d-badge-${tone}`;
  return <span className={`d-badge${t}${plain ? " d-badge-plain" : ""}`}>{children}</span>;
}

export function Skeleton({ lines = 3 }: { lines?: number }) {
  return (
    <div aria-hidden="true">
      {Array.from({ length: lines }, (_, i) => (
        <span key={i} className="d-skel d-skel-line" style={{ width: `${92 - ((i * 17) % 40)}%` }} />
      ))}
    </div>
  );
}

/** Kerangka halaman saat data pertama dimuat: judul + blok, bukan teks "Memuat…". */
export function PageSkeleton() {
  return (
    <div aria-busy="true" aria-label="Memuat">
      <span className="d-skel d-skel-title" />
      <span className="d-skel d-skel-block" style={{ marginBottom: 16 }} />
      <span className="d-skel d-skel-block" style={{ height: 220 }} />
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return <span className="d-spinner" role={label ? "status" : undefined} aria-label={label} aria-hidden={label ? undefined : true} />;
}

export function SearchInput({ label, value, onChange, placeholder }: { label: string; value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <div className="d-search">
      <MagnifyingGlass aria-hidden="true" weight="bold" />
      <input type="search" aria-label={label} placeholder={placeholder ?? label} value={value} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

export function Stat({ label, value, note, href, warn, icon: Icon }: { label: string; value: ReactNode; note?: ReactNode; href?: string; warn?: boolean; icon?: typeof Tray }) {
  const body = (
    <>
      <span className="d-stat-label">{Icon && <Icon aria-hidden="true" weight="bold" />}{label}</span>
      <span className="d-stat-value">{value}</span>
      {note && <span className="d-stat-note">{note}</span>}
    </>
  );
  const cls = `d-stat${warn ? " d-stat-warn" : ""}`;
  return href ? <Link className={cls} href={href}>{body}</Link> : <div className={cls}>{body}</div>;
}

/** Tab berbasis tombol (role=tab). `count` ditampilkan sebagai pil. */
export function Tabs<T extends string>({
  label, value, onChange, items,
}: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  items: { value: T; label: string; count?: number }[];
}) {
  return (
    <div className="d-tabs" role="tablist" aria-label={label}>
      {items.map((it) => (
        <button
          key={it.value}
          type="button"
          role="tab"
          aria-selected={value === it.value}
          onClick={() => onChange(it.value)}
        >
          {it.label}
          {it.count !== undefined && it.count > 0 && <span className="d-tab-count">{it.count}</span>}
        </button>
      ))}
    </div>
  );
}

/** Tombol dengan status sibuk: label tetap terbaca, spinner kecil di depan. */
export function BusyButton({
  busy, children, className = "d-btn d-btn-primary", type = "submit", disabled, onClick, busyLabel,
}: {
  busy?: boolean;
  children: ReactNode;
  className?: string;
  type?: "submit" | "button";
  disabled?: boolean;
  onClick?: () => void;
  busyLabel?: string;
}) {
  return (
    <button type={type} className={className} disabled={busy || disabled} onClick={onClick} aria-busy={busy || undefined}>
      {busy && <span className="d-spinner" aria-hidden="true" />}
      {busy && busyLabel ? busyLabel : children}
    </button>
  );
}
