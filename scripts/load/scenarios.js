// P8-2: skenario beban PRD.md §13.4 (benchmark, BUKAN jaminan kapasitas). Dijalankan oleh scripts/load/run.sh.
//   SCENARIO=public  100 pengunjung situs toko (site, katalog, detail produk) tersebar di 500 toko
//   SCENARIO=admin    30 admin dashboard (pesanan, produk, pelanggan, analitik, percakapan, tiket)
//   SCENARIO=api      50 permintaan API campuran: 70% baca, 30% tulis (buat tiket + balasan)
//   SCENARIO=chat     20 sesi chat pelanggan di toko Pro (AI tiruan), 5 pesan per sesi
//   SCENARIO=all     keempatnya bersamaan
// Env: BASE (default http://127.0.0.1:8080), ROOT (umkm.test), DURATION (60s), TENANTS (path JSON).
// Sesi admin: token deterministik "lt-<slug>" dari seed; CSRF = base64url(sha256("csrf:" + token)).
import http from "k6/http";
import { check, sleep } from "k6";
import crypto from "k6/crypto";
import encoding from "k6/encoding";
import { Trend } from "k6/metrics";

const BASE = __ENV.BASE || "http://127.0.0.1:8080";
const ROOT = __ENV.ROOT || "umkm.test";
const DURATION = __ENV.DURATION || "60s";
const SCENARIO = __ENV.SCENARIO || "public";
const TENANTS = JSON.parse(open(__ENV.TENANTS || "/data/tenants.json"));
const PRO = TENANTS.filter((t) => t.pro);

const readLatency = new Trend("read_latency", true);
const writeLatency = new Trend("write_latency", true);
const chatLatency = new Trend("chat_reply_latency", true);

const VUS = { public: 100, admin: 30, api: 50, chat: 20 };
const THRESHOLDS = {
  // NFR-003 (TTFB publik p95 < 500 ms) — diukur langsung ke API (tanpa Caddy/CDN).
  public: { http_req_failed: ["rate<0.01"], "http_req_duration": ["p(95)<500"] },
  // NFR-001 baca p95 < 300 ms.
  admin: { http_req_failed: ["rate<0.01"], read_latency: ["p(95)<300"] },
  // NFR-001 / NFR-002.
  api: { http_req_failed: ["rate<0.01"], read_latency: ["p(95)<300"], write_latency: ["p(95)<500"] },
  // Chat: tanpa NFR khusus di PRD; dicatat (termasuk latensi provider tiruan).
  chat: { http_req_failed: ["rate<0.01"] },
};

// SCENARIO=all: keempat beban berjalan BERSAMAAN (PRD §13.4 "concurrent"); ambang per skenario via tag.
const NAMES = SCENARIO === "all" ? Object.keys(VUS) : [SCENARIO];
function tagged(name) {
  const out = {};
  for (const [k, v] of Object.entries(THRESHOLDS[name])) out[SCENARIO === "all" ? `${k}{scenario:${name}}` : k] = v;
  return out;
}
export const options = {
  scenarios: Object.fromEntries(NAMES.map((n) => [n, {
    executor: "constant-vus", vus: VUS[n], duration: DURATION, gracefulStop: "15s", exec: n,
  }])),
  thresholds: Object.assign({}, ...NAMES.map(tagged)),
  summaryTrendStats: ["avg", "med", "p(90)", "p(95)", "p(99)", "max"],
};

function pick(list) {
  return list[Math.floor(Math.random() * list.length)];
}

function adminHeaders(t) {
  const token = `lt-${t.slug}`;
  const csrf = encoding.b64encode(crypto.sha256(`csrf:${token}`, "binary"), "rawurl");
  return {
    Cookie: `umkm_session=${token}`,
    "x-csrf-token": csrf,
    Origin: "http://app." + ROOT,
    "content-type": "application/json",
  };
}

function ip() {
  return `10.${__VU % 250}.${Math.floor(Math.random() * 250)}.${1 + (__ITER % 250)}`;
}

