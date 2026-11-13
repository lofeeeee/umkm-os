"use client";

// Shell dasbor yang PERSISTEN (dipasang sekali di src/app/(app)/layout.tsx):
// sesi, daftar toko, dan toko aktif dimuat sekali lalu dipakai ulang saat berpindah halaman —
// sidebar tidak berkedip/hilang di setiap navigasi seperti sebelumnya.
//
// Halaman tetap memakai pola lama `<WithTenant>{(ctx) => …}</WithTenant>`; komponen itu kini
// hanya membaca konteks dari shell.

import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import {
  createContext, Suspense, useCallback, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode,
} from "react";

import {
  ArrowSquareOut,
  Buildings,
  CaretRight,
  CaretUpDown,
  ChartLineUp,
  ChatCircleDots,
  Check,
  CheckSquare,
  ClockCounterClockwise,
  CreditCard,
  GearSix,
  Globe,
  Headset,
  House,
  List,
  LockKey,
  MagnifyingGlass,
  Moon,
  Package,
  Receipt,
  ShieldCheck,
  SignOut,
  Sparkle,
  Stack,
  Storefront,
  Sun,
  Users,
  UsersThree,
  WarningCircle,
  CheckCircle,
  X,
} from "@phosphor-icons/react/ssr";

import { BellMenu } from "./bell";
import { API_ORIGIN, api, ApiError, isMfaRequired, loadSession, roleLabel, setCsrf, type Membership, type Session, type TenantView } from "@/lib/api";
import { PageSkeleton } from "@/ui/page";

/* ============================================================
   Tema
   ============================================================ */
export const THEME_KEY = "umkm-theme";
const THEME_EVENT = "umkm-theme-change";

export function setStoredTheme(mode: "light" | "dark" | null) {
  if (mode) {
    localStorage.setItem(THEME_KEY, mode);
    document.documentElement.dataset.theme = mode;
  } else {
    localStorage.removeItem(THEME_KEY);
    delete document.documentElement.dataset.theme;
  }
  window.dispatchEvent(new Event(THEME_EVENT));
}
function subscribeTheme(onChange: () => void) {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", onChange);
  window.addEventListener("storage", onChange);
  window.addEventListener(THEME_EVENT, onChange);
  return () => {
    mq.removeEventListener("change", onChange);
    window.removeEventListener("storage", onChange);
    window.removeEventListener(THEME_EVENT, onChange);
  };
}
function getThemeSnapshot(): "light" | "dark" {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch { /* storage diblokir */ }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}
const useHydrated = () => useSyncExternalStore(() => () => undefined, () => true, () => false);

export function ThemeToggle() {
  const theme = useSyncExternalStore(subscribeTheme, getThemeSnapshot, () => "light" as const);
  const hydrated = useHydrated();
  const label = theme === "dark" ? "Gunakan tema terang" : "Gunakan tema gelap";
  return (
    <button type="button" className="d-theme-toggle" onClick={() => setStoredTheme(theme === "dark" ? "light" : "dark")} aria-label={label} title={label}>
      {hydrated ? (theme === "dark" ? <Sun weight="bold" /> : <Moon weight="bold" />) : <span className="d-theme-dot" aria-hidden="true" />}
    </button>
  );
}

/* ============================================================
   Navigasi — satu sumber untuk sidebar, breadcrumb, dan ⌘K
   ============================================================ */
