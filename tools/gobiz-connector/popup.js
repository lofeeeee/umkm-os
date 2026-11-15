// Membaca cookie sesi GoBiz (httpOnly, hanya bisa via ekstensi) lalu mengoper ke dashboard UMKM OS
// melalui fragment URL (#gopay_connect=...), yang tidak terkirim ke server mana pun selain lewat
// POST HTTPS dari halaman dashboard. Tidak ada token yang disalin manual.
const $ = (id) => document.getElementById(id);

async function gobizCookieString() {
  const cookies = await chrome.cookies.getAll({ url: "https://api.gobiz.co.id/" });
  // Kirim semua cookie (server mengekstrak access_token & refresh_token). Defensif terhadap
  // perubahan nama cookie di sisi GoBiz.
  return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
}

async function hasSession() {
  const cookies = await chrome.cookies.getAll({ url: "https://api.gobiz.co.id/" });
  const names = new Set(cookies.map((c) => c.name));
  return names.has("access_token") && names.has("refresh_token");
}

async function refresh() {
  const ok = await hasSession();
  $("status").className = "status " + (ok ? "ok" : "no");
  $("status").textContent = ok ? "Sudah login GoBiz ✓ — siap dihubungkan." : "Belum login GoBiz di browser ini.";
  $("connect").disabled = !ok;
}

$("login").addEventListener("click", () => {
  chrome.tabs.create({ url: "https://portal.gofoodmerchant.co.id/" });
});

$("connect").addEventListener("click", async () => {
  const url = ($("url").value || "").trim().replace(/\/+$/, "");
  if (!/^https?:\/\//.test(url)) { $("status").className = "status no"; $("status").textContent = "Isi URL dashboard UMKM OS dulu."; return; }
  const cookie = await gobizCookieString();
  const blob = btoa(unescape(encodeURIComponent(cookie)));
  await chrome.storage.local.set({ url });
  chrome.tabs.create({ url: `${url}/shop-settings#gopay_connect=${blob}` });
  window.close();
});

chrome.storage.local.get("url", ({ url }) => { if (url) $("url").value = url; });
$("url").addEventListener("change", () => chrome.storage.local.set({ url: $("url").value.trim() }));
refresh();
