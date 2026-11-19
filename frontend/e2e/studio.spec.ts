// Studio (ADR-0017, P9-3/4/5) di browser, stack nyata (Caddy + API + PostgreSQL):
// pemilik baru → Studio → pilih template → ubah judul banner inline → seret blok + urungkan → plugin Teks →
// coretan kuas → simpan otomatis (schema v2) → terbitkan →
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
  // Kanvas = renderer situs asli: klik blok Banner untuk memilih, lalu ketik langsung di judulnya.
  const canvas = page.getByRole("main", { name: "Kanvas halaman" });
  await canvas.locator('[data-block-label="Banner"]').click({ position: { x: 8, y: 8 } });
  const hero = canvas.getByRole("textbox", { name: /^Ubah title/ }).first();
  await hero.click();
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.type(title);
  await hero.press("Enter");
  await expect(badge).toHaveText(/Menyimpan|Draf tersimpan/);
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // Seret blok: Banner dipindah ke bawah blok Promo (susunan berubah, tersimpan otomatis).
  const order = () => canvas.locator(".sv2-grid [data-block-label]").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.blockLabel));
  const before = await order();
  const heroBox = (await canvas.locator('[data-block-label="Banner"]').boundingBox())!;
  const promoBox = (await canvas.locator('[data-block-label="Promo"]').boundingBox())!;
  await page.mouse.move(heroBox.x + 12, heroBox.y + heroBox.height - 12);
  await page.mouse.down();
  await page.mouse.move(heroBox.x + 40, promoBox.y + promoBox.height - 4, { steps: 12 });
  await page.mouse.up();
  await expect.poll(order).not.toEqual(before);
  await page.keyboard.press("ControlOrMeta+Z");
  await expect.poll(order).toEqual(before);

  // Plugin Teks dari katalog (P9-8): cari → tambah → isi lewat inspector.
  const notice = `Libur Lebaran: toko tutup 1–3 April ${Date.now()}`;
  await page.getByRole("navigation", { name: "Bilah alat Studio" }).getByRole("button", { name: "Blok" }).click(); // keluar dari pengaturan banner, buka katalog
  await page.getByRole("searchbox", { name: "Cari blok" }).fill("teks");
  await page.getByRole("button", { name: "Tambah Teks" }).click();
  await page.getByRole("complementary", { name: "Pengaturan blok" }).getByRole("textbox", { name: /^Isi/ }).fill(notice);
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // Kuas: satu coretan di latar halaman ikut tersimpan dan terbit.
  await page.getByRole("button", { name: "Coretan" }).click();
  await page.getByRole("button", { name: "Latar halaman" }).click();
  await page.getByRole("button", { name: "Kuas" }).click();
  const grid = (await canvas.locator(".sv2-grid").boundingBox())!;
  await page.mouse.move(grid.x + 40, grid.y + 20);
  await page.mouse.down();
  await page.mouse.move(grid.x + 240, grid.y + 60, { steps: 10 });
  await page.mouse.up();
  await page.keyboard.press("Escape");
  await expect(badge).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  // P9-10: toko baru belum punya produk/alamat/jadwal/nomor WA → catatan "Sebelum terbit" (tidak memblokir).
  await page.getByRole("button", { name: "Terbitkan", exact: true }).click();
  const checks = page.getByRole("list", { name: "Catatan siap terbit" });
  await expect(checks).toContainText("Galeri produk masih kosong");
  await expect(checks).toContainText("nomor WhatsApp/telepon di Profil bisnis kosong");
  // "Tunjukkan" memilih blok terkait di kanvas; lalu terbitkan tetap.
  await checks.getByRole("listitem").filter({ hasText: "Blok Jam buka kosong" }).getByRole("button", { name: "Tunjukkan" }).click();
  await expect(page.getByRole("complementary", { name: "Pengaturan blok" })).toContainText("Jam buka");
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
  await expect(pub.locator(".sv2-ink path")).toHaveCount(1);
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

// Urutan menu halaman: seret di panel Halaman (mouse) atau ↑/↓ pada pegangan (keyboard) → tersimpan di server
// dan dipakai navigasi. Beranda selalu pertama.
test("studio: urutan menu halaman bisa diseret dan tersimpan", async ({ browser }) => {
  test.skip(!APP || !API || !ROOT, "APP_URL/API_URL/ROOT tidak diset");
  test.setTimeout(90_000);
  const slug = `urut${Date.now().toString(36)}`;
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email: `${slug}@x.id`, password: "rahasia-studio-123", business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(`${slug}@x.id`);
  await page.getByLabel("Password").fill("rahasia-studio-123");
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  await page.getByRole("link", { name: "Buka Studio" }).first().click();
  await page.waitForURL(/\/studio\?tenant=/);
  await page.getByRole("button", { name: "Pakai Hangat" }).click();
  await expect(page.locator(".st-save")).toHaveText(/Draf tersimpan/, { timeout: 15_000 });

  const rail = page.getByRole("navigation", { name: "Bilah alat Studio" });
  const list = page.getByRole("list", { name: "Daftar halaman" });
  const order = () => list.locator("[data-page-slug]").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.pageSlug));
  // Templat Hangat membuat beberapa halaman (Menu, Tentang kami, Blog, artikel).
  await rail.getByRole("button", { name: "Halaman" }).click();
  const start = await order();
  expect(start[0]).toBe("home");
  expect(start).toEqual(expect.arrayContaining(["menu", "blog"]));
  const pos = async (s: string) => (await order()).indexOf(s);

  // Mouse: seret "Blog" ke atas "Menu".
  await list.locator('[data-page-slug="blog"]').dragTo(list.locator('[data-page-slug="menu"]'), { targetPosition: { x: 40, y: 4 } });
  await expect(page.getByRole("status").filter({ hasText: "Urutan menu disimpan." })).toBeVisible();
  await expect.poll(async () => (await pos("blog")) < (await pos("menu"))).toBe(true);
  const dragged = await order();
  expect(dragged[0]).toBe("home");
  // Navigasi di kanvas ikut urutan baru (Blog sebelum Menu).
  const nav = page.getByRole("main", { name: "Kanvas halaman" }).getByRole("navigation", { name: "Halaman" });
  const labels = await nav.getByRole("link").allTextContents();
  expect(labels.indexOf("Blog")).toBeLessThan(labels.indexOf("Menu"));

  // Tersimpan di server (muat ulang), lalu keyboard: ↓ pada pegangan "Blog" mengembalikannya ke bawah Menu.
  await page.reload();
  await rail.getByRole("button", { name: "Halaman" }).click();
  await expect.poll(order).toEqual(dragged);
  await page.getByRole("button", { name: /^Ubah urutan Blog/ }).press("ArrowDown");
  await expect.poll(async () => (await pos("blog")) > (await pos("menu"))).toBe(true);
  await expect(page.getByRole("status").filter({ hasText: "Dipindah ke posisi" })).toHaveCount(1);
  await ctx.close();
});
