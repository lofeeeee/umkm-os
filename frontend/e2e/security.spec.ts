// Keamanan akun (AUTH-006 + ubah kata sandi; UAT 15 Nov F7) di browser, stack nyata:
// dua perangkat masuk → keluarkan perangkat lain → ubah kata sandi → kata sandi lama ditolak.
// Env: APP_URL, API_URL, E2E_HOST_MAP.
import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";

async function login(page: Page, email: string, password: string) {
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Masuk" }).click();
}

test("keamanan: sesi aktif, keluarkan perangkat lain, ubah kata sandi", async ({ browser }) => {
  test.skip(!APP || !API, "APP_URL/API_URL tidak diset");
  test.setTimeout(60_000);
  const errors: string[] = [];
  const slug = `sec${Date.now().toString(36)}`;
  const email = `${slug}@x.id`;

  const a = await browser.newContext();
  const pa = await a.newPage();
  pa.on("pageerror", (e) => errors.push(String(e)));
  await pa.goto(`${APP}/login`);
  const reg = await pa.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email, password: "rahasia-lama-123", business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await login(pa, email, "rahasia-lama-123");
  await expect(pa.getByRole("heading", { name: "Beranda" })).toBeVisible();

  // Perangkat kedua.
  const b = await browser.newContext();
  const pb = await b.newPage();
  await login(pb, email, "rahasia-lama-123");
  await expect(pb.getByRole("heading", { name: "Beranda" })).toBeVisible();

  await pa.goto(`${APP}/security`);
  const list = pa.getByRole("list", { name: "Sesi aktif" });
  await expect(list.getByText("Perangkat ini")).toBeVisible();
  await expect(list.getByRole("listitem")).toHaveCount(3); // register + 2 login
  await pa.getByRole("button", { name: "Keluar dari semua perangkat lain" }).click();
  await pa.getByRole("dialog").getByRole("button", { name: "Keluar dari semua" }).click();
  await expect(pa.getByText("2 perangkat dikeluarkan.")).toBeVisible();
  await expect(list.getByRole("listitem")).toHaveCount(1);
  // Perangkat kedua kini terlempar ke login.
  await pb.goto(`${APP}/`);
  await pb.waitForURL(/\/login/);

  // Kata sandi saat ini salah → pesan per bidang, fokus ke pesan.
  await pa.locator("input[name=current_password]").fill("salah-sekali");
  await pa.locator("input[name=new_password]").fill("rahasia-baru-456");
  await pa.getByLabel("Ulangi kata sandi baru").fill("rahasia-baru-456");
  await pa.getByRole("button", { name: "Ubah kata sandi" }).click();
  await expect(pa.getByRole("alert").filter({ hasText: "kata sandi saat ini salah" })).toBeFocused();
  await pa.locator("input[name=current_password]").fill("rahasia-lama-123");
  await pa.getByRole("button", { name: "Ubah kata sandi" }).click();
  await expect(pa.getByText("Kata sandi diubah.")).toBeVisible();

  // Sandi lama ditolak, sandi baru diterima.
  await login(pb, email, "rahasia-lama-123");
  await expect(pb.getByRole("alert")).toBeVisible();
  await login(pb, email, "rahasia-baru-456");
  await expect(pb.getByRole("heading", { name: "Beranda" })).toBeVisible();
  expect(errors).toEqual([]);
});
