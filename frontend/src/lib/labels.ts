// Kamus label & format tunggal untuk dasbor (pedoman: docs/ux/pedoman-label.md).
// Aturan singkat:
//  - Bahasa Indonesia, huruf kapital hanya di awal kalimat ("Tambah produk", bukan "Tambah Produk").
//  - Tombol = kata kerja + objek bila perlu: Simpan, Batal, Tambah produk, Hapus, Arsipkan.
//  - Status = kata sifat/keadaan pendek ("Menunggu pembayaran"), warnanya dari tone di sini.
//  - Angka uang selalu `rupiah()`, tanggal selalu `tanggal()`/`waktu()`.

import type { Tone } from "@/ui/page";
import { intlLocale, t, tmap } from "@/i18n";

export interface StatusDef { label: string; tone: Tone }
/** Label status diterjemahkan saat dibaca (getter), jadi ikut bahasa aktif. */
const S = (label: string, tone: Tone): StatusDef => ({ get label() { return t(label); }, tone });

/** Ambil label + tone; status yang tak dikenal ditampilkan apa adanya dengan tone netral. */
export function status(map: Record<string, StatusDef>, key: string | null | undefined): StatusDef {
  if (!key) return { label: "—", tone: "neutral" };
  return map[key] ?? { label: key, tone: "neutral" };
}

export const ORDER_STATUS: Record<string, StatusDef> = {
  PENDING: S("Menunggu", "warn"),
  AWAITING_PAYMENT: S("Menunggu pembayaran", "warn"),
  PAID: S("Dibayar", "info"),
  APPROVAL_PENDING: S("Menunggu persetujuan", "warn"),
  APPROVED: S("Disetujui", "info"),
  PROCESSING: S("Diproses", "info"),
  READY_FOR_PICKUP: S("Siap diambil", "info"),
  SHIPPED: S("Dikirim", "info"),
  COMPLETED: S("Selesai", "ok"),
  REJECTED: S("Ditolak", "danger"),
  CANCELLED: S("Dibatalkan", "danger"),
  EXPIRED: S("Kedaluwarsa", "neutral"),
  REFUNDED: S("Dana dikembalikan", "neutral"),
};

export const PAYMENT_STATUS: Record<string, StatusDef> = {
  pending: S("Belum dibayar", "neutral"),
  claimed: S("Pembeli menandai sudah bayar", "warn"),
  confirmed: S("Terkonfirmasi", "ok"),
  rejected: S("Ditolak", "danger"),
  expired: S("Kedaluwarsa", "neutral"),
};

export const PRODUCT_STATUS: Record<string, StatusDef> = {
  draft: S("Draf", "neutral"),
  active: S("Aktif", "ok"),
  archived: S("Arsip", "warn"),
};

export const TICKET_STATUS: Record<string, StatusDef> = {
  OPEN: S("Baru", "brand"),
  ASSIGNED: S("Ditugaskan", "info"),
  IN_PROGRESS: S("Dikerjakan", "info"),
  WAITING_CUSTOMER: S("Menunggu pelanggan", "warn"),
  WAITING_INTERNAL: S("Menunggu internal", "warn"),
  RESOLVED: S("Selesai", "ok"),
  CLOSED: S("Ditutup", "neutral"),
};

export const TICKET_PRIORITY: Record<string, StatusDef> = {
  low: S("Rendah", "neutral"),
  normal: S("Normal", "neutral"),
  high: S("Tinggi", "warn"),
  urgent: S("Mendesak", "danger"),
};

export const APPROVAL_STATUS: Record<string, StatusDef> = {
  pending: S("Menunggu", "warn"),
  approved: S("Disetujui", "ok"),
  rejected: S("Ditolak", "danger"),
  expired: S("Kedaluwarsa", "neutral"),
  cancelled: S("Dibatalkan", "neutral"),
};

export const APPROVAL_TYPE: Record<string, string> = tmap({
  order: "Keputusan pesanan",
  refund: "Pengembalian dana",
  stock_adjustment: "Penyesuaian stok",
  price_change: "Perubahan harga",
  ai_response: "Jawaban AI",
  business_config: "Pengaturan toko",
});

export const CONVERSATION_STATUS: Record<string, StatusDef> = {
  AI: S("Dijawab AI", "brand"),
  WAITING_HUMAN: S("Menunggu staf", "warn"),
  HUMAN: S("Ditangani staf", "info"),
  CLOSED: S("Selesai", "ok"),
};

export const INVOICE_STATUS: Record<string, StatusDef> = {
  awaiting_payment: S("Menunggu pembayaran", "warn"),
  submitted: S("Menunggu verifikasi", "info"),
  paid: S("Lunas", "ok"),
  rejected: S("Ditolak", "danger"),
  cancelled: S("Dibatalkan", "neutral"),
};

