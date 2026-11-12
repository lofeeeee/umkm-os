"use client";

// Asisten AI (P5, ADR-0011): provider platform/BYOK (kunci hanya masuk, tampil masked), pengaturan chatbot,
// tool, basis pengetahuan, uji coba, pemakaian token.

import { useCallback, useEffect, useState, type FormEvent } from "react";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";

interface Provider {
  id: string; name: string; source: string; provider_type: string; base_url: string | null; model: string;
  status: string; key_hint: string | null; has_key: boolean;
  capabilities: { tool_calling: boolean; embeddings: boolean };
}
interface Config {
  enabled: boolean; mode: string; primary_provider_id: string | null; fallback_provider_id: string | null;
  temperature: number; max_output_tokens: number; timeout_ms: number; system_prompt: string; welcome_message: string;
  fallback_message: string; allowed_topics: string[]; restricted_topics: string[];
  handover_policy: Record<string, unknown>; operating_schedule: Record<string, unknown>; monthly_token_budget: number | null;
}
interface ConfigView { config: Config; tools: { name: string; enabled: boolean }[]; platform: { available: boolean; models: string[]; monthly_quota: number | null } }
interface Doc { id: string; title: string; status: string; chunk_count: number; error_code: string | null }
interface Usage { platform_tokens: number; byok_tokens: number; platform_quota: number | null; byok_budget: number | null }
interface TestResult { decision: string; reply: string | null; status: string; error_code: string | null; tool_calls: { name: string; status: string }[]; prompt_tokens: number; completion_tokens: number }

const MODES: Record<string, string> = {
  hybrid: "Hybrid — AI menjawab yang jelas, sisanya ke staf (disarankan)",
  auto: "Auto — AI menjawab langsung",
  assisted: "Assisted — AI membuat draf, staf mengirim",
  human_only: "Human Only — AI tidak menjawab",
};
const TOOLS: Record<string, string> = {
  search_products: "Cari produk", get_product_detail: "Detail produk", check_stock: "Cek stok",
  get_business_hours: "Jam buka", get_business_location: "Lokasi toko", search_faq: "Cari FAQ / pengetahuan",
  get_customer_orders: "Pesanan pelanggan (terverifikasi)", get_order_status: "Status pesanan (terverifikasi)",
  create_order_draft: "Draf pesanan (tanpa pembayaran)", request_human_support: "Alihkan ke staf",
};
const DECISION: Record<string, string> = {
  send_auto: "Dikirim otomatis", draft_for_admin: "Draf untuk staf", handover: "Dialihkan ke staf", human_queue: "Menunggu staf",
};
const topics = (s: string) => s.split(",").map((t) => t.trim()).filter(Boolean);

export default function AiPage() {
  return <WithTenant>{(ctx) => <Ai ctx={ctx} />}</WithTenant>;
}

