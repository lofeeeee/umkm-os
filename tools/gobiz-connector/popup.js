// Production popup: deteksi login GoBiz otomatis (polling cookie), lalu oper sesi ke dashboard
// UMKM OS via fragment URL (#gopay_connect=<base64 cookie>). Tidak ada token yang disalin manual.
// Token hanya dibaca dari cookie domain GoBiz & dikirim ke dashboard yang Anda tentukan.
const PORTAL = "https://portal.gofoodmerchant.co.id/";
const $ = (id) => document.getElementById(id);

async function gobizCookies() {
  try { return await chrome.cookies.getAll({ url: PORTAL }); } catch { return []; }
}
async function sessionReady() {
  const names = new Set((await gobizCookies()).map((c) => c.name));
  return names.has("access_token") && names.has("refresh_token");
}

function setLogin(ok) {
  const el = $("loginStatus");
  el.className = "chip " + (ok ? "ok" : "no");
  el.textContent = ok ? "Sudah login GoBiz ✓" : "Belum login";
  $("step1").classList.toggle("done", ok);
  $("connect").disabled = !ok || !/^https?:\/\/.+/.test($("url").value.trim());
}

async function refresh() { setLogin(await sessionReady()); }

$("login").addEventListener("click", () => chrome.tabs.create({ url: PORTAL }));

$("url").addEventListener("input", () => { refresh(); });
$("url").addEventListener("change", () => chrome.storage.local.set({ url: $("url").value.trim() }));

$("connect").addEventListener("click", async () => {
  if (!(await sessionReady())) { await refresh(); return; }
  const url = $("url").value.trim().replace(/\/+$/, "");
  if (!/^https?:\/\/.+/.test(url)) return;
  const cookie = (await gobizCookies()).map((c) => `${c.name}=${c.value}`).join("; ");
  const blob = btoa(unescape(encodeURIComponent(cookie)));
  await chrome.storage.local.set({ url });
  await chrome.tabs.create({ url: `${url}/shop-settings#gopay_connect=${blob}` });
  window.close();
});

chrome.storage.local.get("url", ({ url }) => { if (url) $("url").value = url; refresh(); });
refresh();
// Perbarui status login otomatis selama popup terbuka.
setInterval(refresh, 1500);
