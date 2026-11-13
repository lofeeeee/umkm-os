// Studio (ADR-0017, P9-3/4/5) di browser, stack nyata (Caddy + API + PostgreSQL):
// pemilik baru → Studio → pilih template → ubah judul banner inline → simpan otomatis (schema v2) → terbitkan →
// situs publik merender grid v2, teks tetap teks, <head> per toko (judul + Open Graph) dari API, tanpa error CSP,
// dan tanpa scroll horizontal di ponsel. Env: APP_URL, API_URL, ROOT, E2E_HOST_MAP.
import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";
const ROOT = process.env.ROOT ?? "";

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

test("studio: template → edit inline → simpan otomatis → terbit → situs publik v2", async ({ browser }) => {
  test.skip(!APP || !API || !ROOT, "APP_URL/API_URL/ROOT tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  const slug = `studio${Date.now().toString(36)}`;
  const shop = `http://${slug}.${ROOT}`;

  const owner = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await owner.newPage();
  watch(page, errors);
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email: `${slug}@x.id`, password: "rahasia-studio-123", business_name: `Kopi ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(`${slug}@x.id`);
  await page.getByLabel("Password").fill("rahasia-studio-123");
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  await page.getByRole("link", { name: "Buka Studio" }).first().click();
  await page.waitForURL(/\/studio\?tenant=/);

  // Toko baru: galeri template dulu (bukan kanvas kosong).
  await page.getByRole("button", { name: "Pakai Hangat" }).click();
  const badge = page.locator(".st-save");
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // Edit inline judul banner; HTML diketik sebagai teks biasa.
  const title = `Kopi Senja <script>alert(1)</script> ${Date.now()}`;
  // Klik pertama memilih blok banner; teksnya lalu bisa diketik langsung di kanvas.
  const canvas = page.getByRole("application", { name: /Kanvas halaman/ });
  await canvas.getByLabel(/^Banner.*kolom 1,/).focus();
  const hero = canvas.getByRole("textbox", { name: /^Ubah title/ }).first();
  await hero.click();
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.type(title);
  await hero.press("Enter");
  await expect(badge).toHaveText(/Menyimpan|Draf tersimpan/);
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // Plugin Teks dari katalog (P9-8): pasang → tambah → isi lewat inspector.
  const notice = `Libur Lebaran: toko tutup 1–3 April ${Date.now()}`;
  await page.getByRole("button", { name: "Plugin", exact: true }).click(); // keluar dari pengaturan banner
  await page.getByRole("tab", { name: "Cari plugin" }).click();
  await page.getByRole("searchbox", { name: "Cari plugin" }).fill("teks");
  await page.getByRole("listitem").filter({ hasText: "Paragraf bebas" }).getByRole("button", { name: "Pasang" }).click();
  await page.getByRole("button", { name: "Tambah Teks ke halaman" }).click();
  await page.getByRole("complementary", { name: "Pengaturan plugin" }).getByRole("textbox", { name: /^Isi/ }).fill(notice);
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // P9-10: toko baru belum punya produk/alamat/jadwal/nomor WA → catatan "Sebelum terbit" (tidak memblokir).
  await page.getByRole("button", { name: "Terbitkan", exact: true }).click();
  const checks = page.getByRole("list", { name: "Catatan siap terbit" });
  await expect(checks).toContainText("Galeri produk masih kosong");
  await expect(checks).toContainText("nomor WhatsApp/telepon di Profil bisnis kosong");
  // "Tunjukkan" memilih blok terkait di kanvas; lalu terbitkan tetap.
  await checks.getByRole("listitem").filter({ hasText: "Blok Jam buka kosong" }).getByRole("button", { name: "Tunjukkan" }).click();
  await expect(page.getByRole("complementary", { name: "Pengaturan plugin" })).toContainText("Jam buka");
  await page.getByRole("button", { name: "Terbitkan", exact: true }).click();
  await page.getByRole("button", { name: "Terbitkan sekarang" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Website diterbitkan." })).toBeVisible();

  // Situs publik (konteks baru, tanpa cache): grid v2 + judul baru sebagai TEKS.
  const guest = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const pub = await guest.newPage();
  watch(pub, errors);
  await pub.goto(shop);
  await expect(pub.locator(".sv2")).toBeVisible();
  await expect(pub.getByRole("heading", { level: 1 })).toHaveText(title);
  await expect(pub.getByText(notice)).toBeVisible();
  expect(await pub.locator("script").evaluateAll((s) => s.some((x) => x.textContent?.includes("alert(1)") && !x.getAttribute("type")))).toBe(false);
  // ADR-0018: <head> dari API (crawler tanpa JS melihat nama toko).
  const html = await pub.evaluate(async () => (await fetch(location.href, { cache: "no-store" })).text());
  expect(html).toContain(`<meta property="og:site_name" content="Kopi ${slug}">`);
  expect(html).toContain('<script type="application/ld+json">');
  expect(html).not.toContain("<script>alert(1)");

  // Ponsel: blok bertumpuk, tanpa scroll horizontal.
  await pub.setViewportSize({ width: 390, height: 844 });
  await pub.reload();
  await expect(pub.getByRole("heading", { level: 1 })).toHaveText(title);
  expect(await pub.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  expect(errors).toEqual([]);
});
