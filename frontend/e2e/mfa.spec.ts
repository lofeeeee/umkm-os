// MFA TOTP (AUTH-011/012) di browser, stack nyata: siapkan → aktifkan → kode pemulihan → login dua langkah →
// kebijakan toko. Kode TOTP dihitung di sini dari kunci yang ditampilkan (RFC 6238, HMAC-SHA1).
// Env: APP_URL, API_URL, E2E_HOST_MAP.
import { createHmac } from "node:crypto";

import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";

function base32(s: string): Buffer {
  const A = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = 0, buf = 0;
  const out: number[] = [];
  for (const c of s.replace(/\s/g, "").toUpperCase()) {
    buf = (buf << 5) | A.indexOf(c);
    bits += 5;
    if (bits >= 8) { bits -= 8; out.push((buf >> bits) & 0xff); }
  }
  return Buffer.from(out);
}

function totp(secret: string, stepOffset = 0): string {
  const c = Buffer.alloc(8);
  c.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 1000 / 30) + stepOffset));
  const h = createHmac("sha1", base32(secret)).update(c).digest();
  const o = h.readUInt8(h.length - 1) & 0xf;
  return String((h.readUInt32BE(o) & 0x7fffffff) % 1_000_000).padStart(6, "0");
}

/** Hindari batas langkah 30 detik di tengah satu aksi. */
async function freshStep(page: Page) {
  const left = 30 - (Math.floor(Date.now() / 1000) % 30);
  if (left < 4) await page.waitForTimeout(left * 1000 + 300);
}

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

test("MFA: aktifkan, login dua langkah, kode pemulihan, kebijakan toko", async ({ browser }) => {
  test.skip(!APP || !API, "APP_URL/API_URL tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  const slug = `mfa${Date.now().toString(36)}`;
  const email = `${slug}@x.id`;
  const password = "rahasia-mfa-123";

  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  watch(page, errors);
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email, password, business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);

  async function login() {
    await page.goto(`${APP}/login`);
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Masuk" }).click();
  }
  await login();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  const q = new URL(page.url()).search;

  // Siapkan + aktifkan dari halaman Keamanan (dibuka dengan ?tenant= agar kebijakan toko tampil).
  await page.getByRole("link", { name: "Keamanan" }).click();
  await page.waitForURL(/\/security\?tenant=/);
  await expect(page.getByText("Belum aktif")).toBeVisible();
  // Mewajibkan MFA sebelum sesi lolos MFA ditolak (Owner tidak mengunci dirinya sendiri).
  await page.getByLabel(/Wajibkan MFA untuk Pemilik/).click();
  await expect(page.getByRole("alert").filter({ hasText: /Aktifkan MFA di akun Anda/ })).toBeVisible();
  await expect(page.getByLabel(/Wajibkan MFA untuk Pemilik/)).not.toBeChecked();

  await page.getByRole("button", { name: "Siapkan MFA" }).click();
  const secret = (await page.getByTestId("mfa-secret").textContent()) ?? "";
  expect(secret.replace(/\s/g, "")).toMatch(/^[A-Z2-7]{32}$/);
  await page.getByLabel("Kode dari aplikasi").fill("000000" === totp(secret) ? "111111" : "000000");
  await page.getByRole("button", { name: "Aktifkan" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Kode tidak valid" })).toBeVisible();
  await freshStep(page);
  await page.getByLabel("Kode dari aplikasi").fill(totp(secret));
  await page.getByRole("button", { name: "Aktifkan" }).click();
  const list = page.getByRole("list", { name: "Kode pemulihan" });
  await expect(list.getByRole("listitem")).toHaveCount(10);
  const recovery = ((await list.getByRole("listitem").first().textContent()) ?? "").trim();
  expect(recovery).toMatch(/^[a-z2-7]{5}-[a-z2-7]{5}$/);
  await expect(page.getByText(/Aktif · sisa kode pemulihan: 10/)).toBeVisible();

  // Sesi kini lolos MFA → kebijakan toko boleh diaktifkan.
  await page.getByLabel(/Wajibkan MFA untuk Pemilik/).click();
  await expect(page.getByText("Kebijakan toko tersimpan.")).toBeVisible();
  await expect(page.getByLabel(/Wajibkan MFA untuk Pemilik/)).toBeChecked();

  // Login dua langkah dengan TOTP (langkah berikutnya: kode langkah ini sudah dipakai saat aktivasi).
  await page.getByRole("button", { name: "Keluar", exact: true }).click();
  await page.waitForURL(/\/login$/);
  await login();
  await expect(page.getByRole("heading", { name: "Verifikasi dua langkah" })).toBeVisible();
  await page.getByLabel("Kode verifikasi").fill(totp(secret, 1));
  await page.getByRole("button", { name: "Verifikasi" }).click();
  await expect(page.getByRole("heading", { name: "Beranda" })).toBeVisible();
  await page.goto(`${APP}/orders${q}`);
  await expect(page.getByRole("heading", { name: "Pesanan" })).toBeVisible();

  // Login dengan kode pemulihan (sekali pakai).
  await page.getByRole("button", { name: "Keluar", exact: true }).click();
  await page.waitForURL(/\/login$/);
  await login();
  await page.getByLabel("Kode verifikasi").fill(recovery.toUpperCase());
  await page.getByRole("button", { name: "Verifikasi" }).click();
  await expect(page.getByRole("heading", { name: "Beranda" })).toBeVisible();
  await page.goto(`${APP}/security${q}`);
  await expect(page.getByText(/sisa kode pemulihan: 9/)).toBeVisible();

  // Kode pemulihan yang sama tidak bisa dipakai lagi.
  await page.getByRole("button", { name: "Keluar", exact: true }).click();
  await page.waitForURL(/\/login$/);
  await login();
  await page.getByLabel("Kode verifikasi").fill(recovery);
  await page.getByRole("button", { name: "Verifikasi" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Kode tidak valid" })).toBeVisible();

  expect(errors).toEqual([]);
  await ctx.close();
});
