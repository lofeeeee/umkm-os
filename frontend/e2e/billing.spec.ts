// Langganan + pembayaran manual (P7-3, ADR-0016) di browser, stack nyata. Pemilik toko memilih Pro & mengunggah
// bukti; super admin (akun disiapkan runner: E2E_ADMIN_EMAIL/PASSWORD, platform_role super_admin, BELUM MFA) wajib
// mendaftarkan MFA dulu (AUTH-011), lalu menyetujui di Admin platform; toko melihat Pro aktif.
// Env: APP_URL, API_URL, E2E_HOST_MAP, E2E_ADMIN_EMAIL, E2E_ADMIN_PASSWORD.
import { createHmac } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";
const ADMIN = process.env.E2E_ADMIN_EMAIL ?? "";
const ADMIN_PW = process.env.E2E_ADMIN_PASSWORD ?? "";

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

function totp(secret: string): string {
  const c = Buffer.alloc(8);
  c.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 1000 / 30)));
  const h = createHmac("sha1", base32(secret)).update(c).digest();
  const o = h.readUInt8(h.length - 1) & 0xf;
  return String((h.readUInt32BE(o) & 0x7fffffff) % 1_000_000).padStart(6, "0");
}

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

async function login(page: Page, email: string, password: string) {
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Masuk" }).click();
}

test("langganan: pilih Pro, unggah bukti, super admin (MFA) menyetujui", async ({ browser }) => {
  test.skip(!APP || !API || !ADMIN || !ADMIN_PW, "APP_URL/API_URL/E2E_ADMIN_* tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  const slug = `bill${Date.now().toString(36)}`;

  // Pemilik toko baru → uji coba Starter.
  const owner = await browser.newContext();
  const page = await owner.newPage();
  watch(page, errors);
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email: `${slug}@x.id`, password: "rahasia-bill-123", business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await login(page, `${slug}@x.id`, "rahasia-bill-123");
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  await page.getByRole("link", { name: "Langganan" }).click();
  const current = page.getByRole("region", { name: "Paket saat ini" });
  await expect(current).toContainText("Paket Starter — Uji coba");
  await expect(current).toContainText("Token AI platform bulan ini");

  // Pilih Pro → tagihan + instruksi → unggah bukti.
  await page.getByRole("button", { name: "Pilih Pro" }).click();
  const bill = page.getByRole("region", { name: "Tagihan berjalan" });
  await expect(bill).toContainText("Rp100.000");
  await expect(bill).toContainText("Menunggu pembayaran");
  await expect(bill.getByRole("note")).toContainText("Cara bayar");
  const png = readFileSync(join(__dirname, "fixtures", "proof.png"));
  await bill.getByLabel(/Unggah bukti bayar/).setInputFiles({ name: "bukti.png", mimeType: "image/png", buffer: png });
  await expect(page.getByText("Bukti terkirim. Menunggu verifikasi admin platform.")).toBeVisible();
  await expect(bill).toContainText("Menunggu verifikasi");
  const code = ((await bill.getByRole("heading").textContent()) ?? "").replace("Tagihan ", "").trim();

  // Super admin: tanpa MFA diarahkan ke pendaftaran MFA (AUTH-011), lalu menyetujui.
  const admin = await browser.newContext();
  const ap = await admin.newPage();
  watch(ap, errors);
  await login(ap, ADMIN, ADMIN_PW);
  await ap.waitForURL(/\/security\?reason=mfa/);
  await ap.getByRole("button", { name: "Siapkan MFA" }).click();
  const secret = (await ap.getByTestId("mfa-secret").textContent()) ?? "";
  await ap.getByLabel("Kode dari aplikasi").fill(totp(secret));
  await ap.getByRole("button", { name: "Aktifkan" }).click();
  await expect(ap.getByRole("list", { name: "Kode pemulihan" })).toBeVisible();
  await ap.goto(`${APP}/platform`);
  const queue = ap.getByRole("list", { name: "Tagihan langganan" });
  const item = queue.getByRole("listitem").filter({ hasText: code });
  await expect(item).toContainText(`Toko ${slug}`);
  await expect(item.getByRole("img", { name: `Bukti bayar ${code}` })).toBeVisible();
  await item.getByRole("button", { name: "Setujui" }).click();
  await expect(ap.getByText("Pembayaran disetujui; langganan toko aktif.")).toBeVisible();

  // Toko: Pro aktif, tagihan lunas.
  await page.reload();
  await expect(current).toContainText("Paket Pro — Aktif");
  await expect(page.getByRole("region", { name: "Riwayat tagihan" })).toContainText("Lunas");
  await page.setViewportSize({ width: 360, height: 740 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: "test-results/subscription-mobile.png", fullPage: true });

  expect(errors).toEqual([]);
  await owner.close();
  await admin.close();
});