export const SUBSCRIPTION_STATUS: Record<string, StatusDef> = {
  trialing: S("Masa uji coba", "info"),
  active: S("Aktif", "ok"),
  expired: S("Berakhir", "danger"),
};

export const MEMBER_STATUS: Record<string, StatusDef> = {
  active: S("Aktif", "ok"),
  invited: S("Diundang", "info"),
  suspended: S("Dinonaktifkan", "warn"),
  removed: S("Dikeluarkan", "neutral"),
};

export const STOCK_MOVEMENT: Record<string, string> = tmap({
  stock_in: "Masuk",
  stock_out: "Keluar",
  adjustment: "Opname",
  sale: "Penjualan",
});

/* ---------------- Format ---------------- */
const nf = () => new Intl.NumberFormat(intlLocale());
/** Pemisah ribuan & desimal bahasa aktif (ID: 15.000,50 · EN: 15,000.50). */
const seps = () => (intlLocale() === "id-ID" ? [".", ","] : [",", "."]);

/** `"15000.00"` / `15000` → `"Rp15.000"` (desimal hanya bila bukan ,00). */
export function rupiah(v: string | number | null | undefined): string {
  if (v === null || v === undefined || v === "") return "—";
  const s = typeof v === "number" ? v.toFixed(2) : v;
  const [i, f = "00"] = s.split(".");
  const neg = (i ?? "").startsWith("-");
  const [th, dec] = seps();
  const int = (i ?? "0").replace("-", "").replace(/\B(?=(\d{3})+(?!\d))/g, th!);
  return `${neg ? "−" : ""}Rp${int}${f === "00" || f === "" ? "" : `${dec}${f.padEnd(2, "0")}`}`;
}

/** Ringkas untuk kartu statistik: Rp2,1 jt / Rp350 rb. */
export function rupiahShort(v: string | number | null | undefined): string {
  const n = Number(v ?? 0);
  if (!Number.isFinite(n)) return "—";
  const loc = intlLocale();
  if (Math.abs(n) >= 1e9) return t("Rp{0} M", (n / 1e9).toLocaleString(loc, { maximumFractionDigits: 1 }));
  if (Math.abs(n) >= 1e6) return t("Rp{0} jt", (n / 1e6).toLocaleString(loc, { maximumFractionDigits: 1 }));
  if (Math.abs(n) >= 1e4) return t("Rp{0} rb", (n / 1e3).toLocaleString(loc, { maximumFractionDigits: 0 }));
  return `Rp${nf().format(n)}`;
}

export const angka = (n: number | null | undefined) => (n === null || n === undefined ? "—" : nf().format(n));
export const persen = (v: number | null | undefined) =>
  v === null || v === undefined ? "—" : `${(v * 100).toLocaleString(intlLocale(), { maximumFractionDigits: 1 })}%`;

function d(iso: string | number | null | undefined): Date | null {
  if (iso === null || iso === undefined || iso === "") return null;
  const x = new Date(iso);
  return Number.isNaN(x.getTime()) ? null : x;
}
/** 13 Nov 2026 */
export const tanggal = (iso: string | number | null | undefined) =>
  d(iso)?.toLocaleDateString(intlLocale(), { day: "numeric", month: "short", year: "numeric" }) ?? "—";
