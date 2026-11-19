// Klien API dashboard (ADR-0015): cookie sesi HttpOnly (dikirim browser) + header CSRF.
// Token CSRF tidak disimpan di storage persisten; diambil ulang dari GET /auth/session.
import { t as tx, tmap } from "@/i18n";
import { serverMessage } from "@/i18n/server";

export const API_ORIGIN = (process.env.NEXT_PUBLIC_API_ORIGIN ?? "").replace(/\/$/, "");

export interface FieldErr { path: string; code: string }

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public details: unknown,
  ) {
    super(message);
  }
  get fieldErrors(): FieldErr[] {
    const d = this.details as { errors?: FieldErr[] } | null;
    return Array.isArray(d?.errors) ? d.errors : [];
  }
}

let csrf: string | null = null;

export function setCsrf(token: string | null) {
  csrf = token;
}

type Body = { json: unknown } | { bytes: Blob } | undefined;

export async function api<T>(method: string, path: string, body?: Body): Promise<T> {
  const headers: Record<string, string> = {};
  let payload: BodyInit | undefined;
  if (body && "json" in body) {
    headers["content-type"] = "application/json";
    payload = JSON.stringify(body.json);
  } else if (body && "bytes" in body) {
    headers["content-type"] = "application/octet-stream";
    payload = body.bytes;
  }
  if (method !== "GET" && csrf) headers["x-csrf-token"] = csrf;
  const res = await fetch(`${API_ORIGIN}${path}`, { method, headers, body: payload, credentials: "include" });
  if (res.status === 204) return undefined as T;
  const data = await res.json().catch(() => null);
  if (!res.ok || !data?.success) {
    const e = data?.error ?? {};
    throw new ApiError(res.status, e.code ?? "UNKNOWN", e.message ?? `HTTP ${res.status}`, e.details ?? null);
  }
  return data.data as T;
}

export interface Session {
  user_id: string; session_id: string; csrf_token: string;
  /** Hanya dari GET /auth/session (AUTH-011/012). */
  mfa_verified?: boolean; mfa_enrollment_required?: boolean;
  /** Pemegang peran platform; hanya mereka yang perlu memeriksa /platform/me. */
  platform_staff?: boolean;
}
/** Jawaban 202 /auth/login bila akun memakai MFA. */
export interface MfaChallenge { mfa_required: true; mfa_token: string }

/** 403 `details.reason = "mfa_required"`: akun/toko mewajibkan MFA untuk sesi ini. */
export function isMfaRequired(e: unknown): boolean {
  return e instanceof ApiError && e.status === 403
    && (e.details as { reason?: string } | null)?.reason === "mfa_required";
}
export interface Membership { tenant_id: string; name: string; slug: string; role: string }

/**
 * Nama peran dalam bahasa Indonesia. Satu sumber untuk sidebar, home, dan Tim —
 * dulu home punya peta sendiri yang tidak kena karena API mengirim huruf besar.
 */
const ROLE_TEXT: Record<string, string> = tmap({
  owner: "Pemilik",
  admin: "Admin",
  manager: "Manajer",
  catalog_manager: "Pengelola katalog",
  support_agent: "Agen dukungan",
  staff: "Staf",
  viewer: "Pengamat",
});
export function roleLabel(role: string): string {
  return ROLE_TEXT[role.trim().toLowerCase()] ?? role;
}
export interface TenantView {
  id: string; name: string; slug: string; status: string;
  primary_hostname: string | null; my_role: string; my_permissions: string[];
}

/** Sesi aktif atau null (401). */
export async function loadSession(): Promise<Session | null> {
  try {
    const s = await api<Session>("GET", "/api/v1/auth/session");
    setCsrf(s.csrf_token);
    return s;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) return null;
    throw e;
  }
}

/** URL absolut untuk signed path dari API (`/files/...`). */
export function apiUrl(path: string): string {
  return `${API_ORIGIN}${path}`;
}

/**
 * GET mentah dengan sesi yang sama seperti api(): untuk berkas (blob) dan probe akses yang
 * statusnya dibaca langsung (403 = memang tidak berhak, bukan error yang perlu ditampilkan).
 */
export function apiGet(path: string): Promise<Response> {
  return fetch(apiUrl(path), { credentials: "include" });
}