function publicVisit() {
  const t = pick(TENANTS);
  const h = { headers: { Host: `${t.slug}.${ROOT}` } };
  const s = http.get(`${BASE}/api/v1/public/site`, h);
  check(s, { "site 200": (r) => r.status === 200 });
  const p = http.get(`${BASE}/api/v1/public/products?limit=24`, h);
  check(p, { "products 200": (r) => r.status === 200 });
  const n = 1 + Math.floor(Math.random() * 20);
  const d = http.get(`${BASE}/api/v1/public/products/produk-${n}`, h);
  check(d, { "product 200": (r) => r.status === 200 });
  sleep(1);
}

const READS = [
  (t) => `/api/v1/tenants/${t.t}/orders?limit=20`,
  (t) => `/api/v1/tenants/${t.t}/products?limit=20`,
  (t) => `/api/v1/tenants/${t.t}/customers?limit=20`,
  (t) => `/api/v1/tenants/${t.t}/analytics/overview`,
  (t) => `/api/v1/tenants/${t.t}/conversations?limit=20`,
  (t) => `/api/v1/tenants/${t.t}/tickets?limit=20`,
];

// Nama metrik tanpa tenant id (menghindari ratusan ribu time series di k6).
const READ_NAMES = ["orders", "products", "customers", "analytics/overview", "conversations", "tickets"];

function adminRead(t) {
  const i = Math.floor(Math.random() * READS.length);
  const r = http.get(`${BASE}${READS[i](t)}`, { headers: adminHeaders(t), tags: { kind: "read", name: READ_NAMES[i] } });
  readLatency.add(r.timings.duration);
  check(r, { "read 200": (x) => x.status === 200 });
}

function adminWrite(t) {
  const h = { headers: adminHeaders(t), tags: { kind: "write" } };
  const c = http.post(`${BASE}/api/v1/tenants/${t.t}/tickets`, JSON.stringify({ subject: `Beban ${__VU}-${__ITER}` }),
    Object.assign({}, h, { tags: { kind: "write", name: "tickets" } }));
  writeLatency.add(c.timings.duration);
  check(c, { "ticket 201": (x) => x.status === 201 });
  if (c.status === 201) {
    const id = c.json("data.id");
    const m = http.post(`${BASE}/api/v1/tenants/${t.t}/tickets/${id}/messages`,
      JSON.stringify({ body: "Balasan uji beban", internal: false }),
      Object.assign({}, h, { tags: { kind: "write", name: "tickets/{id}/messages" } }));
    writeLatency.add(m.timings.duration);
    check(m, { "reply 2xx": (x) => x.status >= 200 && x.status < 300 });
  }
}

function chatSession() {
  const t = pick(PRO);
  const origin = `http://${t.slug}.${ROOT}`;
  const base = { Host: `${t.slug}.${ROOT}`, Origin: origin, "content-type": "application/json", "X-Forwarded-For": ip() };
  const s = http.post(`${BASE}/api/v1/public/chat/conversations`,
    JSON.stringify({ consent_version: "ai-consent-v1-20261113", accept: true }), { headers: base });
  check(s, { "chat start 201": (r) => r.status === 201 });
  if (s.status !== 201) { sleep(1); return; }
  const cookie = (s.headers["Set-Cookie"] || "").split(";")[0];
  for (let i = 0; i < 5; i++) {
    const m = http.post(`${BASE}/api/v1/public/chat/messages`, JSON.stringify({ body: `Ada produk ${i + 1}?` }),
      { headers: Object.assign({ Cookie: cookie }, base) });
    chatLatency.add(m.timings.duration);
    check(m, { "chat msg 200": (r) => r.status === 200 });
    sleep(1);
  }
}

export function public_() { publicVisit(); }
export function chat() { chatSession(); }
export function admin() { adminRead(pick(TENANTS)); sleep(0.5); }
// api: 70% baca, 30% tulis, tanpa jeda (permintaan bersamaan).
export function api() { const t = pick(TENANTS); if (Math.random() < 0.7) adminRead(t); else adminWrite(t); }
export { public_ as public };
