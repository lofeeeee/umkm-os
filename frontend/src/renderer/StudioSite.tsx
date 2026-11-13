// Renderer publik dokumen Studio (`schema_version = 2`, ADR-0017) — P9-4.
// Desktop: grid 12 kolom sesuai x/y/w/h dari editor. Ponsel (lebar < 720 px): blok ditumpuk berurutan
// (atas→bawah, kiri→kanan). `hide_on` dihormati. Plugin section memakai renderer v1 (`Site`) per blok
// sehingga tampilan = pratinjau editor. Semua teks dirender sebagai teks (React escaping).

import type { CSSProperties, ReactNode } from "react";

import { ChatWidget } from "@/chat/widget";
import type { BlockV2, PageV2 } from "@/studio/doc";

import { fontStack, safeColor, text, waLink } from "./safe";
import { Site, type GridProduct } from "./Site";
import type { Business, OpenStatus } from "./types";

export interface StudioSiteProps {
  doc: PageV2;
  business: Business;
  status?: OpenStatus | null;
  products?: GridProduct[];
  assetUrl: (ref: string) => string | undefined;
}

const isEmbed = (b: BlockV2) =>
  (b.kind === "chatbot" && (b.layout === "floating" || b.layout === "bar")) || (b.kind === "whatsapp" && b.layout === "floating");

/** Urutan tumpukan ponsel: y lalu x (sama dengan mobileOrder di editor). */
export function mobileSorted(blocks: BlockV2[]): BlockV2[] {
  return [...blocks].sort((a, b) => a.y - b.y || a.x - b.x || a.id.localeCompare(b.id));
}

/** Tautan wa.me dengan pesan terisi (nomor dari profil bisnis; tanpa nomor → null). */
export function whatsappHref(number: string | null | undefined, message?: string): string | null {
  const base = waLink(number);
  if (!base) return null;
  const msg = message?.trim();
  return msg ? `${base}?text=${encodeURIComponent(msg)}` : base;
}

function TextBlock({ b }: { b: BlockV2 }) {
  const title = text(b.data.title);
  return (
    <div className="s-text">
      {title && <h2 className="s-title">{title}</h2>}
      {text(b.data.body).split("\n").filter((p) => p.trim()).map((p, i) => <p key={i}>{p}</p>)}
    </div>
  );
}

function WhatsAppButton({ b, business, floating }: { b: BlockV2; business: Business; floating?: boolean }) {
  const href = whatsappHref(business.profile.whatsapp ?? business.profile.phone, text(b.data.message));
  const label = text(b.data.label) || "Pesan via WhatsApp";
  if (!href) return floating ? null : <p className="s-muted">Nomor WhatsApp belum diatur di Profil bisnis.</p>;
  return (
    <a className={floating ? "s-wa-float" : "s-btn s-wa"} href={href} target="_blank" rel="noopener noreferrer" aria-label={floating ? label : undefined}>
      <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" fill="currentColor"><path d="M12 2a10 10 0 0 0-8.6 15.1L2 22l5-1.3A10 10 0 1 0 12 2Zm0 18.2a8.2 8.2 0 0 1-4.2-1.2l-.3-.2-3 .8.8-2.9-.2-.3A8.2 8.2 0 1 1 12 20.2Zm4.5-6.1c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.6.1l-.8 1c-.1.2-.3.2-.5.1a6.7 6.7 0 0 1-3.3-2.9c-.2-.4.2-.4.7-1.3.1-.1 0-.3 0-.4l-.8-1.8c-.2-.5-.4-.4-.6-.4h-.5a1 1 0 0 0-.7.3 3 3 0 0 0-.9 2.2 5.1 5.1 0 0 0 1.1 2.7 11.7 11.7 0 0 0 4.5 4c1.7.7 2.3.8 3.2.6.5-.1 1.5-.6 1.7-1.2.2-.6.2-1.1.1-1.2l-.4-.3Z" /></svg>
      {!floating && <span>{label}</span>}
    </a>
  );
}

function Block({ b, props }: { b: BlockV2; props: StudioSiteProps }) {
  let body: ReactNode;
  if (b.kind === "text") body = <div className="site s-solo"><section className="s"><div className="s-inner"><TextBlock b={b} /></div></section></div>;
  else if (b.kind === "whatsapp") body = <div className="site s-solo"><section className="s"><div className="s-inner"><WhatsAppButton b={b} business={props.business} /></div></section></div>;
  else if (b.kind === "chatbot") {
    body = <ChatWidget variant="inline" title={text(b.data.title)} welcome={text(b.data.welcome)} startOpen={b.chat_open !== false} primary={safeColor(props.doc.theme.primary)} />;
  } else {
    body = (
      <Site
        content={{ schema_version: 1, template: "studio", theme: { primary: props.doc.theme.primary, font: props.doc.theme.font }, sections: [{ id: b.id, type: b.kind, enabled: true, data: b.data }] }}
        business={props.business}
        status={props.status}
        products={props.products}
        assetUrl={props.assetUrl}
      />
    );
  }
  return <div className={`st-content st-style-${b.style}${b.layout ? ` st-layout-${b.layout}` : ""}`}>{body}</div>;
}

export function StudioSite(props: StudioSiteProps) {
  const { doc } = props;
  const grid = mobileSorted(doc.blocks.filter((b) => !isEmbed(b)));
  const chat = doc.blocks.find((b) => b.kind === "chatbot" && isEmbed(b));
  const wa = doc.blocks.find((b) => b.kind === "whatsapp" && isEmbed(b));
  const style = {
    "--s-primary": safeColor(doc.theme.primary),
    "--s-font": fontStack(doc.theme.font),
    "--s-size": doc.theme.text_scale ?? 1,
  } as CSSProperties;
  return (
    <div className="sv2" data-surface={doc.theme.surface ?? "light"} style={style}>
      <div className="sv2-grid">
        {grid.map((b, i) => (
          <div
            key={b.id}
            id={b.id}
            className="sv2-block"
            data-kind={b.kind}
            data-section-type={b.kind}
            data-hide={b.hide_on}
            style={{
              gridColumn: `${b.x + 1} / span ${b.w}`,
              gridRow: `${b.y + 1} / span ${b.h}`,
              ["--order" as string]: i,
              // Tinggi manual (auto_height=false) dipertahankan persis seperti di editor (baris 24 px + celah 12 px).
              ...(b.auto_height === false ? { minHeight: b.h * 36 } : {}),
            } as CSSProperties}
          >
            <Block b={b} props={props} />
          </div>
        ))}
      </div>
      {chat && <ChatWidget variant={chat.layout === "bar" ? "bar" : "bubble"} title={text(chat.data.title)} welcome={text(chat.data.welcome)} startOpen={chat.chat_open === true} primary={safeColor(doc.theme.primary)} />}
      {wa && <WhatsAppButton b={wa} business={props.business} floating />}
    </div>
  );
}
