// Renderer dokumen Studio (`schema_version = 2`, ADR-0017) — SATU-SATUNYA sumber tampilan v2.
//
// Dipakai oleh situs publik DAN kanvas Studio lewat prop `mode`, sehingga "preview = terbit" dijamin
// secara struktural: tidak ada komponen visual kedua yang harus dijaga sinkron.
// - `customer` (default): tanpa pembungkus editor, tanpa EditContext — perilaku situs publik.
// - `studio-edit`: tiap blok dibungkus `EditableBlockWrapper`, teks bisa diketik langsung.
//
// Desktop: grid 12 kolom sesuai x/y/w/h dari editor. Ponsel (lebar < 720 px): blok ditumpuk berurutan
// (atas→bawah, kiri→kanan). `hide_on` dihormati. Plugin section memakai renderer v1 (`Site`) per blok.
// Semua teks dirender sebagai teks (React escaping).

import { EditContext, T, useEditing, type EditApi } from "./edit";
import type { CSSProperties, ReactNode } from "react";

import { ChatWidget } from "@/chat/widget";
import { EditableBlockWrapper } from "@/studio/EditableBlockWrapper";
import type { BlockV2, PageV2 } from "@/studio/doc";
import { pluginDef } from "@/studio/plugins";

import { PublicNav, type NavLink } from "./PublicNav";
import { fontStack, onColor, primaryVars, safeColor, text, waLink } from "./safe";
import { Site, type GridProduct } from "./Site";
import type { Business, OpenStatus } from "./types";

/** Halaman terbit yang bisa ditautkan dari navigasi (dari `/public/site`). */
export interface SitePageLink { slug: string; title: string; current?: boolean }

export type RenderMode = "customer" | "studio-edit";

export interface StudioSiteProps {
  doc: PageV2;
  business: Business;
  status?: OpenStatus | null;
  products?: GridProduct[];
  assetUrl: (ref: string) => string | undefined;
  /** Halaman terbit lain untuk tautan navigasi. Kosong = hanya nama toko (tanpa tautan). */
  pages?: SitePageLink[];
  /** "customer" (default) = situs publik; "studio-edit" = kanvas editor. */
  mode?: RenderMode;
  /** Id blok terpilih (hanya mode studio-edit). */
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
  /** Menulis `data[path]` pada blok (hanya mode studio-edit; dari `T` contentEditable). */
  onEditBlock?: (id: string, path: string, value: string) => void;
}

const isEmbed = (b: BlockV2) =>
  (b.kind === "chatbot" && (b.layout === "floating" || b.layout === "bar")) || (b.kind === "whatsapp" && b.layout === "floating");

/** `jam-buka` → `Jam buka` (label navigasi dari nama anchor, bukan slug mentah). */
function titleCase(s: string): string {
  const t = s.replace(/-/g, " ");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

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

/** Plugin Teks. Di Studio (EditContext) judul & isi bisa diketik langsung; di situs publik teks biasa. */
export function TextBlock({ b }: { b: Pick<BlockV2, "data"> }) {
  const editing = useEditing();
  const title = text(b.data.title);
  return (
    <div className="s-text">
      {(title || editing) && <h2 className="s-title"><T path="title" v={b.data.title} placeholder="Judul (opsional)" /></h2>}
      {editing
        ? <p><T path="body" v={b.data.body} multiline placeholder="Tulis paragraf di sini…" /></p>
        : text(b.data.body).split("\n").filter((p) => p.trim()).map((p, i) => <p key={i}>{p}</p>)}
    </div>
  );
}

/** Ikon WhatsApp (sama di editor & situs terbit — paritas DOM dijaga tes). */
function WaIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" fill="currentColor"><path d="M12 2a10 10 0 0 0-8.6 15.1L2 22l5-1.3A10 10 0 1 0 12 2Zm0 18.2a8.2 8.2 0 0 1-4.2-1.2l-.3-.2-3 .8.8-2.9-.2-.3A8.2 8.2 0 1 1 12 20.2Zm4.5-6.1c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.6.1l-.8 1c-.1.2-.3.2-.5.1a6.7 6.7 0 0 1-3.3-2.9c-.2-.4.2-.4.7-1.3.1-.1 0-.3 0-.4l-.8-1.8c-.2-.5-.4-.4-.6-.4h-.5a1 1 0 0 0-.7.3 3 3 0 0 0-.9 2.2 5.1 5.1 0 0 0 1.1 2.7 11.7 11.7 0 0 0 4.5 4c1.7.7 2.3.8 3.2.6.5-.1 1.5-.6 1.7-1.2.2-.6.2-1.1.1-1.2l-.4-.3Z" /></svg>
  );
}

