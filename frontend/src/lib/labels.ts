// Kamus label & format tunggal untuk dasbor (pedoman: docs/ux/pedoman-label.md).
// Aturan singkat:
//  - Bahasa Indonesia, huruf kapital hanya di awal kalimat ("Tambah produk", bukan "Tambah Produk").
//  - Tombol = kata kerja + objek bila perlu: Simpan, Batal, Tambah produk, Hapus, Arsipkan.
//  - Status = kata sifat/keadaan pendek ("Menunggu pembayaran"), warnanya dari tone di sini.
//  - Angka uang selalu `rupiah()`, tanggal selalu `tanggal()`/`waktu()`.

import type { Tone } from "@/ui/page";

export interface StatusDef { label: string; tone: Tone }
const S = (label: string, tone: Tone): StatusDef => ({ label, tone });

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

export const APPROVAL_TYPE: Record<string, string> = {
  order: "Keputusan pesanan",
  refund: "Pengembalian dana",
  stock_adjustment: "Penyesuaian stok",
  price_change: "Perubahan harga",
  ai_response: "Jawaban AI",
  business_config: "Pengaturan toko",
};

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

export const STOCK_MOVEMENT: Record<string, string> = {
  stock_in: "Masuk",
  stock_out: "Keluar",
  adjustment: "Opname",
  sale: "Penjualan",
};

/* ---------------- Format ---------------- */
const nf = new Intl.NumberFormat("id-ID");

/** `"15000.00"` / `15000` → `"Rp15.000"` (desimal hanya bila bukan ,00). */
export function rupiah(v: string | number | null | undefined): string {
  if (v === null || v === undefined || v === "") return "—";
  const s = typeof v === "number" ? v.toFixed(2) : v;
  const [i, f = "00"] = s.split(".");
  const neg = (i ?? "").startsWith("-");
  const int = (i ?? "0").replace("-", "").replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${neg ? "−" : ""}Rp${int}${f === "00" || f === "" ? "" : `,${f.padEnd(2, "0")}`}`;
}

/** Ringkas untuk kartu statistik: Rp2,1 jt / Rp350 rb. */
export function rupiahShort(v: string | number | null | undefined): string {
  const n = Number(v ?? 0);
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 1e9) return `Rp${(n / 1e9).toLocaleString("id-ID", { maximumFractionDigits: 1 })} M`;
  if (Math.abs(n) >= 1e6) return `Rp${(n / 1e6).toLocaleString("id-ID", { maximumFractionDigits: 1 })} jt`;
  if (Math.abs(n) >= 1e4) return `Rp${(n / 1e3).toLocaleString("id-ID", { maximumFractionDigits: 0 })} rb`;
  return `Rp${nf.format(n)}`;
}

export const angka = (n: number | null | undefined) => (n === null || n === undefined ? "—" : nf.format(n));
export const persen = (v: number | null | undefined) =>
  v === null || v === undefined ? "—" : `${(v * 100).toLocaleString("id-ID", { maximumFractionDigits: 1 })}%`;

function d(iso: string | number | null | undefined): Date | null {
  if (iso === null || iso === undefined || iso === "") return null;
  const x = new Date(iso);
  return Number.isNaN(x.getTime()) ? null : x;
}
/** 13 Nov 2026 */
export const tanggal = (iso: string | number | null | undefined) =>
  d(iso)?.toLocaleDateString("id-ID", { day: "numeric", month: "short", year: "numeric" }) ?? "—";
/** 13 Nov 2026, 14.05 */
export const waktu = (iso: string | number | null | undefined) =>
  d(iso)?.toLocaleString("id-ID", { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" }) ?? "—";

/** "5 menit lalu", "kemarin", lalu tanggal. */
export function relatif(iso: string | number | null | undefined, now = Date.now()): string {
  const x = d(iso);
  if (!x) return "—";
  const s = Math.round((now - x.getTime()) / 1000);
  if (s < 0) return waktu(iso);
  if (s < 60) return "baru saja";
  if (s < 3600) return `${Math.floor(s / 60)} menit lalu`;
  if (s < 86400) return `${Math.floor(s / 3600)} jam lalu`;
  if (s < 172800) return "kemarin";
  if (s < 604800) return `${Math.floor(s / 86400)} hari lalu`;
  return tanggal(iso);
}