/** 13 Nov 2026, 14.05 */
export const waktu = (iso: string | number | null | undefined) =>
  d(iso)?.toLocaleString(intlLocale(), { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" }) ?? "—";

/** "5 menit lalu", "kemarin", lalu tanggal. */
export function relatif(iso: string | number | null | undefined, now = Date.now()): string {
  const x = d(iso);
  if (!x) return "—";
  const s = Math.round((now - x.getTime()) / 1000);
  if (s < 0) return waktu(iso);
  if (s < 60) return t("baru saja");
  if (s < 3600) return t("{0} menit lalu", Math.floor(s / 60));
  if (s < 86400) return t("{0} jam lalu", Math.floor(s / 3600));
  if (s < 172800) return t("kemarin");
  if (s < 604800) return t("{0} hari lalu", Math.floor(s / 86400));
  return tanggal(iso);
}

/* ---------------- Izin (RBAC) ---------------- */
export const PERMISSION_GROUPS: { label: string; prefix: string[] }[] = [
  { get label() { return t("Toko & tim"); }, prefix: ["tenant", "members"] },
  { get label() { return t("Website"); }, prefix: ["website"] },
  { get label() { return t("Produk & stok"); }, prefix: ["products", "inventory"] },
  { get label() { return t("Pesanan & pembayaran"); }, prefix: ["orders", "payments"] },
  { get label() { return t("Pelanggan & layanan"); }, prefix: ["customers", "conversations", "tickets"] },
  { get label() { return t("Asisten AI"); }, prefix: ["ai_config", "ai_provider"] },
  { get label() { return t("Kontrol"); }, prefix: ["approvals", "analytics", "audit"] },
];
export const PERMISSION_TEXT: Record<string, string> = tmap({
  "tenant.read": "Lihat pengaturan toko", "tenant.update": "Ubah pengaturan toko",
  "members.read": "Lihat anggota tim", "members.invite": "Undang anggota", "members.manage": "Kelola anggota & peran",
  "website.read": "Lihat website", "website.edit": "Ubah draf website", "website.publish": "Terbitkan website",
  "products.read": "Lihat produk", "products.create": "Tambah produk", "products.update": "Ubah produk", "products.archive": "Arsipkan produk",
  "inventory.read": "Lihat stok", "inventory.adjust": "Catat perubahan stok",
  "orders.read": "Lihat pesanan", "orders.create": "Buat pesanan", "orders.approve": "Setujui & proses pesanan",
  "orders.reject": "Tolak pesanan", "orders.cancel": "Batalkan pesanan",
  "payments.verify": "Konfirmasi pembayaran", "payments.refund": "Catat pengembalian dana",
  "customers.read": "Lihat pelanggan", "customers.manage": "Kelola data pelanggan",
  "conversations.read": "Lihat percakapan", "conversations.reply": "Balas percakapan", "conversations.takeover": "Ambil alih / tugaskan percakapan",
  "tickets.read": "Lihat tiket", "tickets.assign": "Tugaskan tiket", "tickets.reply": "Balas tiket", "tickets.close": "Selesaikan tiket",
  "ai_config.read": "Lihat pengaturan AI", "ai_config.manage": "Ubah pengaturan AI", "ai_provider.manage": "Kelola penyedia AI",
  "approvals.read": "Lihat persetujuan", "approvals.decide": "Putuskan persetujuan",
  "analytics.read": "Lihat analitik", "audit.read": "Lihat log aktivitas",
});

/** Label ramah untuk kode aksi audit (`ticket.created` → "Tiket dibuat"). */
const AUDIT_OBJ: Record<string, string> = tmap({
  auth: "Masuk", member: "Anggota", role: "Peran", business: "Profil bisnis", website: "Website", commerce: "Pengaturan toko",
  product: "Produk", inventory: "Stok", order: "Pesanan", payment: "Pembayaran", ticket: "Tiket", approval: "Persetujuan",
  customer: "Pelanggan", invitation: "Undangan", ai: "Asisten AI", conversation: "Percakapan", subscription: "Langganan", security: "Keamanan",
});
const AUDIT_VERB: Record<string, string> = tmap({
  created: "dibuat", updated: "diubah", deleted: "dihapus", changed: "diubah", published: "diterbitkan", archived: "diarsipkan",
  confirmed: "dikonfirmasi", rejected: "ditolak", approved: "disetujui", cancelled: "dibatalkan", assigned: "ditugaskan",
  login: "berhasil", logout: "keluar", failed: "gagal", invited: "diundang", removed: "dikeluarkan", refunded: "dikembalikan",
  closed: "ditutup", resolved: "diselesaikan", status_changed: "status diubah", adjusted: "disesuaikan", accepted: "diterima",
});
export function auditLabel(action: string): string {
  const [obj = "", ...rest] = action.split(".");
  const verb = rest.join("_");
  const o = AUDIT_OBJ[obj];
  const v = AUDIT_VERB[verb];
  return o && v ? `${o} ${v}` : o ? `${o} · ${verb.replace(/_/g, " ")}` : action;
}

/** "Chrome di Windows" dari User-Agent — cukup untuk mengenali perangkat, bukan sidik jari. */
export function deviceName(ua: string | null): string {
  if (!ua) return t("Perangkat tidak dikenal");
  const browser = /Edg\//.test(ua) ? "Edge" : /OPR\/|Opera/.test(ua) ? "Opera" : /Firefox\//.test(ua) ? "Firefox"
    : /SamsungBrowser/.test(ua) ? "Samsung Internet" : /Chrome\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : null;
  const os = /Android/.test(ua) ? "Android" : /iPhone|iPad|iPod/.test(ua) ? "iOS" : /Windows/.test(ua) ? "Windows"
    : /Mac OS X|Macintosh/.test(ua) ? "macOS" : /Linux/.test(ua) ? "Linux" : null;
  if (browser && os) return t("{0} di {1}", browser, os);
  return browser ?? os ?? (ua.length > 40 ? `${ua.slice(0, 40)}…` : ua);
}
