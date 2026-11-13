"use client";

// Asisten AI (P5, ADR-0011): provider platform/BYOK (kunci hanya masuk, tampil masked), pengaturan chatbot,
// tool, basis pengetahuan, uji coba, pemakaian token.

import { useCallback, useEffect, useState, type FormEvent } from "react";

import { BookOpen, Brain, Plus, Robot, Trash } from "@phosphor-icons/react/ssr";

import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, describe } from "@/lib/api";
import { angka } from "@/lib/labels";
import { confirm } from "@/ui/dialog";
import { Badge, BusyButton, Card, EmptyState, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";

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

const MODES: { value: string; title: string; desc: string }[] = [
  { value: "hybrid", title: "Hybrid (disarankan)", desc: "AI menjawab pertanyaan yang jelas; sisanya diteruskan ke staf." },
  { value: "auto", title: "Otomatis", desc: "AI menjawab semua pertanyaan langsung." },
  { value: "assisted", title: "Dibantu", desc: "AI menyiapkan draf; staf memeriksa lalu mengirim." },
  { value: "human_only", title: "Hanya staf", desc: "AI tidak menjawab; semua chat masuk antrean staf." },
];
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

/** `limit`: null = tanpa batas, 0 = tidak termasuk paket, >0 = kuota bulanan. */
function Meter({ label, used, limit }: { label: string; used: number; limit: number | null }) {
  const pct = limit ? Math.min(used / limit, 1) : 0;
  return (
    <div className="d-meter">
      <div className="d-meter-head"><span>{label}</span><span>{angka(used)}{limit ? ` / ${angka(limit)}` : ""} token</span></div>
      {limit === 0 ? <span className="d-hint">Tidak termasuk paket Anda</span>
        : limit ? <div className="d-progress"><span style={{ transform: `scaleX(${pct})`, background: pct > 0.9 ? "var(--danger)" : pct > 0.7 ? "var(--warn)" : undefined }} /></div>
        : <span className="d-hint">Tanpa batas</span>}
    </div>
  );
}

/** Teks kuota untuk pembaca layar & deskripsi kartu. */
const quotaText = (used: number, limit: number | null) =>
  `${angka(used)}${limit === 0 ? " (tidak termasuk paket)" : limit ? ` / ${angka(limit)}` : ""} token`;

function Ai({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}/ai`;
  const [providers, setProviders] = useState<Provider[]>([]);
  // Konfigurasi identik berulang membingungkan: tidak ada cara membedakan kartunya di UI.
  const dupes = providers.length - new Set(providers.map((p) => `${p.source}|${p.provider_type}|${p.model}|${p.base_url ?? ""}`)).size;
  const [view, setView] = useState<ConfigView | null>(null);
  const [cfg, setCfg] = useState<Config | null>(null);
  const [docs, setDocs] = useState<Doc[]>([]);
  const [usage, setUsage] = useState<Usage | null>(null);
  /** Paket tidak menyertakan token AI platform (mis. Starter): batas efektif 0 dari server. */
  const platformBlocked = usage?.platform_quota === 0;
  const [err, setErr] = useState<string | null>(null);
  const [cfgErr, setCfgErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const load = useCallback(() => Promise.all([
    api<Provider[]>("GET", `${base}/providers`).then(setProviders),
    api<ConfigView>("GET", `${base}/config`).then((v) => { setView(v); setCfg(v.config); }),
    api<Doc[]>("GET", `${base}/knowledge`).then(setDocs),
    api<Usage>("GET", `${base}/usage`).then(setUsage),
  ]).catch((e) => setErr(describe(e))), [base]);
  useEffect(() => { void load(); }, [load]);
  async function run(f: () => Promise<unknown>, ok: string) {
    setErr(null);
    try { await f(); toast.ok(ok); await load(); } catch (e) { setErr(describe(e)); }
  }
  const manage = ctx.can("ai_config.manage");
  const manageProv = ctx.can("ai_provider.manage");
  if (!view || !cfg) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const active = providers.filter((p) => p.status === "active");
  async function saveCfg() {
    if (!cfg) return;
    setSaving(true);
    setCfgErr(null);
    try { await api("PUT", `${base}/config`, { json: cfg }); toast.ok("Pengaturan tersimpan."); await load(); } catch (e) { setCfgErr(describe(e)); } finally { setSaving(false); }
  }
  return (
    <>
      <PageHeader
        title="Asisten AI"
        description="Chatbot yang menjawab pelanggan di website toko memakai data produk, jam buka, dan basis pengetahuan Anda."
        actions={<Badge tone={cfg.enabled && cfg.primary_provider_id ? "ok" : "neutral"}>{cfg.enabled && cfg.primary_provider_id ? "Chatbot aktif" : "Chatbot nonaktif"}</Badge>}
      />
      <Alert kind="error">{err}</Alert>
      {!cfg.primary_provider_id && <Alert kind="info">Langkah pertama: tambahkan penyedia AI, lalu pilih sebagai penyedia utama di pengaturan chatbot.</Alert>}
      {platformBlocked && providers.some((p) => p.source === "platform" && p.id === cfg.primary_provider_id) && (
        <Alert kind="error">
          Penyedia utama memakai AI platform, tetapi paket Anda tidak menyertakan token AI platform. Chatbot akan
          meneruskan semua pertanyaan ke staf. <a href={`/subscription?tenant=${ctx.tenant.id}`}>Tingkatkan ke Pro</a> atau
          tambahkan penyedia dengan kunci API sendiri.
        </Alert>
      )}

      {usage && (
        <Card
          title="Pemakaian token"
          label="Pemakaian"
          description={`Pemakaian bulan ini: platform ${quotaText(usage.platform_tokens, usage.platform_quota)} · kunci sendiri ${quotaText(usage.byok_tokens, usage.byok_budget)}`}
        >
          <div className="d-grid2" aria-hidden="true">
            <Meter label="AI platform" used={usage.platform_tokens} limit={usage.platform_quota} />
            <Meter label="Kunci API sendiri" used={usage.byok_tokens} limit={usage.byok_budget} />
          </div>
        </Card>
      )}

      <Card title="Penyedia AI" description={providers.length > 0 ? `${providers.length} penyedia terdaftar${dupes > 0 ? ` · ${dupes} berkonfigurasi sama, pertimbangkan menghapus yang tidak dipakai` : ""}.` : "Sumber model bahasa yang dipakai chatbot."} label="Penyedia AI">
        {providers.length === 0 ? (
          <EmptyState icon={Brain} title="Belum ada penyedia AI">Tambahkan satu di bawah untuk mulai memakai Asisten AI.</EmptyState>
        ) : (
          <ul className="d-list" style={{ marginBottom: 16 }}>
            {providers.map((p) => (
              <li key={p.id}>
                <div className="d-list-main">
                  <div className="d-row">
                    <strong>{p.name}</strong>
                    {cfg.primary_provider_id === p.id && <Badge tone="brand">Utama</Badge>}
                    {cfg.fallback_provider_id === p.id && <Badge tone="info">Cadangan</Badge>}
                  </div>
                  <span className="d-muted">{p.source === "platform" ? "AI platform" : `${p.provider_type} · ${p.base_url}`} · {p.model} · {p.status === "active" ? "aktif" : "nonaktif"}{p.key_hint ? ` · kunci ${p.key_hint}` : ""}</span>
                </div>
                {manageProv && (
                  <div className="d-row">
                    {p.source === "byok" && <RotateKey onSave={(k) => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { api_key: k } }), "Kunci diganti.")} />}
                    <button className="d-btn d-btn-sm" onClick={() => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { active: p.status !== "active" } }), "Penyedia diperbarui.")}>{p.status === "active" ? "Nonaktifkan" : "Aktifkan"}</button>
                    <button className="d-btn d-btn-sm d-btn-ghost" aria-label={`Hapus penyedia ${p.name}`} onClick={async () => {
                      if (await confirm({ title: `Hapus penyedia “${p.name}”?`, body: "Chatbot berhenti memakai penyedia ini. Kunci API yang tersimpan ikut dihapus.", confirmLabel: "Hapus", danger: true })) void run(() => api("DELETE", `${base}/providers/${p.id}`), "Penyedia dihapus.");
                    }}><Trash weight="bold" aria-hidden="true" /></button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {manageProv && <AddProvider view={view} platformBlocked={platformBlocked} subscriptionHref={`/subscription?tenant=${ctx.tenant.id}`} first={providers.length === 0} onAdd={(body) => run(() => api("POST", `${base}/providers`, { json: body }), "Penyedia ditambahkan.")} />}
      </Card>

      <Card title="Pengaturan chatbot" label="Pengaturan chatbot" description="Cara chatbot menyapa, kapan menyerahkan ke staf, dan topik yang boleh dijawab.">
        <form onSubmit={(e) => { e.preventDefault(); void saveCfg(); }}>
          <fieldset disabled={!manage} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
            <label className="d-switch">
              <input type="checkbox" checked={cfg.enabled} onChange={(e) => setCfg({ ...cfg, enabled: e.target.checked })} />
              <span><strong>Aktifkan chatbot untuk pelanggan</strong><span className="d-muted">Tombol chat muncul di website toko.</span></span>
            </label>
            <div className="d-field" role="radiogroup" aria-label="Mode">
              <span>Mode</span>
              <div className="d-choice-grid">
                {MODES.map((m) => (
                  <label key={m.value} className="d-choice">
                    <input type="radio" name="mode" value={m.value} checked={cfg.mode === m.value} onChange={() => setCfg({ ...cfg, mode: m.value })} />
                    <span><strong>{m.title}</strong><span className="d-muted">{m.desc}</span></span>
                  </label>
                ))}
              </div>
            </div>
            <div className="d-grid">
              {(["primary_provider_id", "fallback_provider_id"] as const).map((f) => (
                <label key={f} className="d-field"><span>{f === "primary_provider_id" ? "Penyedia utama" : "Penyedia cadangan"}</span>
                  <select value={cfg[f] ?? ""} onChange={(e) => setCfg({ ...cfg, [f]: e.target.value || null })}>
                    <option value="">{f === "primary_provider_id" ? "Pilih penyedia…" : "Tanpa cadangan"}</option>
                    {active.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                  </select>
                </label>
              ))}
            </div>
            <label className="d-field"><span>Panduan untuk AI <span className="d-opt">(gaya bahasa, info toko)</span></span><textarea rows={4} maxLength={4000} placeholder="mis. Jawab ramah dan singkat, panggil pelanggan “kak”." value={cfg.system_prompt} onChange={(e) => setCfg({ ...cfg, system_prompt: e.target.value })} /></label>
            <div className="d-grid">
              <label className="d-field"><span>Pesan sambutan</span><input maxLength={500} value={cfg.welcome_message} onChange={(e) => setCfg({ ...cfg, welcome_message: e.target.value })} /></label>
              <label className="d-field"><span>Pesan saat dialihkan ke staf</span><input maxLength={500} value={cfg.fallback_message} onChange={(e) => setCfg({ ...cfg, fallback_message: e.target.value })} /></label>
              <label className="d-field"><span>Topik yang dilayani (pisahkan koma)</span><input defaultValue={cfg.allowed_topics.join(", ")} placeholder="produk, harga, pengiriman" onBlur={(e) => setCfg({ ...cfg, allowed_topics: topics(e.target.value) })} /></label>
              <label className="d-field"><span>Topik terlarang — langsung ke staf (pisahkan koma)</span><input defaultValue={cfg.restricted_topics.join(", ")} placeholder="komplain, refund" onBlur={(e) => setCfg({ ...cfg, restricted_topics: topics(e.target.value) })} /></label>
            </div>
            <h3>Lanjutan</h3>
            <div className="d-grid">
              <label className="d-field"><span>Temperature (0–2)</span><input type="number" step="0.1" min={0} max={2} value={cfg.temperature} onChange={(e) => setCfg({ ...cfg, temperature: Number(e.target.value) })} /><span className="d-hint">Rendah = konsisten, tinggi = kreatif.</span></label>
              <label className="d-field"><span>Maks token jawaban</span><input type="number" min={16} max={4096} value={cfg.max_output_tokens} onChange={(e) => setCfg({ ...cfg, max_output_tokens: Number(e.target.value) })} /></label>
              <label className="d-field"><span>Batas waktu (ms)</span><input type="number" min={1000} max={60000} step={1000} value={cfg.timeout_ms} onChange={(e) => setCfg({ ...cfg, timeout_ms: Number(e.target.value) })} /></label>
              <label className="d-field"><span>Batas token bulanan untuk kunci sendiri <span className="d-opt">(opsional)</span></span><input type="number" min={1} value={cfg.monthly_token_budget ?? ""} onChange={(e) => setCfg({ ...cfg, monthly_token_budget: e.target.value ? Number(e.target.value) : null })} /></label>
            </div>
            <Alert kind="error">{cfgErr}</Alert>
            <BusyButton busy={saving}>Simpan pengaturan</BusyButton>
          </fieldset>
        </form>
      </Card>

      <Card title="Kemampuan" description="Data apa saja yang boleh dipakai chatbot saat menjawab." label="Kemampuan">
        <div className="d-choice-grid">
          {view.tools.map((t) => (
            <label key={t.name} className="d-switch" style={{ margin: 0 }}>
              <input type="checkbox" disabled={!manage} checked={t.enabled} onChange={(e) => run(() => api("PUT", `${base}/tools`, { json: { tools: [{ name: t.name, enabled: e.target.checked }] } }), "Kemampuan diperbarui.")} />
              <span><strong>{TOOLS[t.name] ?? t.name}</strong></span>
            </label>
          ))}
        </div>
      </Card>

      <Card title="Basis pengetahuan" description="FAQ, kebijakan retur, info pengiriman — dipakai AI untuk menjawab dengan akurat." label="Basis pengetahuan">
        {docs.length === 0 ? <EmptyState icon={BookOpen} title="Belum ada dokumen">Tambahkan FAQ, kebijakan retur, info pengiriman, dll.</EmptyState> : (
          <ul className="d-list" style={{ marginBottom: 16 }}>
            {docs.map((d) => (
              <li key={d.id}>
                <div className="d-list-main">
                  <strong>{d.title}</strong>
                  <span className="d-muted">{d.status === "indexed" ? `siap (${d.chunk_count} bagian)` : d.status === "pending" ? "sedang diproses" : `gagal${d.error_code ? `: ${d.error_code}` : ""}`}</span>
                </div>
                <Badge tone={d.status === "indexed" ? "ok" : d.status === "pending" ? "info" : "danger"} plain>{d.status === "indexed" ? "Siap" : d.status === "pending" ? "Diproses" : "Gagal"}</Badge>
                {manage && (
                  <div className="d-row">
                    <button className="d-btn d-btn-sm" onClick={() => run(() => api("POST", `${base}/knowledge/${d.id}/reindex`), "Dijadwalkan ulang.")}>Proses ulang</button>
                    <button className="d-btn d-btn-sm d-btn-ghost" aria-label={`Hapus dokumen ${d.title}`} onClick={async () => {
                      if (await confirm({ title: `Hapus dokumen “${d.title}”?`, confirmLabel: "Hapus", danger: true })) void run(() => api("DELETE", `${base}/knowledge/${d.id}`), "Dokumen dihapus.");
                    }}><Trash weight="bold" aria-hidden="true" /></button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {manage && <AddDoc onAdd={(title, body) => run(() => api("POST", `${base}/knowledge`, { json: { title, body } }), "Dokumen ditambahkan; akan diproses di latar.")} />}
      </Card>

      {manage && <TestConsole url={`${base}/test`} onDone={() => { void load(); }} />}
    </>
  );
}

function RotateKey({ onSave }: { onSave: (k: string) => void }) {
  const [open, setOpen] = useState(false);
  const [k, setK] = useState("");
  if (!open) return <button className="d-btn d-btn-sm" onClick={() => setOpen(true)}>Ganti kunci</button>;
  return (
    <form className="d-row" onSubmit={(e) => { e.preventDefault(); onSave(k); setK(""); setOpen(false); }}>
      <input aria-label="Kunci API baru" type="password" autoComplete="off" placeholder="Kunci baru" value={k} onChange={(e) => setK(e.target.value)} style={{ width: 180 }} autoFocus />
      <button className="d-btn d-btn-sm d-btn-primary" disabled={k.length < 8}>Simpan</button>
      <button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => setOpen(false)}>Batal</button>
    </form>
  );
}

function AddProvider({ view, onAdd, first, platformBlocked, subscriptionHref }: { view: ConfigView; onAdd: (b: Record<string, string>) => void; first: boolean; platformBlocked: boolean; subscriptionHref: string }) {
  const [source, setSource] = useState(view.platform.available && !platformBlocked ? "platform" : "byok");
  const [f, setF] = useState({ name: "", provider_type: "openai", base_url: "https://api.openai.com/v1", model: view.platform.models[0] ?? "", api_key: "" });
  function submit(e: FormEvent) {
    e.preventDefault();
    onAdd(source === "platform"
      ? { name: f.name, source, model: f.model }
      : { name: f.name, source, provider_type: f.provider_type, base_url: f.base_url, model: f.model, api_key: f.api_key });
    setF({ ...f, name: "", api_key: "" });
  }
  return (
    <details className="d-details d-subpanel" open={first}>
      <summary><Plus weight="bold" aria-hidden="true" /> Penyedia baru</summary>
      <form onSubmit={submit}>
        <div className="d-seg" role="group" aria-label="Sumber" style={{ marginBottom: 16 }}>
          {view.platform.available && <button type="button" aria-pressed={source === "platform"} onClick={() => setSource("platform")}>AI platform (kuota bulanan)</button>}
          <button type="button" aria-pressed={source === "byok"} onClick={() => setSource("byok")}>Kunci API sendiri</button>
        </div>
        {source === "platform" && platformBlocked && (
          <p className="d-hint" role="note" style={{ marginTop: -8, marginBottom: 16 }}>
            Paket Anda belum menyertakan token AI platform, jadi chatbot dengan penyedia ini tidak akan menjawab.{" "}
            <a href={subscriptionHref}>Lihat paket Pro</a> atau pilih “Kunci API sendiri”.
          </p>
        )}
        <div className="d-grid">
          <label className="d-field"><span>Nama</span><input required maxLength={60} placeholder="mis. AI Utama" value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></label>
          {source === "platform" ? (
            <label className="d-field"><span>Model</span>
              <select value={f.model} onChange={(e) => setF({ ...f, model: e.target.value })}>{view.platform.models.map((m) => <option key={m}>{m}</option>)}</select>
            </label>
          ) : (
            <>
              <label className="d-field"><span>Jenis</span>
                <select value={f.provider_type} onChange={(e) => setF({ ...f, provider_type: e.target.value })}>
                  {["openai", "openrouter", "groq", "together", "custom"].map((t) => <option key={t}>{t}</option>)}
                </select>
              </label>
              <label className="d-field"><span>Model</span><input required value={f.model} placeholder="mis. gpt-4o-mini" onChange={(e) => setF({ ...f, model: e.target.value })} /></label>
              <label className="d-field"><span>Alamat endpoint (HTTPS)</span><input required type="url" value={f.base_url} onChange={(e) => setF({ ...f, base_url: e.target.value })} /></label>
            </>
          )}
        </div>
        {source !== "platform" && (
          <label className="d-field"><span>Kunci API</span><input required type="password" autoComplete="off" minLength={8} value={f.api_key} onChange={(e) => setF({ ...f, api_key: e.target.value })} /><span className="d-hint">Disimpan terenkripsi dan tidak ditampilkan lagi.</span></label>
        )}
        <button className="d-btn d-btn-primary">Tambah penyedia</button>
      </form>
    </details>
  );
}

function AddDoc({ onAdd }: { onAdd: (t: string, b: string) => void }) {
  const [t, setT] = useState("");
  const [b, setB] = useState("");
  return (
    <details className="d-details d-subpanel" open>
      <summary><Plus weight="bold" aria-hidden="true" /> Dokumen baru</summary>
      <label className="d-field"><span>Judul dokumen</span><input maxLength={200} placeholder="mis. Kebijakan pengiriman" value={t} onChange={(e) => setT(e.target.value)} /></label>
      <label className="d-field"><span>Isi</span><textarea rows={5} maxLength={50000} value={b} onChange={(e) => setB(e.target.value)} /></label>
      <button className="d-btn" disabled={!t.trim() || !b.trim()} onClick={() => { onAdd(t, b); setT(""); setB(""); }}>Tambah dokumen</button>
    </details>
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
    <Card title="Uji coba" description="Coba pertanyaan pelanggan sebelum chatbot diaktifkan. Dihitung ke pemakaian token." label="Uji coba">
      <form className="d-row" style={{ flexWrap: "nowrap" }} onSubmit={(e) => { e.preventDefault(); if (q.trim()) void ask(); }}>
        <input aria-label="Pertanyaan uji" placeholder="mis. Jam berapa toko buka?" maxLength={2000} value={q} onChange={(e) => setQ(e.target.value)} />
        <BusyButton busy={busy} disabled={!q.trim()} busyLabel="Menunggu…">Tanya</BusyButton>
      </form>
      <Alert kind="error">{err}</Alert>
      {r && (
        <div aria-label="Hasil uji" className="cv-timeline" style={{ marginTop: 16 }}>
          <div className="cv-msg cv-customer"><div className="cv-body">{q}</div></div>
          <div className="cv-msg cv-ai">
            <div className="cv-head"><Robot weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> <strong>{DECISION[r.decision] ?? r.decision}</strong>{r.status !== "ok" && <span className="d-muted"> ({r.status}{r.error_code ? `: ${r.error_code}` : ""})</span>}</div>
            {r.reply && <div className="cv-body">{r.reply}</div>}
            <div className="cv-run d-muted">
              {r.tool_calls.length > 0 && <>Tool: {r.tool_calls.map((c) => `${TOOLS[c.name] ?? c.name} (${c.status})`).join(", ")} · </>}
              {r.prompt_tokens + r.completion_tokens} token
            </div>
          </div>
        </div>
      )}
    </Card>
  );
}