export function tenantParam(): string | null {
  if (typeof window === "undefined") return null;
  const t = new URLSearchParams(window.location.search).get("tenant");
  return t && /^[0-9a-f-]{36}$/.test(t) ? t : null;
}

export const ERROR_TEXT: Record<string, string> = tmap({
  required: "wajib diisi",
  too_long: "terlalu panjang",
  unsafe_url: "tautan tidak aman / tidak valid",
  invalid_asset_ref: "gambar tidak valid",
  asset_not_found: "gambar tidak ditemukan",
  invalid_color: "warna harus #RRGGBB",
  invalid_font_preset: "font tidak dikenal",
  unknown_field: "isian tidak dikenal",
  duplicate_id: "id ganda",
  out_of_range: "di luar rentang",
  invalid_length: "jumlah butir tidak valid",
  invalid_email: "email tidak valid",
  invalid_phone: "nomor tidak valid (hanya angka, awalan 0 atau 62)",
  invalid_timezone: "timezone tidak dikenal",
  invalid_time: "format jam HH:MM",
  open_equals_close: "jam buka sama dengan jam tutup",
  duplicate_day: "hari ganda",
  invalid_date: "format tanggal YYYY-MM-DD",
  closed_xor_hours: "pilih tutup ATAU isi jam",
  invalid_or_duplicate_date: "tanggal tidak valid atau ganda",
  template_mismatch: "templat tidak sesuai website",
  reserved_slug: "alamat URL ini dipakai halaman toko (pilih alamat lain)",
  core_block_required: "blok inti halaman sistem wajib ada tepat satu",
  core_block_not_allowed: "blok inti hanya boleh di halaman sistemnya",
  latitude_longitude_pair: "isi latitude dan longitude bersamaan",
  control_character: "karakter tidak diizinkan",
  // P3: katalog, stok, CRM, pesanan
  invalid_price: "harga tidak valid (angka, maks 2 desimal)",
  promo_not_below_price: "harga promo harus di bawah harga normal",
  invalid_sku: "SKU hanya huruf/angka/titik/strip, maks 64",
  duplicate_sku: "SKU ganda",
  invalid_slug: "alamat URL hanya huruf kecil, angka, dan strip",
  unknown_value: "nilai opsi tidak dikenal",
  count_mismatch: "isi semua opsi varian",
  duplicate_combination: "kombinasi varian ganda",
  options_required_for_multiple_variants: "tambahkan opsi untuk lebih dari satu varian",
  no_active_variant: "minimal satu varian aktif",
  invalid_category: "kategori tidak valid",
  invalid_parent: "induk kategori tidak valid (maks 2 tingkat)",
  invalid_quantity: "jumlah tidak valid",
  invalid_kind: "jenis mutasi tidak valid",
  invalid_tag: "tag hanya huruf/angka/spasi/strip, maks 30",
  email_or_phone_required: "isi email atau telepon",
  required_for_checkout: "wajib diisi sebelum menerima pesanan online",
  required_for_amount_mismatch: "catatan wajib bila jumlah berbeda dari tagihan",
  exceeds_refundable: "melebihi sisa yang dapat dikembalikan",
  invalid_status: "status tidak valid",
  too_many: "terlalu banyak",
  duplicate: "ganda",
  // P4
  invalid_policy: "kebijakan tidak valid",
  not_a_member: "bukan anggota aktif toko",
  unknown_permission: "izin tidak dikenal",
  invalid_reference: "rujukan bukan milik toko ini",
  use_assign_endpoint: "gunakan pilihan penugasan",
  weak_password: "password minimal 8 karakter dan tidak sama dengan email",
  // P5
  invalid_api_key: "kunci API tidak valid (8–512 karakter, tanpa spasi)",
  scheme_not_allowed: "endpoint wajib HTTPS",
  forbidden_address: "alamat jaringan internal/lokal tidak diizinkan",
  forbidden_host: "host lokal/internal tidak diizinkan",
  userinfo_in_url: "URL tidak boleh memuat nama pengguna/kata sandi",
  model_not_offered: "model tidak tersedia di AI platform",
  provider_not_available: "provider tidak ada atau nonaktif",
  same_as_primary: "harus berbeda dari provider utama",
  platform_fields_not_allowed: "AI platform tidak memakai URL/kunci sendiri",
});

