// Uji browser terhadap stack nyata (Caddy + API + PostgreSQL), bukan mock.
// Env: SITE_URL (mis. http://toko.localhost), APP_URL (http://app.localhost), E2E_EMAIL, E2E_PASSWORD.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  timeout: 30_000,
  retries: 0,
  reporter: [["list"]],
  use: {
    headless: true,
    trace: "off",
    // E2E_HOST_MAP=umkm.test -> *.umkm.test diarahkan ke Caddy lokal (tanpa mengubah /etc/hosts).
    launchOptions: process.env.E2E_HOST_MAP
      ? { args: [`--host-resolver-rules=MAP *.${process.env.E2E_HOST_MAP} 127.0.0.1, MAP ${process.env.E2E_HOST_MAP} 127.0.0.1`] }
      : {},
  },
});