export interface NavItem { label: string; href: string; icon: typeof Globe; perm?: string; tenant?: boolean; keywords?: string }
export const NAV: { label: string; items: NavItem[] }[] = [
  {
    label: "",
    items: [{ label: "Beranda", href: "/", icon: House, keywords: "ringkasan dashboard home" }],
  },
  {
    label: "Penjualan",
    items: [
      { label: "Pesanan", href: "/orders", icon: Receipt, perm: "orders.read", tenant: true, keywords: "order transaksi bayar qris" },
      { label: "Produk", href: "/products", icon: Package, perm: "products.read", tenant: true, keywords: "katalog barang harga varian" },
      { label: "Stok", href: "/inventory", icon: Stack, perm: "inventory.read", tenant: true, keywords: "inventori persediaan" },
      { label: "Pelanggan", href: "/customers", icon: Users, perm: "customers.read", tenant: true, keywords: "customer pembeli" },
    ],
  },
  {
    label: "Layanan pelanggan",
    items: [
      { label: "Percakapan", href: "/conversations", icon: ChatCircleDots, perm: "conversations.read", tenant: true, keywords: "chat inbox pesan" },
      { label: "Tiket", href: "/tickets", icon: Headset, perm: "tickets.read", tenant: true, keywords: "keluhan masalah support" },
      { label: "Asisten AI", href: "/ai", icon: Sparkle, perm: "ai_config.read", tenant: true, keywords: "bot otomatis llm" },
    ],
  },
  {
    label: "Toko online",
    items: [
      { label: "Website", href: "/website", icon: Globe, tenant: true, keywords: "situs tampilan template terbitkan" },
      { label: "Pembayaran & pengiriman", href: "/shop-settings", icon: GearSix, perm: "tenant.read", tenant: true, keywords: "qris ongkir pengaturan toko checkout" },
      { label: "Profil bisnis", href: "/business", icon: Buildings, tenant: true, keywords: "alamat jam buka kontak" },
    ],
  },
  {
    label: "Laporan",
    items: [
      { label: "Analitik", href: "/analytics", icon: ChartLineUp, perm: "analytics.read", tenant: true, keywords: "omzet statistik laporan" },
      { label: "Log aktivitas", href: "/audit", icon: ClockCounterClockwise, perm: "audit.read", tenant: true, keywords: "audit riwayat jejak" },
    ],
  },
  {
    label: "Pengaturan",
    items: [
      { label: "Tim", href: "/team", icon: UsersThree, perm: "members.read", tenant: true, keywords: "anggota peran role undang staf" },
      { label: "Persetujuan", href: "/approvals", icon: CheckSquare, perm: "approvals.read", tenant: true, keywords: "approval izin" },
      { label: "Langganan", href: "/subscription", icon: CreditCard, perm: "tenant.read", tenant: true, keywords: "paket tagihan billing" },
      { label: "Keamanan", href: "/security", icon: LockKey, keywords: "mfa password dua langkah" },
    ],
  },
];
const PLATFORM_ITEM: NavItem = { label: "Admin platform", href: "/platform", icon: ShieldCheck, keywords: "super admin verifikasi" };

export function findNav(pathname: string): { group: string; item: NavItem } | null {
  for (const g of NAV) for (const it of g.items) if (it.href === pathname) return { group: g.label, item: it };
  if (pathname === PLATFORM_ITEM.href) return { group: "", item: PLATFORM_ITEM };
  return null;
}

/* ============================================================
   Konteks aplikasi
   ============================================================ */
export interface TenantCtx {
  session: Session;
  tenant: TenantView;
  can: (permission: string) => boolean;
}
interface AppState {
  session: Session | null;
  memberships: Membership[] | null;
  tenant: TenantView | null;
  tenantState: "none" | "loading" | "ready" | "error" | "pick";
  tenantError: string | null;
  platformAdmin: boolean;
  /** `?tenant=` untuk tautan (kosong bila tidak ada toko aktif). */
  q: string;
  can: (p: string) => boolean;
  reloadTenant: () => void;
  logout: () => Promise<void>;
}
const AppContext = createContext<AppState | null>(null);
export function useApp(): AppState {
  const v = useContext(AppContext);
  if (!v) throw new Error("useApp di luar AppShell");
  return v;
}

const LAST_TENANT = "umkm-last-tenant";
function lastTenant(): string | null {
  try { return localStorage.getItem(LAST_TENANT); } catch { return null; }
}
function rememberTenant(id: string) {
  try { localStorage.setItem(LAST_TENANT, id); } catch { /* abaikan */ }
}