export const LIMIT_TEXT: Record<string, string> = tmap({
  max_products: "produk", max_members: "anggota tim", storage_bytes: "penyimpanan file (byte)",
  conversations_monthly: "percakapan per bulan", platform_ai_tokens_monthly: "token AI per bulan",
});

/**
 * Pesan validasi per-field untuk ditampilkan inline di bawah input.
 *
 * Kunci mengikuti `path` dari API (mis. `variants[0].price`). Pesan memakai
 * kalimat Indonesia yang bisa dibaca pemilik toko — bukan path JSON mentah.
 * Path yang tidak punya padanan ramah ditampilkan apa adanya agar tidak hilang.
 */
export function fieldErrors(e: unknown): Map<string, string> {
  const out = new Map<string, string>();
  if (!(e instanceof ApiError)) return out;
  for (const f of e.fieldErrors) {
    const path = f.path || "";
    out.set(path, ERROR_TEXT[f.code] ?? f.code);
  }
  return out;
}

/** Label ramah untuk path error API. */
export function fieldLabel(path: string): string {
  if (path === "name") return tx("Nama produk");
  if (path === "slug") return tx("Alamat URL");
  if (path === "options") return tx("Opsi varian");
  const v = path.match(/^variants\[(\d+)\]\.(\w+)$/);
  if (v) {
    const n = Number(v[1] ?? 0) + 1;
    const part: Record<string, string> = tmap({ sku: "SKU", price: "Harga", promo_price: "Harga promo", option_values: "Nilai opsi", name: "Nama varian" });
    const key = v[2] ?? "";
    return tx("Varian {0} · {1}", n, part[key] ?? key);
  }
  return path;
}

/** Nama bidang umum untuk pesan validasi (UAT 15 Nov: "name: wajib diisi" → "Nama: wajib diisi"). */
export const FIELD_TEXT: Record<string, string> = tmap({
  name: "Nama", email: "Email", phone: "Telepon", whatsapp: "WhatsApp", description: "Deskripsi",
  address: "Alamat", label: "Label", latitude: "Latitude", longitude: "Longitude", timezone: "Zona waktu",
  url: "URL", platform: "Platform", fulfillment_note: "Catatan pengambilan/pengantaran", note: "Catatan",
  date: "Tanggal", open: "Jam buka", close: "Jam tutup", password: "Password", price: "Harga", sku: "SKU",
  slug: "Alamat URL", title: "Judul", delivery_fee: "Ongkir", quantity: "Jumlah", reason: "Alasan",
});

/** `addresses[0].city` → label bidang terakhir (+ nomor urut bila dalam daftar). */
function genericLabel(path: string, labels?: Record<string, string>): string {
  if (!path) return tx("Dokumen");
  if (labels?.[path]) return labels[path]!;
  const m = /^(?:.*?\[(\d+)\]\.)?([^.[\]]+)$/.exec(path);
  const last = m?.[2] ?? path;
  const base = labels?.[last] ?? FIELD_TEXT[last];
  if (!base) return path;
  return m?.[1] !== undefined ? `${base} #${Number(m[1]) + 1}` : base;
}

/** Pesan galat ramah. `labels` (opsional) menimpa nama bidang per halaman, mis. `{ name: "Nama bisnis" }`. */
export function describe(e: unknown, labels?: Record<string, string>): string {
  if (e instanceof ApiError) {
    const f = e.fieldErrors;
    if (f.length) return f.map((x) => `${genericLabel(x.path, labels)}: ${ERROR_TEXT[x.code] ?? x.code}`).join("; ");
    const reason = (e.details as { reason?: string; limit?: string; max?: number } | null)?.reason;
    if (e.status === 403 && reason === "plan_limit") {
      const d = e.details as { limit?: string; max?: number };
      return tx("Batas paket tercapai ({0}: maks {1}). Tingkatkan paket di menu Langganan.", LIMIT_TEXT[d.limit ?? ""] ?? d.limit, d.max);
    }
    if (e.status === 403 && reason === "subscription_expired") return tx("Langganan toko sudah berakhir. Perpanjang di menu Langganan.");
    if (e.status === 403) return tx("Anda tidak punya izin untuk aksi ini.");
    // Pesan server diterjemahkan ke bahasa antarmuka (teks generik ErrorCode diganti kalimat jelas).
    return serverMessage(e.code, e.message);
  }
  return tx("Tidak dapat menghubungi server.");
}
