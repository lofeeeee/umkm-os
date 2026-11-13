// Alur toko P3 di browser, stack nyata (Caddy + API + PostgreSQL):
// pemilik daftar → produk + stok + QRIS → terbit; pembeli guest → keranjang → checkout →
// halaman bayar QRIS → "sudah bayar"; pemilik konfirmasi; pembeli melihat "Diproses".
// Env: APP_URL (http://app.umkm.test), ROOT (umkm.test), API_URL (http://api.umkm.test), E2E_HOST_MAP.
import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const ROOT = process.env.ROOT ?? "";
const API = process.env.API_URL ?? "";

// PNG 1×1 valid (lolos pemeriksaan isi server).
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

async function noHorizontalScroll(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
}

test("toko: checkout guest + pembayaran QRIS statis + konfirmasi pemilik", async ({ browser }) => {
  test.skip(!APP || !ROOT || !API, "APP_URL/ROOT/API_URL tidak diset");
  test.setTimeout(120_000);
  const errors: string[] = [];
  const slug = `e2e${Date.now().toString(36)}`;
  const email = `${slug}@x.id`;
  const password = "rahasia-e2e-123";
  const SITE = `http://${slug}.${ROOT}`;

  // ---------- pemilik
  const owner = await (await browser.newContext()).newPage();
  watch(owner, errors);
  await owner.goto(`${APP}/login`);
  const reg = await owner.evaluate(async ([api, b]) => {
    const r = await fetch(`${api}/api/v1/auth/register`, {
      method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
    });
    return r.status;
  }, [API, { email, password, business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await owner.goto(`${APP}/login`);
  await owner.getByLabel("Email").fill(email);
  await owner.getByLabel("Password").fill(password);
  await owner.getByRole("button", { name: "Masuk" }).click();
  await owner.getByRole("link", { name: "Kelola website" }).click();
  await owner.waitForURL(/\/website\?tenant=/);
  const q = new URL(owner.url()).search;

  // template + terbit (situs publik dengan grid produk)
  const choose = owner.getByRole("button", { name: "Pakai template ini" }).first();
  await expect(owner.getByRole("button", { name: /Pakai template ini|Terbitkan/ }).first()).toBeVisible();
  if (await choose.isVisible()) await choose.click();
  await owner.getByRole("button", { name: "Terbitkan" }).click();
  await expect(owner.getByRole("status").filter({ hasText: "Website diterbitkan." })).toBeVisible();

  // ambil di tempat
  await owner.goto(`${APP}/business${q}`);
  await owner.getByLabel("Bisa ambil di tempat").check();
  await owner.getByRole("button", { name: "Simpan profil" }).click();
  await expect(owner.getByText("Tersimpan.").first()).toBeVisible();

  // produk
  await owner.goto(`${APP}/products${q}`);
  await owner.getByRole("button", { name: "Tambah produk" }).click();
  await owner.getByLabel("Nama").fill("Kopi Susu E2E");
  await owner.getByLabel("SKU").fill("E2E-1");
  await owner.getByLabel("Harga", { exact: true }).fill("18000");
  await owner.getByLabel("Harga promo").fill("15000");
  await owner.getByRole("button", { name: "Simpan" }).click();
  const row = owner.getByRole("row", { name: /Kopi Susu E2E/ });
  await expect(row.getByRole("cell", { name: "Draf", exact: true })).toBeVisible();
  await Promise.all([
    owner.waitForResponse((r) => r.url().endsWith("/status") && r.status() === 200),
    row.getByRole("button", { name: "Aktifkan" }).click(),
  ]);
  await expect(row.getByRole("cell", { name: "Aktif", exact: true })).toBeVisible();

  // stok
  await owner.goto(`${APP}/inventory${q}`);
  await owner.getByRole("button", { name: "Kelola" }).click();
  await owner.getByLabel("Jumlah").fill("5");
  await owner.getByLabel("Alasan").fill("Stok awal");
  await owner.getByRole("button", { name: "Catat" }).click();
  await expect(owner.getByText("Mutasi tercatat.")).toBeVisible();

  // QRIS + aktifkan checkout
  await owner.goto(`${APP}/shop-settings${q}`);
  await owner.getByRole("button", { name: "Pilih gambar" }).click();
  await owner.locator("input[type=file]").setInputFiles({ name: "qris.png", mimeType: "image/png", buffer: PNG });
  await expect(owner.locator(".d-field img").first()).toBeVisible();
  await owner.getByLabel("Nama penerima (tampil di halaman bayar)").fill("Toko E2E");
  await owner.getByLabel("Terima pesanan online").check();
  await owner.getByRole("button", { name: "Simpan" }).click();
  await expect(owner.getByText("Tersimpan.")).toBeVisible();

  // ---------- pembeli (konteks baru, ponsel)
  const buyerCtx = await browser.newContext({ viewport: { width: 375, height: 800 } });
  const buyer = await buyerCtx.newPage();
  watch(buyer, errors);
  await buyer.goto(SITE);
  const card = buyer.locator(".s-product", { hasText: "Kopi Susu E2E" });
  await expect(card).toContainText("Rp15.000");
  await noHorizontalScroll(buyer);
  await card.click();
  await expect(buyer.getByRole("heading", { name: "Kopi Susu E2E" })).toBeVisible();
  await buyer.getByLabel("Jumlah").fill("2");
  await buyer.getByRole("button", { name: "Tambah ke keranjang" }).click();
  await buyer.getByRole("link", { name: "Lihat keranjang" }).click();
  await expect(buyer.getByText("Perkiraan total:")).toContainText("Rp30.000");
  await buyer.getByLabel("Nama").fill("Sari <b>Pembeli</b>");
  await buyer.getByLabel("Email").fill(`buyer-${slug}@x.id`);
  await buyer.getByRole("checkbox", { name: /menyetujui/ }).check();
  await noHorizontalScroll(buyer);
  await buyer.getByRole("button", { name: "Buat pesanan" }).click();

  await expect(buyer.getByRole("heading", { name: /Pesanan ORD-/ })).toBeVisible();
  await expect(buyer.getByText("Menunggu pembayaran")).toBeVisible();
  await expect(buyer.getByText(/bayar tepat Rp30\.000/i)).toBeVisible();
  const qris = buyer.getByRole("img", { name: /QRIS/ });
  await expect(qris).toBeVisible();
  expect(await qris.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(0);
  await noHorizontalScroll(buyer);
  await buyer.screenshot({ path: "test-results/shop-pay-mobile.png", fullPage: true });
  expect(new URL(buyer.url()).search).toBe("");
  await buyer.getByRole("button", { name: "Saya sudah membayar" }).click();
  await expect(buyer.getByText("Toko akan memverifikasi")).toBeVisible();

  // ---------- pemilik konfirmasi
  await owner.goto(`${APP}/orders${q}`);
  await owner.getByRole("button", { name: "Buka" }).first().click();
  await expect(owner.getByText("Pembeli menandai sudah bayar")).toBeVisible();
  await expect(owner.getByText("Sari <b>Pembeli</b>", { exact: false })).toBeVisible();
  await owner.getByRole("button", { name: "Konfirmasi diterima" }).click();
  await expect(owner.getByText("Pembayaran dikonfirmasi.")).toBeVisible();
  await expect(owner.getByRole("heading", { name: /Diproses/ })).toBeVisible();

  // ---------- pembeli melihat status baru; stok terpotong
  await buyer.getByRole("button", { name: "Perbarui status" }).click();
  await expect(buyer.locator(".shop-status")).toHaveText("Diproses");
  await owner.goto(`${APP}/inventory${q}`);
  await expect(owner.getByRole("row", { name: /Kopi Susu E2E/ }).getByRole("cell").nth(1)).toHaveText("3");

  expect(errors).toEqual([]);
});
