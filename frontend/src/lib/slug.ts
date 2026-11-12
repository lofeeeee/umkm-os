// Slug halaman dari path situs publik (sama dengan is_valid_page_slug di Rust).
export function pageSlug(pathname: string): string | null {
  const s = pathname.replace(/^\/+|\/+$/g, "");
  if (s === "" || s === "site" || s === "site.html") return "home";
  return /^[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?$/.test(s) ? s : null;
}
