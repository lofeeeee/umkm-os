// Alur operasional P4 di browser, stack nyata (Caddy + API + worker + PostgreSQL + Redis):
// tiket (catatan internal vs balasan publik, status), notifikasi realtime lewat SSE tanpa reload,
// role kustom, audit log. Env: APP_URL, API_URL, E2E_HOST_MAP.
import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

test("operasional: tiket, notifikasi realtime, role kustom, audit", async ({ browser }) => {
  test.skip(!APP || !API, "APP_URL/API_URL tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  const slug = `ops${Date.now().toString(36)}`;
  const email = `${slug}@x.id`;
  const password = "rahasia-ops-123";

  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  watch(page, errors);
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email, password, business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  const q = new URL(page.url()).search;

  // Tab A menunggu notifikasi (lonceng tanpa angka)
  await page.goto(`${APP}/orders${q}`);
  const bell = page.getByRole("button", { name: /^Notifikasi/ });
  await expect(bell).toHaveAccessibleName("Notifikasi");

  // Tab B membuat tiket
  const b = await ctx.newPage();
  watch(b, errors);
  await b.goto(`${APP}/tickets${q}`);
  await b.getByLabel("Subjek tiket baru").fill("Pesanan belum sampai");
  await b.getByRole("button", { name: "Buat tiket" }).click();
  await expect(b.getByRole("cell", { name: "Pesanan belum sampai" })).toBeVisible();

  // Tab A: lonceng bertambah TANPA reload (outbox → worker → Redis → bridge → SSE)
  await expect(bell).toHaveAccessibleName("Notifikasi (1)", { timeout: 20_000 });
  await bell.click();
  await expect(page.getByRole("dialog", { name: "Notifikasi" })).toContainText("Ticket baru");

  // Detail tiket: catatan internal + balasan publik, status
  await b.getByRole("button", { name: "Buka" }).click();
  await b.getByLabel("Balasan").fill("Catatan: cek kurir JNE");
  await b.getByLabel("Catatan internal (tidak terlihat pelanggan)").check();
  await b.getByRole("button", { name: "Kirim" }).click();
  await expect(b.getByText("Catatan internal").first()).toBeVisible();
  await b.getByLabel("Catatan internal (tidak terlihat pelanggan)").uncheck();
  await b.getByLabel("Balasan").fill("Sedang kami cek, mohon ditunggu");
  await b.getByRole("button", { name: "Kirim" }).click();
  await expect(b.getByText("Sedang kami cek, mohon ditunggu")).toBeVisible();
  await b.getByRole("button", { name: "Selesai", exact: true }).click();
  await expect(b.getByRole("heading", { name: /TKT-/ })).toBeVisible();
  await expect(b.getByText(/^Selesai · prioritas/)).toBeVisible();

  // Role kustom
  await b.goto(`${APP}/team${q}`);
  await b.getByRole("button", { name: "Peran kustom baru" }).click();
  await b.getByLabel("Nama peran").fill("Kasir");
  await b.getByLabel("orders.read").check();
  await b.getByRole("button", { name: "Simpan peran" }).click();
  await expect(b.getByText("Peran tersimpan.")).toBeVisible();
  await expect(b.getByRole("listitem").filter({ hasText: "Kasir" })).toContainText("1 izin");

  // Audit log memuat aksi-aksi tadi
  await b.goto(`${APP}/audit${q}`);
  await expect(b.getByRole("cell", { name: "ticket.created" })).toBeVisible();
  await expect(b.getByRole("cell", { name: "role.changed" })).toBeVisible();
  await b.setViewportSize({ width: 375, height: 800 });
  await b.goto(`${APP}/tickets${q}`);
  await b.screenshot({ path: "test-results/tickets-mobile.png", fullPage: true });

  expect(errors).toEqual([]);
});