/** Plugin WhatsApp: tautan wa.me ke nomor Profil bisnis. Di Studio label bisa diketik langsung. */
export function WhatsAppButton({ b, business, floating }: { b: Pick<BlockV2, "data">; business: Business; floating?: boolean }) {
  const editing = useEditing();
  const href = whatsappHref(business.profile.whatsapp ?? business.profile.phone, text(b.data.message));
  const label = text(b.data.label) || "Pesan via WhatsApp";
  if (editing) {
    // `<a>` tanpa href: elemen yang sama dengan situs terbit (paritas DOM), tetapi klik tidak
    // menavigasi sehingga label di dalamnya tetap bisa diketik.
    return (
      <a className="s-btn s-wa">
        <WaIcon />
        <T path="label" v={b.data.label} placeholder="Pesan via WhatsApp" />
      </a>
    );
  }
  if (!href) return floating ? null : <p className="s-muted">Nomor WhatsApp belum diatur di Profil bisnis.</p>;
  return (
    <a className={floating ? "s-wa-float" : "s-btn s-wa"} href={href} target="_blank" rel="noopener noreferrer" aria-label={floating ? label : undefined}>
      <WaIcon />
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
        content={{
          schema_version: 1,
          template: "studio",
          theme: {
            primary: props.doc.theme.primary,
            font: props.doc.theme.font,
            navbar_style: props.doc.theme.navbar_style,
            navbar_sticky: props.doc.theme.navbar_sticky,
            background_image: props.doc.theme.background_image,
            background_size: props.doc.theme.background_size,
            background_position: props.doc.theme.background_position,
            background_opacity: props.doc.theme.background_opacity,
          },
          sections: [{ id: b.id, type: b.kind, enabled: true, data: b.data }],
        }}
        business={props.business}
        status={props.status}
        products={props.products}
        assetUrl={props.assetUrl}
        slotWidth={(b.w / 12) * 1180}
      />
    );
  }
  return <div className={`st-content st-style-${b.style}${b.layout ? ` st-layout-${b.layout}` : ""}`}>{body}</div>;
}

/** Anchor berbahasa Indonesia yang dipakai tombol template (`#kontak`, `#menu`, …) → blok pertama
 *  jenis itu. ID blok sendiri acak (mis. `contact-t2d478i`), jadi tanpa ini tombol tidak menggulir
 *  (UAT 15 Nov customer #11). */
const ANCHORS: [string, string[]][] = [
  ["contact", ["kontak"]],
  ["product_grid", ["produk", "menu", "koleksi"]],
  ["gallery", ["karya", "galeri"]],
  ["location", ["lokasi"]],
  ["business_hours", ["jam-buka"]],
  ["testimonials", ["testimoni"]],
  ["about", ["tentang"]],
];

function anchorsFor(blocks: { id: string; kind: string }[]): Map<string, string[]> {
  const out = new Map<string, string[]>();
  const taken = new Set(blocks.map((b) => b.id));
  for (const [kind, names] of ANCHORS) {
    const first = blocks.find((b) => b.kind === kind);
    if (!first) continue;
    const free = names.filter((n) => !taken.has(n));
    free.forEach((n) => taken.add(n));
    if (free.length) out.set(first.id, free);
  }
  // Situs kerajinan tanpa galeri: "Lihat karya" ke galeri produk.
  const grid = blocks.find((b) => b.kind === "product_grid");
  if (grid && !taken.has("karya")) out.set(grid.id, [...(out.get(grid.id) ?? []), "karya"]);
  return out;
}

