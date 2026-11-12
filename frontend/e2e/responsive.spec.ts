// AC-WEB-017 (375/768/1024/1440, tanpa scroll horizontal, semua section aktif terlihat)
// AC-WEB-018 (teks <script> literal, CSP ketat tidak memblokir aplikasi sendiri).
import { expect, test } from "@playwright/test";

const SITE = process.env.SITE_URL ?? "";

for (const width of [375, 768, 1024, 1440]) {
  test(`situs publik ${width}px`, async ({ page }) => {
    test.skip(!SITE, "SITE_URL tidak diset");
    const violations: string[] = [];
    page.on("console", (m) => { if (/Content Security Policy|Refused to/i.test(m.text())) violations.push(m.text()); });
    page.on("pageerror", (e) => violations.push(String(e)));
    await page.setViewportSize({ width, height: 900 });
    await page.goto(SITE);
    const sections = page.locator("[data-section-type]");
    await expect(sections.first()).toBeVisible();
    const n = await sections.count();
    expect(n).toBeGreaterThan(3);
    for (let i = 0; i < n; i++) {
      const box = await sections.nth(i).boundingBox();
      expect(box, `section ${i}`).not.toBeNull();
      expect(box!.width).toBeGreaterThan(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width + 1);
    }
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow, "scroll horizontal").toBeLessThanOrEqual(0);
    // Judul hero dirender PERSIS seperti data API (teks literal, termasuk markup bila ada).
    const api = await page.evaluate(async () => (await fetch("/_api/public/site")).json());
    const hero = api.data.page.content.sections.find((x: { type: string }) => x.type === "hero");
    await expect(page.locator("[data-section-type=hero] h1")).toHaveText(hero.data.title);
    expect(await page.evaluate(() => document.querySelectorAll("section script").length)).toBe(0);
    expect(violations).toEqual([]);
    await page.screenshot({ path: `test-results/site-${width}.png`, fullPage: true });
  });
}