function AppProvider({ children }: { children: ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const urlTenant = params.get("tenant");
  const [session, setSession] = useState<Session | null>(null);
  const [memberships, setMemberships] = useState<Membership[] | null>(null);
  const [loaded, setLoaded] = useState<{ id: string; tenant: TenantView | null; error: string | null } | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [platformAdmin, setPlatformAdmin] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  // 1) Sesi + daftar toko: sekali per pemasangan shell.
  useEffect(() => {
    (async () => {
      const s = await loadSession();
      if (!s) {
        router.replace("/login");
        return;
      }
      if (s.mfa_enrollment_required && pathname !== "/security") {
        router.replace("/security?reason=mfa");
      }
      setSession(s);
      setMemberships(await api<Membership[]>("GET", "/api/v1/tenants"));
      // Admin platform: probe senyap; tautan hanya muncul bila diizinkan.
      fetch(`${API_ORIGIN}/api/v1/platform/me`, { credentials: "include" })
        .then((r) => setPlatformAdmin(r.ok), () => undefined);
    })().catch(() => setFatal("Tidak dapat menghubungi server. Periksa koneksi lalu muat ulang halaman."));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- sekali per shell
  }, []);

  // 2) Toko aktif: dari URL, lalu toko terakhir dipakai, lalu satu-satunya toko.
  const wanted = useMemo(() => {
    if (!memberships) return undefined;
    if (urlTenant) return urlTenant;
    const last = lastTenant();
    if (last && memberships.some((m) => m.tenant_id === last)) return last;
    if (memberships.length === 1) return memberships[0]!.tenant_id;
    return null;
  }, [memberships, urlTenant]);

  useEffect(() => {
    if (!wanted || !session) return;
    // Tulis pilihan ke URL agar bisa di-bookmark & bertahan saat refresh.
    if (!urlTenant) {
      const url = new URL(window.location.href);
      url.searchParams.set("tenant", wanted);
      window.history.replaceState(null, "", url.toString());
    }
    let live = true;
    api<TenantView>("GET", `/api/v1/tenants/${wanted}`)
      .then((t) => {
        if (!live) return;
        setLoaded({ id: wanted, tenant: t, error: null });
        rememberTenant(t.id);
      })
      .catch((e) => {
        if (!live) return;
        if (isMfaRequired(e)) {
          router.replace(`/security?tenant=${wanted}&reason=mfa`);
          return;
        }
        setLoaded({
          id: wanted,
          tenant: null,
          error: e instanceof ApiError && (e.status === 404 || e.status === 403) ? "Toko tidak ditemukan atau Anda bukan anggotanya." : "Gagal memuat toko.",
        });
      });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- urlTenant hanya untuk menulis URL
  }, [wanted, session, reloadKey, router]);

  // Keadaan toko diturunkan dari data, bukan disimpan terpisah.
  const current = loaded && loaded.id === wanted ? loaded : null;
  const tenant = current?.tenant ?? null;
  const tenantState: AppState["tenantState"] = fatal
    ? "error"
    : wanted === undefined || !session
      ? "loading"
      : wanted === null
        ? (memberships && memberships.length > 1 ? "pick" : "none")
        : !current
          ? "loading"
          : current.tenant ? "ready" : "error";
  const tenantError = fatal ?? current?.error ?? null;

  const logout = useCallback(async () => {
    try {
      await api("POST", "/api/v1/auth/logout");
    } finally {
      setCsrf(null);
      router.replace("/login");
    }
  }, [router]);

  const value = useMemo<AppState>(() => ({
    session, memberships, tenant, tenantState, tenantError, platformAdmin,
    q: tenant ? `?tenant=${tenant.id}` : "",
    can: (p) => !!tenant?.my_permissions.includes(p),
    reloadTenant: () => setReloadKey((k) => k + 1),
    logout,
  }), [session, memberships, tenant, tenantState, tenantError, platformAdmin, logout]);

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

/* ============================================================
   Sidebar
   ============================================================ */
function initial(name: string) {
  return (name.trim()[0] ?? "T").toUpperCase();
}

function usePopover() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return { open, setOpen, ref };
}

function TenantSwitcher({ onNavigate }: { onNavigate: () => void }) {
  const { tenant, memberships } = useApp();
  const router = useRouter();
  const { open, setOpen, ref } = usePopover();
  const name = tenant?.name ?? "Pilih toko";
  const go = (id: string) => {
    setOpen(false);
    onNavigate();
    rememberTenant(id);
    router.push(`/?tenant=${id}`);
  };
  return (
    <div className="d-switcher" ref={ref}>
      <button type="button" className="d-switcher-btn" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className={`d-avatar${tenant ? "" : " d-avatar-muted"}`} aria-hidden="true">{tenant ? initial(tenant.name) : <Storefront weight="bold" />}</span>
        <span className="d-switcher-meta">
          <span className="d-switcher-name">{name}</span>
          <span className="d-switcher-sub">{tenant ? roleLabel(tenant.my_role) : "UMKM OS"}</span>
        </span>
        <CaretUpDown weight="bold" aria-hidden="true" />
      </button>
      {open && (
        <div className="d-pop" role="menu" style={{ left: 0, right: 0, top: "calc(100% + 4px)" }}>
          <div className="d-menu-label">Toko Anda</div>
          {(memberships ?? []).map((m) => (
            <button key={m.tenant_id} type="button" role="menuitem" className="d-menu-item" aria-current={m.tenant_id === tenant?.id} onClick={() => go(m.tenant_id)}>
              <span className="d-avatar" style={{ width: 20, height: 20, fontSize: "0.6875rem", borderRadius: 4 }} aria-hidden="true">{initial(m.name)}</span>
              <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.name}</span>
              {m.tenant_id === tenant?.id && <Check weight="bold" className="d-check-mark" aria-hidden="true" />}
            </button>
          ))}
          {tenant?.primary_hostname && (
            <>
              <div className="d-menu-sep" />
              <a className="d-menu-item" role="menuitem" href={`https://${tenant.primary_hostname}`} target="_blank" rel="noreferrer">
                <ArrowSquareOut weight="bold" aria-hidden="true" /> Lihat situs toko
              </a>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function SidebarNav({ onNavigate }: { onNavigate: () => void }) {
  const pathname = usePathname();
  const { tenant, can, q, platformAdmin } = useApp();
  const groups = platformAdmin ? [...NAV, { label: "Platform", items: [PLATFORM_ITEM] }] : NAV;
  return (
    <nav aria-label="Navigasi utama" className="d-sidebar-nav">
      {groups.map((group) => {
        const items = group.items.filter((it) => (!it.tenant || !!tenant) && (!it.perm || can(it.perm)));
        if (items.length === 0) return null;
        const labelId = `nav-${group.label.replace(/\W+/g, "-").toLowerCase() || "utama"}`;
        return (
          <div className="d-nav-group" key={group.label || "utama"} role="group" aria-labelledby={group.label ? labelId : undefined} aria-label={group.label ? undefined : "Utama"}>
            {group.label && <p className="d-nav-label" id={labelId}>{group.label}</p>}
            {items.map((it) => (
              <Link
                key={it.href}
                className="d-nav"
                href={`${it.href}${q}`}
                aria-current={pathname === it.href ? "page" : undefined}
                onClick={onNavigate}
              >
                <it.icon aria-hidden="true" weight={pathname === it.href ? "fill" : "regular"} />
                <span className="d-nav-text">{it.label}</span>
              </Link>
            ))}
          </div>
        );
      })}
    </nav>
  );
}

/* ============================================================
   Command palette (⌘K / Ctrl+K)
   ============================================================ */
interface Cmd { id: string; group: string; label: string; icon: typeof Globe; keywords?: string; run: () => void }

function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog ref={ref} className="d-dialog d-cmdk" aria-label="Cari halaman dan aksi" onCancel={(e) => { e.preventDefault(); onClose(); }} onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      {open && <CommandBody onClose={onClose} />}
    </dialog>
  );
}

function CommandBody({ onClose }: { onClose: () => void }) {
  const { tenant, can, q, memberships, platformAdmin } = useApp();
  const router = useRouter();
  const [text, setText] = useState("");
  const [sel, setSel] = useState(0);

  const cmds = useMemo<Cmd[]>(() => {
    const go = (href: string) => () => { onClose(); router.push(href); };
    const out: Cmd[] = [];
    const groups = platformAdmin ? [...NAV, { label: "Platform", items: [PLATFORM_ITEM] }] : NAV;
    for (const g of groups) for (const it of g.items) {
      if ((it.tenant && !tenant) || (it.perm && !can(it.perm))) continue;
      out.push({ id: `nav:${it.href}`, group: "Buka halaman", label: it.label, icon: it.icon, keywords: `${g.label} ${it.keywords ?? ""}`, run: go(`${it.href}${q}`) });
    }
    if (tenant) {
      if (can("products.create")) out.push({ id: "new-product", group: "Aksi cepat", label: "Tambah produk", icon: Package, keywords: "baru buat", run: go(`/products${q}&new=1`) });
      if (can("members.invite") || can("members.manage")) out.push({ id: "invite", group: "Aksi cepat", label: "Undang anggota tim", icon: UsersThree, keywords: "staf karyawan", run: go(`/team${q}`) });
      out.push({ id: "publish", group: "Aksi cepat", label: "Ubah & terbitkan website", icon: Globe, keywords: "edit situs", run: go(`/website${q}`) });
    }
    for (const m of memberships ?? []) {
      if (m.tenant_id === tenant?.id) continue;
      out.push({ id: `t:${m.tenant_id}`, group: "Pindah toko", label: m.name, icon: Storefront, keywords: m.slug, run: () => { onClose(); rememberTenant(m.tenant_id); router.push(`/?tenant=${m.tenant_id}`); } });
    }
    out.push({ id: "theme", group: "Preferensi", label: "Ganti tema terang/gelap", icon: Moon, keywords: "dark light mode", run: () => { setStoredTheme(getThemeSnapshot() === "dark" ? "light" : "dark"); onClose(); } });
    return out;
  }, [tenant, can, q, memberships, platformAdmin, router, onClose]);

  const norm = (s: string) => s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");
  const shown = useMemo(() => {
    const t = norm(text.trim());
    if (!t) return cmds;
    return cmds.filter((c) => t.split(/\s+/).every((w) => norm(`${c.label} ${c.keywords ?? ""} ${c.group}`).includes(w)));
  }, [cmds, text]);
  const cur = Math.min(sel, Math.max(shown.length - 1, 0));

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") { e.preventDefault(); setSel((cur + 1) % Math.max(shown.length, 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel((cur - 1 + shown.length) % Math.max(shown.length, 1)); }
    else if (e.key === "Enter") { e.preventDefault(); shown[cur]?.run(); }
  }

  let lastGroup = "";
  return (
        <>
          <div className="d-cmdk-input">
            <MagnifyingGlass weight="bold" aria-hidden="true" />
            <input
              autoFocus
              role="combobox"
              aria-expanded="true"
              aria-controls="d-cmdk-list"
              aria-activedescendant={shown[cur] ? `cmd-${shown[cur]!.id}` : undefined}
              aria-label="Cari halaman atau aksi"
              placeholder="Cari halaman atau aksi…"
              value={text}
              onChange={(e) => { setText(e.target.value); setSel(0); }}
              onKeyDown={onKey}
            />
          </div>
          <ul className="d-cmdk-list" id="d-cmdk-list" role="listbox">
            {shown.length === 0 && <li className="d-empty" style={{ padding: 24 }}><p className="d-muted">Tidak ada hasil untuk “{text}”.</p></li>}
            {shown.map((c, i) => {
              const head = c.group !== lastGroup ? c.group : null;
              lastGroup = c.group;
              return (
                <li key={c.id} id={`cmd-${c.id}`} role="option" aria-selected={i === cur} onMouseMove={() => setSel(i)}>
                  {head && <div className="d-cmdk-group" aria-hidden="true">{head}</div>}
                  <button type="button" tabIndex={-1} className="d-menu-item" onClick={c.run}>
                    <c.icon weight="bold" aria-hidden="true" />
                    {c.label}
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="d-cmdk-foot" aria-hidden="true">
            <span><kbd>↑</kbd> <kbd>↓</kbd> pilih</span>
            <span><kbd>Enter</kbd> buka</span>
            <span><kbd>Esc</kbd> tutup</span>
          </div>
        </>
  );
}

/* ============================================================
   Topbar + kerangka
   ============================================================ */
function Breadcrumbs() {
  const pathname = usePathname();
  const { tenant, q } = useApp();
  const nav = findNav(pathname);
  return (
    <nav className="d-crumbs" aria-label="Lokasi">
      {tenant && pathname !== "/" && (
        <>
          <Link href={`/${q}`}>{tenant.name.length > 24 ? `${tenant.name.slice(0, 22)}…` : tenant.name}</Link>
          <CaretRight weight="bold" aria-hidden="true" />
        </>
      )}
      <span aria-current="page">{nav?.item.label ?? (tenant?.name ?? "UMKM OS")}</span>
    </nav>
  );
}

function Shell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const app = useApp();
  const [navOpen, setNavOpen] = useState(false);
  const [cmdOpen, setCmdOpen] = useState(false);
  const closeNav = useCallback(() => setNavOpen(false), []);
  const closeCmd = useCallback(() => setCmdOpen(false), []);
  const isMac = useSyncExternalStore(() => () => undefined, () => /Mac|iPhone|iPad/.test(navigator.platform), () => false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCmdOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <a className="d-skip" href="#main">Lompat ke konten utama</a>
      <div className="d-frame" data-nav-open={navOpen ? "true" : undefined}>
        <aside className="d-sidebar" id="sidebar-nav" aria-label="Panel navigasi">
          <div className="d-sidebar-head">
            <TenantSwitcher onNavigate={closeNav} />
          </div>
          <SidebarNav onNavigate={closeNav} />
          <div className="d-sidebar-foot">
            <button type="button" className="d-btn d-btn-ghost d-btn-sm d-logout" onClick={() => void app.logout()}>
              <SignOut aria-hidden="true" weight="bold" />
              Keluar
            </button>
          </div>
        </aside>
        <div className="d-scrim" onClick={closeNav} aria-hidden="true" />
      </div>

      <header className="d-topbar">
        <button
          type="button"
          className="d-burger"
          aria-expanded={navOpen}
          aria-controls="sidebar-nav"
          aria-label={navOpen ? "Tutup menu navigasi" : "Buka menu navigasi"}
          onClick={() => setNavOpen(!navOpen)}
        >
          {navOpen ? <X weight="bold" /> : <List weight="bold" />}
        </button>
        <Breadcrumbs />
        <div className="d-topbar-actions">
          <button type="button" className="d-search-btn" onClick={() => setCmdOpen(true)} aria-label="Cari halaman atau aksi" aria-keyshortcuts="Control+K Meta+K">
            <MagnifyingGlass weight="bold" aria-hidden="true" />
            <span>Cari…</span>
            <kbd>{isMac ? "⌘" : "Ctrl"} K</kbd>
          </button>
          {app.tenant && <BellMenu tenantId={app.tenant.id} />}
          <ThemeToggle />
        </div>
      </header>

      <main id="main" className="d-main">
        <div className="d-page" key={pathname}>{children}</div>
      </main>
      <CommandPalette open={cmdOpen} onClose={closeCmd} />
    </>
  );
}

/** Dipasang di src/app/(app)/layout.tsx. */
export function AppShell({ children }: { children: ReactNode }) {
  return (
    <Suspense fallback={null}>
      <AppProvider>
        <Shell>{children}</Shell>
      </AppProvider>
    </Suspense>
  );
}

/* ============================================================
   API halaman
   ============================================================ */

/** Pemilih toko untuk pengguna dengan beberapa toko dan belum memilih. */
export function TenantPicker() {
  const { memberships } = useApp();
  return (
    <>
      <div className="d-page-head">
        <div className="d-page-head-text">
          <h1>Toko saya</h1>
          <p>Pilih toko yang ingin Anda kelola.</p>
        </div>
      </div>
      <div className="d-bento d-stagger">
        {(memberships ?? []).map((m) => (
          <Link key={m.tenant_id} className="d-card d-pick" href={`/?tenant=${m.tenant_id}`} onClick={() => rememberTenant(m.tenant_id)}>
            <span className="d-row" style={{ gap: 12, flexWrap: "nowrap" }}>
              <span className="d-avatar d-avatar-lg" aria-hidden="true">{initial(m.name)}</span>
              <span style={{ minWidth: 0 }}>
                <strong style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.name}</strong>
                <span className="d-muted d-small">{m.slug} · {roleLabel(m.role)}</span>
              </span>
              <CaretRight weight="bold" aria-hidden="true" style={{ marginLeft: "auto", color: "var(--subtle)" }} />
            </span>
          </Link>
        ))}
      </div>
    </>
  );
}

/**
 * Halaman yang butuh konteks toko. Data datang dari shell; komponen ini hanya menangani
 * keadaan muat/galat/pilih-toko secara seragam.
 */
export function WithTenant({ children }: { children: (ctx: TenantCtx) => ReactNode }) {
  const app = useApp();
  if (app.tenantState === "pick") return <TenantPicker />;
  if (app.tenantState === "none") {
    return (
      <div className="d-card d-empty">
        <p className="d-empty-title">Anda belum menjadi anggota toko mana pun</p>
        <p>Minta undangan dari pemilik toko, atau daftarkan usaha Anda sendiri.</p>
        <Link className="d-btn d-btn-primary" href="/register">Daftarkan usaha</Link>
      </div>
    );
  }
  if (app.tenantState === "error") {
    return (
      <>
        <Alert kind="error">{app.tenantError ?? "Gagal memuat toko."}</Alert>
        <Link className="d-btn" href="/">Kembali ke beranda</Link>
      </>
    );
  }
  if (app.tenantState !== "ready" || !app.tenant || !app.session) return <PageSkeleton />;
  return <>{children({ session: app.session, tenant: app.tenant, can: app.can })}</>;
}

export function Alert({ kind, children }: { kind: "error" | "ok" | "warn" | "info"; children: ReactNode }) {
  if (!children) return null;
  const Icon = kind === "ok" ? CheckCircle : WarningCircle;
  return (
    <div className={`d-alert d-alert-${kind}`} role={kind === "error" ? "alert" : "status"}>
      <Icon weight="fill" aria-hidden="true" />
      <div>{children}</div>
    </div>
  );
}