export function StudioSite(props: StudioSiteProps) {
  const { doc, mode = "customer" } = props;
  const editing = mode === "studio-edit";
  const navStyle = doc.theme.navbar_style === "side" ? "side" : "top";
  const isSideNav = navStyle === "side";
  const grid = mobileSorted(doc.blocks.filter((b) => !isEmbed(b)));
  const anchors = anchorsFor(grid);
  const chat = doc.blocks.find((b) => b.kind === "chatbot" && isEmbed(b));
  const wa = doc.blocks.find((b) => b.kind === "whatsapp" && isEmbed(b));
  const bgUrl = doc.theme.background_image ? (props.assetUrl(doc.theme.background_image) ?? doc.theme.background_image) : undefined;
  // Tautan navigasi: halaman terbit dari server. Tanpa daftar halaman (mis. pratinjau) tautan jatuh ke
  // anchor bagian di halaman ini supaya navigasi tidak kosong.
  const navLinks: NavLink[] = (props.pages ?? []).filter((p) => !p.current).length > 0
    ? (props.pages ?? []).map((p) => ({ href: p.slug === "home" ? "/" : `/${p.slug}`, label: p.title }))
    : Array.from(anchors.entries()).map(([id, names]) => ({ href: `#${names[0] ?? id}`, label: titleCase(names[0] ?? id) }));
  const logoUrl = props.business.profile.logo ? props.assetUrl(props.business.profile.logo) : undefined;
  const style = {
    ...primaryVars(doc.theme.primary),
    "--s-font": fontStack(doc.theme.font),
    "--s-size": doc.theme.text_scale ?? 1,
    ...(bgUrl ? { "--s-bg-img": `url("${bgUrl}")` } : {}),
    "--s-bg-size": doc.theme.background_size ?? "cover",
    "--s-bg-pos": doc.theme.background_position ?? "center",
    "--s-bg-opacity": ((doc.theme.background_opacity ?? 0) / 100).toString(),
  } as CSSProperties;
  return (
    <div
      className={`sv2 site${isSideNav ? " s-has-side-nav" : ""}${editing ? " sf-editing" : ""}`}
      data-surface={doc.theme.surface ?? "light"}
      data-mode={mode}
      style={style}
    >
      {bgUrl && (
        <div
          className="s-bg-overlay"
          aria-hidden="true"
          style={{
            position: "fixed",
            inset: 0,
            pointerEvents: "none",
            backgroundImage: "var(--s-bg-img)",
            backgroundSize: "var(--s-bg-size, cover)",
            backgroundPosition: "var(--s-bg-pos, center)",
            backgroundRepeat: doc.theme.background_size === "repeat" ? "repeat" : "no-repeat",
            opacity: "var(--s-bg-opacity, 0)",
            backdropFilter: "blur(12px)",
            WebkitBackdropFilter: "blur(12px)",
            zIndex: 0,
          }}
        />
      )}
      <div className="sv2-shell">
        <PublicNav
          style={navStyle}
          sticky={doc.theme.navbar_sticky === true}
          brand={props.business.profile.name}
          logoUrl={logoUrl}
          links={navLinks}
        />
        <div className="sv2-main">
          <div className="sv2-grid">
            {grid.map((b, i) => {
              const body = (
                <>
                  {anchors.get(b.id)?.map((a) => <span key={a} id={a} className="sv2-anchor" aria-hidden="true" />)}
                  <Block b={b} props={props} />
                </>
              );
              const cellStyle = {
                gridColumn: `${b.x + 1} / span ${b.w}`,
                gridRow: `${b.y + 1} / span ${b.h}`,
                ["--order" as string]: i,
                // Tinggi manual (auto_height=false) dipertahankan persis seperti di editor (baris 24 px + celah 12 px).
                ...(b.auto_height === false ? { minHeight: b.h * 36 } : {}),
              } as CSSProperties;
              // Mode editor: blok dibungkus pelapis seleksi. Mode publik: struktur apa adanya.
              if (editing) {
                return (
                  <div key={b.id} id={b.id} className="sv2-block" data-kind={b.kind} data-section-type={b.kind} data-hide={b.hide_on} style={cellStyle}>
                    <EditableBlockWrapper
                      blockId={b.id}
                      label={pluginDef(b.kind).name}
                      selected={props.selectedId === b.id}
                      locked={b.locked}
                      onSelect={() => props.onSelect?.(b.id)}
                    >
                      <EditContext.Provider value={editApiFor(b, props.onEditBlock)}>{body}</EditContext.Provider>
                    </EditableBlockWrapper>
                  </div>
                );
              }
              return (
                <div key={b.id} id={b.id} className="sv2-block" data-kind={b.kind} data-section-type={b.kind} data-hide={b.hide_on} style={cellStyle}>
                  {body}
                </div>
              );
            })}
          </div>
          {/* P9-9: kebijakan privasi & syarat selalu bisa dijangkau dari situs (chat & akun mengumpulkan data). */}
          <footer className="sv2-foot">
            <span>© {props.business.profile.name}</span>
            {/* UAT 15 Nov customer #5: keranjang/pesanan/akun terjangkau dari halaman depan juga. */}
            <nav aria-label="Belanja">
              <a href="/keranjang">Keranjang</a>
              <a href="/pesanan">Pesanan saya</a>
              <a href="/akun">Akun</a>
            </nav>
            <nav aria-label="Informasi toko">
              <a href="/kebijakan-privasi">Kebijakan privasi</a>
              <a href="/syarat-transaksi">Syarat transaksi</a>
            </nav>
          </footer>
        </div>
      </div>
      {chat && (editing
        // Di editor widget asli tidak dipasang: jangan memulai percakapan/panggilan API hanya karena
        // pratinjau. Markup kelasnya sama supaya bentuknya identik dengan situs terbit.
        ? <ChatPlaceholder b={chat} primary={doc.theme.primary} />
        : <ChatWidget variant={chat.layout === "bar" ? "bar" : "bubble"} title={text(chat.data.title)} welcome={text(chat.data.welcome)} startOpen={chat.chat_open === true} primary={safeColor(doc.theme.primary)} />)}
      {wa && (editing
        ? <WaPlaceholder b={wa} business={props.business} />
        : <WhatsAppButton b={wa} business={props.business} floating />)}
    </div>
  );
}

