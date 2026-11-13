// ADR-0014: static export; Caddy melayani hasil `out/`. Tidak ada Node server di produksi.
/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "export",
  reactStrictMode: true,
  poweredByHeader: false,
  images: { unoptimized: true },
  // ADR-0018: build ID tetap (dari commit bila tersedia) agar dua build sumber yang sama menghasilkan HTML
  // identik — shell <head> dari API harus cocok byte-per-byte dengan CSP hash build yang disajikan Caddy.
  generateBuildId: async () => process.env.NEXT_BUILD_ID || "umkm",
  // Dev: origin 127.0.0.1 dipakai agar cookie sesi API (SameSite=Lax, host 127.0.0.1) tidak
  // ditolak sebagai cross-site (localhost:3000 vs 127.0.0.1:8080 = site berbeda bagi browser).
  allowedDevOrigins: ["127.0.0.1"],
};
export default nextConfig;
