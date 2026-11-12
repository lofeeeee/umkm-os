// ADR-0014: static export; Caddy melayani hasil `out/`. Tidak ada Node server di produksi.
/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "export",
  reactStrictMode: true,
  poweredByHeader: false,
  images: { unoptimized: true },
};
export default nextConfig;
