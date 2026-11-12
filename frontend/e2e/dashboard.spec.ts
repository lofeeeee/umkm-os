// Alur editor di browser: login -> website -> ubah teks hero -> preview ponsel -> simpan -> terbit.
import { expect, test } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const SITE = process.env.SITE_URL ?? "";

test("editor: edit, preview ponsel, publish, tampil di situs publik", async ({ page, context, browser }) => {
  test.skip(!APP || !SITE || !process.env.E2E_EMAIL, "APP_URL/SITE_URL/E2E_EMAIL tidak diset");
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(process.env.E2E_EMAIL!);
  await page.getByLabel("Password").fill(process.env.E2E_PASSWORD!);
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await expect(page.getByRole("heading", { name: "Website" })).toBeVisible();

  const title = `Judul <script>alert(1)</script> ${Date.now()}`;
  await page.getByRole("button", { name: /Ubah Hero/ }).click();
  const field = page.getByLabel("Judul *").first();
  await field.fill(title);
  const preview = page.getByTestId("preview");
  await expect(preview.getByRole("heading", { level: 1 })).toHaveText(title);

  await page.getByRole("button", { name: "Ponsel" }).click();
  await expect.poll(async () => (await preview.boundingBox())?.width).toBe(375);
  await page.screenshot({ path: "test-results/editor-mobile.png", fullPage: true });

  await page.getByRole("button", { name: "Simpan draft" }).click();
  await expect(page.getByRole("status")).toHaveText("Draft tersimpan.");
  // Draft belum terbit: publik masih versi lama
  const pub = await context.newPage();
  await pub.goto(SITE);
  await expect(pub.locator("[data-section-type=hero] h1")).not.toHaveText(title);

  await page.getByRole("button", { name: "Terbitkan" }).click();
  await expect(page.getByRole("status")).toHaveText("Website diterbitkan.");
  // Respons publik di-cache 60 dtk (ADR-0014 #7): pengunjung baru (konteks tanpa cache) melihat versi baru.
  const fresh = await (await browser.newContext()).newPage();
  await fresh.goto(SITE);
  await expect(fresh.locator("[data-section-type=hero] h1")).toHaveText(title);
  // Markup dalam judul tampil sebagai teks dan tidak menghasilkan elemen skrip (AC-WEB-018).
  expect(await fresh.evaluate(() => document.querySelectorAll("section script").length)).toBe(0);
  expect(errors).toEqual([]);
});
