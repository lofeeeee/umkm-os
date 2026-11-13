// Web chat situs toko (P6-2, ADR-0006) di browser, stack nyata + provider AI tiruan (e2e/mock-llm.py)
// sebagai provider platform. Pemberitahuan AI → jawaban AI → tab kedua menerima lewat SSE →
// "Bicara dengan admin" → akhiri. Env: APP_URL, API_URL, ROOT (mis. umkm.test), E2E_AI=1, E2E_HOST_MAP.
import { expect, test, type Page } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";
const ROOT = process.env.ROOT ?? "";

function watch(page: Page, errors: string[]) {
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
}

test("web chat: persetujuan AI, jawaban AI, realtime, handover, akhiri", async ({ browser }) => {
  test.skip(!APP || !API || !ROOT || !process.env.E2E_AI, "APP_URL/API_URL/ROOT/E2E_AI tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  const slug = `chat${Date.now().toString(36)}`;
  const shop = `http://${slug}.${ROOT}`;

  // Pemilik: daftar → provider platform → aktifkan chatbot (Hybrid, tool jam buka aktif bawaan).
  const owner = await browser.newContext();
  const page = await owner.newPage();
  watch(page, errors);
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email: `${slug}@x.id`, password: "rahasia-chat-123", business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(`${slug}@x.id`);
  await page.getByLabel("Password").fill("rahasia-chat-123");
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  await page.getByRole("link", { name: "Asisten AI" }).click();
  await page.getByLabel("Nama").fill("AI Platform");
  await page.getByRole("button", { name: "Tambah provider" }).click();
  await expect(page.getByText("Provider ditambahkan.")).toBeVisible();
  await page.getByLabel("Provider utama").selectOption({ label: "AI Platform" });
  await page.getByLabel("Aktifkan chatbot untuk pelanggan").check();
  await page.getByRole("button", { name: "Simpan pengaturan" }).click();
  await expect(page.getByText("Pengaturan tersimpan.")).toBeVisible();

  // Pelanggan (konteks terpisah, tanpa login) di situs toko.
  const guest = await browser.newContext();
  const a = await guest.newPage();
  watch(a, errors);
  await a.goto(`${shop}/keranjang`);
  await a.getByRole("button", { name: "Chat" }).click();
  const panel = a.getByRole("dialog", { name: "Chat dengan toko" });
  // D-12: pemberitahuan tampil sebelum pesan pertama; belum ada kotak pesan.
  await expect(panel).toContainText("asisten AI");
  await expect(panel).toContainText("bicara dengan admin");
  await expect(panel.getByLabel("Tulis pesan")).toHaveCount(0);
  await panel.getByRole("button", { name: "Setuju & mulai chat" }).click();
  await panel.getByLabel("Tulis pesan").fill("Jam berapa toko buka?");
  await panel.getByRole("button", { name: "Kirim" }).click();
  const log = panel.getByRole("list", { name: "Pesan" });
  await expect(log).toContainText("Jam berapa toko buka?");
  await expect(log).toContainText("Halo dari asisten uji. Data jam buka diterima.");
  await expect(log).toContainText("Asisten AI");

  // Tab kedua (cookie sesi chat sama) memulihkan percakapan dan menerima pesan baru lewat SSE.
  const b = await guest.newPage();
  watch(b, errors);
  await b.goto(`${shop}/keranjang`);
  await b.getByRole("button", { name: "Chat" }).click();
  const logB = b.getByRole("dialog", { name: "Chat dengan toko" }).getByRole("list", { name: "Pesan" });
  await expect(logB).toContainText("Halo dari asisten uji. Data jam buka diterima.");
  await b.waitForTimeout(500); // EventSource tersambung
  // Isi pesan dirender sebagai teks, bukan HTML.
  await panel.getByLabel("Tulis pesan").fill("<b>tebal</b> ada promo?");
  await panel.getByRole("button", { name: "Kirim" }).click();
  await expect(logB).toContainText("<b>tebal</b> ada promo?");
  await expect(logB.locator("b")).toHaveCount(0);

  // Minta staf → AI berhenti, status menunggu staf (juga di tab kedua lewat SSE).
  await panel.getByRole("button", { name: "Bicara dengan admin" }).click();
  await expect(panel.getByRole("status")).toContainText("Menunggu staf toko");
  await expect(log).toContainText("Pesan Anda diteruskan ke staf toko");
  await expect(b.getByRole("status")).toContainText("Menunggu staf toko");
  await panel.getByLabel("Tulis pesan").fill("halo?");
  await panel.getByRole("button", { name: "Kirim" }).click();
  await expect(log.getByRole("listitem").last()).toContainText("halo?");
  await expect(panel.getByRole("button", { name: "Bicara dengan admin" })).toHaveCount(0);

  // Ponsel: panel muat tanpa scroll horizontal.
  await a.setViewportSize({ width: 360, height: 740 });
  expect(await a.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await a.screenshot({ path: "test-results/chat-mobile.png" });

  // Akhiri → kembali ke pemberitahuan; percakapan lama tidak bisa dipakai lagi.
  await panel.getByRole("button", { name: "Akhiri chat" }).click();
  await expect(panel.getByRole("button", { name: "Setuju & mulai chat" })).toBeVisible();
  const after = await a.evaluate(async () => (await fetch("/_api/public/chat/conversation")).status);
  expect(after).toBe(401);

  expect(errors).toEqual([]);
  await owner.close();
  await guest.close();
});