function Ai({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/ai`;
  const [providers, setProviders] = useState<Provider[]>([]);
  const [view, setView] = useState<ConfigView | null>(null);
  const [cfg, setCfg] = useState<Config | null>(null);
  const [docs, setDocs] = useState<Doc[]>([]);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [msg, setMsg] = useState<{ ok?: string; err?: string }>({});
  const load = useCallback(() => Promise.all([
    api<Provider[]>("GET", `${base}/providers`).then(setProviders),
    api<ConfigView>("GET", `${base}/config`).then((v) => { setView(v); setCfg(v.config); }),
    api<Doc[]>("GET", `${base}/knowledge`).then(setDocs),
    api<Usage>("GET", `${base}/usage`).then(setUsage),
  ]).catch((e) => setMsg({ err: describe(e) })), [base]);
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setMsg({});
    try { await f(); setMsg({ ok }); await load(); } catch (e) { setMsg({ err: describe(e) }); }
  }
  const manage = ctx.can("ai_config.manage");
  const manageProv = ctx.can("ai_provider.manage");
  if (!view || !cfg) return <><Alert kind="error">{msg.err}</Alert><p className="d-muted" aria-busy="true">Memuat…</p></>;
  return (
    <>
      <h1>Asisten AI</h1>
      <Alert kind="error">{msg.err}</Alert>
      <Alert kind="ok">{msg.ok}</Alert>
      {usage && (
        <p className="d-muted">
          Pemakaian bulan ini: platform {usage.platform_tokens.toLocaleString("id-ID")}
          {usage.platform_quota ? ` / ${usage.platform_quota.toLocaleString("id-ID")}` : ""} token · kunci sendiri {usage.byok_tokens.toLocaleString("id-ID")}
          {usage.byok_budget ? ` / ${usage.byok_budget.toLocaleString("id-ID")}` : ""} token
        </p>
      )}

      <section className="d-card">
        <h2>Provider</h2>
        <ul className="d-thread">
          {providers.map((p) => (
            <li key={p.id}>
              <strong>{p.name}</strong> <span className="d-muted">{p.source === "platform" ? "AI platform" : `${p.provider_type} · ${p.base_url}`} · {p.model} · {p.status === "active" ? "aktif" : "nonaktif"}{p.key_hint ? ` · kunci ${p.key_hint}` : ""}</span>
              {manageProv && (
                <div className="d-row">
                  <button className="d-btn d-btn-sm" onClick={() => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { active: p.status !== "active" } }), "Provider diperbarui.")}>{p.status === "active" ? "Nonaktifkan" : "Aktifkan"}</button>
                  {p.source === "byok" && <RotateKey onSave={(k) => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { api_key: k } }), "Kunci diganti.")} />}
                  <button className="d-btn d-btn-sm" onClick={() => run(() => api("DELETE", `${base}/providers/${p.id}`), "Provider dihapus.")}>Hapus</button>
                </div>
              )}
            </li>
          ))}
          {providers.length === 0 && <li className="d-muted">Belum ada provider.</li>}
        </ul>
        {manageProv && <AddProvider view={view} onAdd={(body) => run(() => api("POST", `${base}/providers`, { json: body }), "Provider ditambahkan.")} />}
      </section>

      <section className="d-card">
        <h2>Pengaturan chatbot</h2>
        <fieldset disabled={!manage} style={{ border: 0, padding: 0 }}>
          <label className="d-check"><input type="checkbox" checked={cfg.enabled} onChange={(e) => setCfg({ ...cfg, enabled: e.target.checked })} /> Aktifkan chatbot untuk pelanggan</label>
          <label className="d-field"><span>Mode</span>
            <select value={cfg.mode} onChange={(e) => setCfg({ ...cfg, mode: e.target.value })}>
              {Object.entries(MODES).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </label>
          {(["primary_provider_id", "fallback_provider_id"] as const).map((f) => (
            <label key={f} className="d-field"><span>{f === "primary_provider_id" ? "Provider utama" : "Provider cadangan"}</span>
              <select value={cfg[f] ?? ""} onChange={(e) => setCfg({ ...cfg, [f]: e.target.value || null })}>
                <option value="">—</option>
                {providers.filter((p) => p.status === "active").map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </label>
          ))}
          <div className="d-row">
            <label className="d-field"><span>Temperature (0–2)</span><input type="number" step="0.1" min={0} max={2} value={cfg.temperature} onChange={(e) => setCfg({ ...cfg, temperature: Number(e.target.value) })} /></label>
            <label className="d-field"><span>Maks token jawaban</span><input type="number" min={16} max={4096} value={cfg.max_output_tokens} onChange={(e) => setCfg({ ...cfg, max_output_tokens: Number(e.target.value) })} /></label>
            <label className="d-field"><span>Batas waktu (ms)</span><input type="number" min={1000} max={60000} step={1000} value={cfg.timeout_ms} onChange={(e) => setCfg({ ...cfg, timeout_ms: Number(e.target.value) })} /></label>
          </div>
          <label className="d-field"><span>Panduan untuk AI (gaya bahasa, info toko)</span><textarea rows={4} maxLength={4000} value={cfg.system_prompt} onChange={(e) => setCfg({ ...cfg, system_prompt: e.target.value })} /></label>
          <label className="d-field"><span>Pesan sambutan</span><input maxLength={500} value={cfg.welcome_message} onChange={(e) => setCfg({ ...cfg, welcome_message: e.target.value })} /></label>
          <label className="d-field"><span>Pesan saat dialihkan ke staf</span><input maxLength={500} value={cfg.fallback_message} onChange={(e) => setCfg({ ...cfg, fallback_message: e.target.value })} /></label>
          <label className="d-field"><span>Topik yang dilayani (pisahkan koma)</span><input defaultValue={cfg.allowed_topics.join(", ")} onBlur={(e) => setCfg({ ...cfg, allowed_topics: topics(e.target.value) })} /></label>
          <label className="d-field"><span>Topik terlarang — langsung ke staf (pisahkan koma)</span><input defaultValue={cfg.restricted_topics.join(", ")} onBlur={(e) => setCfg({ ...cfg, restricted_topics: topics(e.target.value) })} /></label>
          <label className="d-field"><span>Batas token bulanan untuk kunci sendiri (opsional)</span><input type="number" min={1} value={cfg.monthly_token_budget ?? ""} onChange={(e) => setCfg({ ...cfg, monthly_token_budget: e.target.value ? Number(e.target.value) : null })} /></label>
          <button className="d-btn d-btn-primary" onClick={() => run(() => api("PUT", `${base}/config`, { json: cfg }), "Pengaturan tersimpan.")}>Simpan pengaturan</button>
        </fieldset>
        <h3>Kemampuan (tool)</h3>
        <div className="d-perms">
          {view.tools.map((t) => (
            <label key={t.name} className="d-check">
              <input type="checkbox" disabled={!manage} checked={t.enabled} onChange={(e) => run(() => api("PUT", `${base}/tools`, { json: { tools: [{ name: t.name, enabled: e.target.checked }] } }), "Tool diperbarui.")} /> {TOOLS[t.name] ?? t.name}
            </label>
          ))}
        </div>
      </section>

      <section className="d-card">
        <h2>Basis pengetahuan</h2>
        <ul className="d-thread">
          {docs.map((d) => (
            <li key={d.id}>
              <strong>{d.title}</strong> <span className="d-muted">{d.status === "indexed" ? `siap (${d.chunk_count} bagian)` : d.status === "pending" ? "sedang diproses" : `gagal${d.error_code ? `: ${d.error_code}` : ""}`}</span>
              {manage && (
                <div className="d-row">
                  <button className="d-btn d-btn-sm" onClick={() => run(() => api("POST", `${base}/knowledge/${d.id}/reindex`), "Dijadwalkan ulang.")}>Proses ulang</button>
                  <button className="d-btn d-btn-sm" onClick={() => run(() => api("DELETE", `${base}/knowledge/${d.id}`), "Dokumen dihapus.")}>Hapus</button>
                </div>
              )}
            </li>
          ))}
          {docs.length === 0 && <li className="d-muted">Belum ada dokumen. Tambahkan FAQ, kebijakan retur, info pengiriman, dll.</li>}
        </ul>
        {manage && <AddDoc onAdd={(title, body) => run(() => api("POST", `${base}/knowledge`, { json: { title, body } }), "Dokumen ditambahkan; akan diproses di latar.")} />}
      </section>

      {manage && <TestConsole url={`${base}/test`} onDone={() => { void load(); }} />}
    </>
  );
}

function RotateKey({ onSave }: { onSave: (k: string) => void }) {
  const [k, setK] = useState("");
  return (
    <>
      <input aria-label="Kunci API baru" type="password" autoComplete="off" placeholder="Kunci baru" value={k} onChange={(e) => setK(e.target.value)} />
      <button className="d-btn d-btn-sm" disabled={k.length < 8} onClick={() => { onSave(k); setK(""); }}>Ganti kunci</button>
    </>
  );
}

function AddProvider({ view, onAdd }: { view: ConfigView; onAdd: (b: Record<string, string>) => void }) {
  const [source, setSource] = useState(view.platform.available ? "platform" : "byok");
  const [f, setF] = useState({ name: "", provider_type: "openai", base_url: "https://api.openai.com/v1", model: view.platform.models[0] ?? "", api_key: "" });
  function submit(e: FormEvent) {
    e.preventDefault();
    onAdd(source === "platform"
      ? { name: f.name, source, model: f.model }
      : { name: f.name, source, provider_type: f.provider_type, base_url: f.base_url, model: f.model, api_key: f.api_key });
    setF({ ...f, api_key: "" });
  }
  return (
    <form onSubmit={submit}>
      <h3>Tambah provider</h3>
      <div className="d-row">
        <label className="d-field"><span>Sumber</span>
          <select value={source} onChange={(e) => setSource(e.target.value)}>
            {view.platform.available && <option value="platform">AI platform (kuota bulanan)</option>}
            <option value="byok">Kunci API sendiri</option>
          </select>
        </label>
        <label className="d-field"><span>Nama</span><input required maxLength={60} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></label>
      </div>
      {source === "platform" ? (
        <label className="d-field"><span>Model</span>
          <select value={f.model} onChange={(e) => setF({ ...f, model: e.target.value })}>{view.platform.models.map((m) => <option key={m}>{m}</option>)}</select>
        </label>
      ) : (
        <>
          <div className="d-row">
            <label className="d-field"><span>Jenis</span>
              <select value={f.provider_type} onChange={(e) => setF({ ...f, provider_type: e.target.value })}>
                {["openai", "openrouter", "groq", "together", "custom"].map((t) => <option key={t}>{t}</option>)}
              </select>
            </label>
            <label className="d-field"><span>Model</span><input required value={f.model} onChange={(e) => setF({ ...f, model: e.target.value })} /></label>
          </div>
          <label className="d-field"><span>Alamat endpoint (HTTPS)</span><input required type="url" value={f.base_url} onChange={(e) => setF({ ...f, base_url: e.target.value })} /></label>
          <label className="d-field"><span>Kunci API (disimpan terenkripsi, tidak ditampilkan lagi)</span><input required type="password" autoComplete="off" minLength={8} value={f.api_key} onChange={(e) => setF({ ...f, api_key: e.target.value })} /></label>
        </>
      )}
      <button className="d-btn d-btn-primary">Tambah provider</button>
    </form>
  );
}

function AddDoc({ onAdd }: { onAdd: (t: string, b: string) => void }) {
  const [t, setT] = useState("");
  const [b, setB] = useState("");
  return (
    <div>
      <label className="d-field"><span>Judul dokumen</span><input maxLength={200} value={t} onChange={(e) => setT(e.target.value)} /></label>
      <label className="d-field"><span>Isi</span><textarea rows={5} maxLength={50000} value={b} onChange={(e) => setB(e.target.value)} /></label>
      <button className="d-btn" disabled={!t.trim() || !b.trim()} onClick={() => { onAdd(t, b); setT(""); setB(""); }}>Tambah dokumen</button>
    </div>
  );
}

function TestConsole({ url, onDone }: { url: string; onDone: () => void }) {
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [r, setR] = useState<TestResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  async function ask() {
    setBusy(true);
    setErr(null);
    try { setR(await api<TestResult>("POST", url, { json: { message: q } })); onDone(); } catch (e) { setErr(describe(e)); } finally { setBusy(false); }
  }
  return (
    <section className="d-card">
      <h2>Uji coba</h2>
      <p className="d-muted">Coba pertanyaan pelanggan. Dihitung ke pemakaian token.</p>
      <div className="d-row">
        <input aria-label="Pertanyaan uji" style={{ flex: 1 }} maxLength={2000} value={q} onChange={(e) => setQ(e.target.value)} />
        <button className="d-btn d-btn-primary" disabled={busy || !q.trim()} onClick={ask}>{busy ? "Menunggu…" : "Tanya"}</button>
      </div>
      <Alert kind="error">{err}</Alert>
      {r && (
        <div aria-label="Hasil uji">
          <p><strong>{DECISION[r.decision] ?? r.decision}</strong>{r.status !== "ok" && <span className="d-muted"> ({r.status}{r.error_code ? `: ${r.error_code}` : ""})</span>}</p>
          {r.reply && <p style={{ whiteSpace: "pre-wrap" }}>{r.reply}</p>}
          {r.tool_calls.length > 0 && <p className="d-muted">Tool: {r.tool_calls.map((c) => `${TOOLS[c.name] ?? c.name} (${c.status})`).join(", ")}</p>}
          <p className="d-muted">{r.prompt_tokens + r.completion_tokens} token</p>
        </div>
      )}
    </section>
  );
}