/** API edit untuk satu blok: menulis `data[path]` pada blok itu. */
function editApiFor(b: BlockV2, onEditBlock?: (id: string, path: string, value: string) => void): EditApi | null {
  if (!onEditBlock) return null;
  return {
    commit: (path, value) => onEditBlock(b.id, path, value),
  };
}

/** Pratinjau chatbot di editor — markup & kelas sama dengan `ChatWidget` versi tertutup. */
function ChatPlaceholder({ b, primary }: { b: BlockV2; primary: string }) {
  const title = text(b.data.title) || "Chat dengan toko";
  const welcome = text(b.data.welcome);
  const variant = b.layout === "bar" ? "bar" : "bubble";
  const style = { ["--chat-primary" as string]: safeColor(primary), ["--chat-on-primary" as string]: onColor(safeColor(primary)) } as CSSProperties;
  return (
    <div className={`chat chat-v-${variant}`} style={style} aria-hidden="true" data-editor-preview="">
      {variant === "bar" ? (
        <span className="chat-bar">
          <span className="chat-bar-who"><span className="chat-dot" />{title}</span>
          <span className="chat-bar-input">{welcome || "Tulis pertanyaan…"}</span>
          <span className="chat-bar-go">➤</span>
        </span>
      ) : (
        <span className="chat-fab">Chat</span>
      )}
    </div>
  );
}

/** Pratinjau tombol WhatsApp melayang di editor. */
function WaPlaceholder({ b, business }: { b: BlockV2; business: Business }) {
  const label = text(b.data.label) || "Pesan via WhatsApp";
  const href = whatsappHref(business.profile.whatsapp ?? business.profile.phone, text(b.data.message));
  if (!href) return null;
  return (
    <span className="s-wa-float" aria-hidden="true" data-editor-preview="">
      <svg viewBox="0 0 24 24" width="28" height="28" fill="currentColor"><path d="M12 2a10 10 0 0 0-8.6 15.1L2 22l5-1.3A10 10 0 1 0 12 2Zm0 18.2a8.2 8.2 0 0 1-4.2-1.2l-.3-.2-3 .8.8-2.9-.2-.3A8.2 8.2 0 1 1 12 20.2Zm4.5-6.1c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.6.1l-.8 1c-.1.2-.3.2-.5.1a6.7 6.7 0 0 1-3.3-2.9c-.2-.4.2-.4.7-1.3.1-.1 0-.3 0-.4l-.8-1.8c-.2-.5-.4-.4-.6-.4h-.5a1 1 0 0 0-.7.3 3 3 0 0 0-.9 2.2 5.1 5.1 0 0 0 1.1 2.7 11.7 11.7 0 0 0 4.5 4c1.7.7 2.3.8 3.2.6.5-.1 1.5-.6 1.7-1.2.2-.6.2-1.1.1-1.2l-.4-.3Z" /></svg>
      <span className="s-sr-only">{label}</span>
    </span>
  );
}
