// Asisten AI (P5) di browser, stack nyata + provider tiruan lokal (frontend/e2e/mock-llm.py) yang
// didaftarkan sebagai provider platform "trusted internal". Env: APP_URL, API_URL, E2E_AI=1.
import { expect, test } from "@playwright/test";

const APP = process.env.APP_URL ?? "";
const API = process.env.API_URL ?? "";

test("asisten AI: provider platform, pengetahuan diindeks worker, uji coba memakai tool", async ({ page }) => {
  test.skip(!APP || !API || !process.env.E2E_AI, "APP_URL/API_URL/E2E_AI tidak diset");
  test.setTimeout(90_000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) errors.push(m.text()); });
  const slug = `ai${Date.now().toString(36)}`;
  await page.goto(`${APP}/login`);
  const reg = await page.evaluate(async ([api, b]) => (await fetch(`${api}/api/v1/auth/register`, {
    method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify(b),
  })).status, [API, { email: `${slug}@x.id`, password: "rahasia-ai-123", business_name: `Toko ${slug}`, slug, accept_terms: true, accept_privacy: true }] as const);
  expect(reg).toBe(201);
  await page.goto(`${APP}/login`);
  await page.getByLabel("Email").fill(`${slug}@x.id`);
  await page.getByLabel("Password").fill("rahasia-ai-123");
  await page.getByRole("button", { name: "Masuk" }).click();
  await page.getByRole("link", { name: "Kelola website" }).click();
  await page.waitForURL(/\/website\?tenant=/);
  const q = new URL(page.url()).search;

  await page.getByRole("link", { name: "Asisten AI" }).click();
  await expect(page.getByRole("heading", { name: "Asisten AI" })).toBeVisible();
  // Provider platform (model dari daftar operator; tanpa URL/kunci).
  await page.getByLabel("Nama").fill("AI Platform");
  await page.getByRole("button", { name: "Tambah provider" }).click();
  await expect(page.getByText("Provider ditambahkan.")).toBeVisible();
  await expect(page.getByText(/AI platform · mock-1 · aktif/)).toBeVisible();
  await page.getByLabel("Provider utama").selectOption({ label: "AI Platform" });
  await page.getByLabel("Topik terlarang — langsung ke staf (pisahkan koma)").fill("politik");
  await page.getByLabel("Topik terlarang — langsung ke staf (pisahkan koma)").blur();
  await page.getByRole("button", { name: "Simpan pengaturan" }).click();
  await expect(page.getByText("Pengaturan tersimpan.")).toBeVisible();
  // Nilai di luar batas ditolak server dengan pesan field.
  await page.getByLabel("Temperature (0–2)").fill("3");
  await page.getByRole("button", { name: "Simpan pengaturan" }).click();
  await expect(page.getByText(/di luar/)).toBeVisible();
  await page.getByLabel("Temperature (0–2)").fill("0.3");

  // Basis pengetahuan → worker mengindeks di latar.
  await page.getByLabel("Judul dokumen").fill("Pengiriman");
  await page.getByLabel("Isi").fill("Pengiriman ke Bandung 2 hari kerja.");
  await page.getByRole("button", { name: "Tambah dokumen" }).click();
  await expect(page.getByText("sedang diproses")).toBeVisible();
  await expect(async () => {
    await page.goto(`${APP}/ai${q}`);
    await expect(page.getByText(/siap \(1 bagian\)/)).toBeVisible({ timeout: 2_000 });
  }).toPass({ timeout: 40_000 });

  // Uji coba: model meminta tool jam buka → backend mengeksekusi → jawaban berdasar data → Hybrid mengirim.
  await page.getByLabel("Pertanyaan uji").fill("Jam berapa toko buka?");
  await page.getByRole("button", { name: "Tanya" }).click();
  const res = page.getByLabel("Hasil uji");
  await expect(res).toContainText("Dikirim otomatis");
  await expect(res).toContainText("Halo dari asisten uji. Data jam buka diterima.");
  await expect(res).toContainText("Jam buka (ok)");
  // Topik terlarang → langsung ke staf tanpa provider.
  await page.getByLabel("Pertanyaan uji").fill("Bagaimana soal politik?");
  await page.getByRole("button", { name: "Tanya" }).click();
  await expect(res).toContainText("Dialihkan ke staf");
  await expect(page.getByText(/Pemakaian bulan ini: platform [1-9]/)).toBeVisible();
  await page.setViewportSize({ width: 375, height: 800 });
  await page.screenshot({ path: "test-results/ai-mobile.png", fullPage: true });
  expect(errors).toEqual([]);
});
