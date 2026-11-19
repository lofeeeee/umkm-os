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
import { t as tx, tmap } from "@/i18n";
import { Tr } from "@/i18n/react";

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
  { value: "hybrid", get title() { return tx("Hybrid (disarankan)"); }, get desc() { return tx("AI menjawab pertanyaan yang jelas; sisanya diteruskan ke staf."); } },
  { value: "auto", get title() { return tx("Otomatis"); }, get desc() { return tx("AI menjawab semua pertanyaan langsung."); } },
  { value: "assisted", get title() { return tx("Dibantu"); }, get desc() { return tx("AI menyiapkan draf; staf memeriksa lalu mengirim."); } },
  { value: "human_only", get title() { return tx("Hanya staf (tanpa AI)"); }, get desc() { return tx("Tanpa penyedia AI. Semua chat masuk antrean staf. Cocok untuk mulai menjual."); } },
];
const TOOLS: Record<string, string> = tmap({
  search_products: "Cari produk", get_product_detail: "Detail produk", check_stock: "Cek stok",
  get_business_hours: "Jam buka", get_business_location: "Lokasi toko", search_faq: "Cari FAQ / pengetahuan",
  get_customer_orders: "Pesanan pelanggan (terverifikasi)", get_order_status: "Status pesanan (terverifikasi)",
  create_order_draft: "Draf pesanan (tanpa pembayaran)", request_human_support: "Alihkan ke staf",
});
const DECISION: Record<string, string> = tmap({
  send_auto: "Dikirim otomatis", draft_for_admin: "Draf untuk staf", handover: "Dialihkan ke staf", human_queue: "Menunggu staf",
});
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
      {limit === 0 ? <span className="d-hint">{tx("Tidak termasuk paket Anda")}</span>
        : limit ? <div className="d-progress"><span style={{ transform: `scaleX(${pct})`, background: pct > 0.9 ? "var(--danger)" : pct > 0.7 ? "var(--warn)" : undefined }} /></div>
        : <span className="d-hint">{tx("Tanpa batas")}</span>}
    </div>
  );
}

/** Teks kuota untuk pembaca layar & deskripsi kartu. */
const quotaText = (used: number, limit: number | null) =>
  tx("{0}{1} token", angka(used), limit === 0 ? tx(" (tidak termasuk paket)") : limit ? ` / ${angka(limit)}` : "");

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
    try { await f(); await load(); toast.ok(ok); } catch (e) { setErr(describe(e)); }
  }
  const manage = ctx.can("ai_config.manage");
  const manageProv = ctx.can("ai_provider.manage");
  if (!view || !cfg) return <><Alert kind="error">{err}</Alert>{!err && <PageSkeleton />}</>;
  const active = providers.filter((p) => p.status === "active");
  /** "Hanya staf": chat tetap jalan (antrean staf) tanpa penyedia AI — server mengizinkan ini
   *  (`ai.rs`: `enabled && mode != human_only` baru mewajibkan penyedia). */
  const humanOnly = cfg.mode === "human_only";
  /** Chat siap menerima pelanggan: aktif dan (mode staf-saja ATAU ada penyedia utama). */
  const chatReady = cfg.enabled && (humanOnly || !!cfg.primary_provider_id);
  async function saveCfg() {
    if (!cfg) return;
    setSaving(true);
    setCfgErr(null);
    // Mode "Hanya staf" tidak memanggil penyedia AI. Mengirim penyedia lama akan membuat
    // penyedia tampak "dipakai chatbot aktif" padahal tidak pernah dipanggil.
    const body = cfg.mode === "human_only"
      ? { ...cfg, enabled: true, primary_provider_id: null, fallback_provider_id: null }
      : cfg;
    // Toast setelah data dimuat ulang: form sudah mencerminkan nilai tersimpan, dan muat ulang yang
    // terlambat tidak menimpa isian yang baru diketik pengguna.
    try { await api("PUT", `${base}/config`, { json: body }); await load(); toast.ok(tx("Pengaturan tersimpan.")); } catch (e) { setCfgErr(describe(e)); } finally { setSaving(false); }
  }
  return (
    <>
      <PageHeader
        title={tx("Asisten AI")}
        description={tx("Chatbot yang menjawab pelanggan di website toko memakai data produk, jam buka, dan basis pengetahuan Anda.")}
        actions={<Badge tone={chatReady ? "ok" : "neutral"}>{chatReady ? (humanOnly ? tx("Chat aktif (staf)") : tx("Chatbot aktif")) : tx("Chatbot nonaktif")}</Badge>}
      />
      <Alert kind="error">{err}</Alert>
      {!chatReady && !humanOnly && (
        <Alert kind="info">
          {tx("Langkah pertama: pilih mode")}{" "}<strong>{tx("Hanya staf")}</strong>{" "}{tx("bila chat dilayani manual tanpa AI, atau tambahkan penyedia AI lalu pilih sebagai penyedia utama.")}
        </Alert>
      )}
      {platformBlocked && providers.some((p) => p.source === "platform" && p.id === cfg.primary_provider_id) && (
        <Alert kind="error">
          {tx("Penyedia utama memakai AI platform, tetapi paket Anda tidak menyertakan token AI platform. Chatbot akan meneruskan semua pertanyaan ke staf.")}{" "}<a href={`/subscription?tenant=${ctx.tenant.id}`}>{tx("Tingkatkan ke Pro")}</a>{" "}{tx("atau tambahkan penyedia dengan kunci API sendiri.")}
        </Alert>
      )}

      {usage && (
        <Card
          title={tx("Pemakaian token")}
          label={tx("Pemakaian")}
          description={tx("Pemakaian bulan ini: platform {0} · kunci sendiri {1}", quotaText(usage.platform_tokens, usage.platform_quota), quotaText(usage.byok_tokens, usage.byok_budget))}
        >
          <div className="d-grid2" aria-hidden="true">
            <Meter label={tx("AI platform")} used={usage.platform_tokens} limit={usage.platform_quota} />
            <Meter label={tx("Kunci API sendiri")} used={usage.byok_tokens} limit={usage.byok_budget} />
          </div>
        </Card>
      )}

      <Card title={tx("Penyedia AI")} description={providers.length > 0 ? tx("{0} penyedia terdaftar{1}.", providers.length, dupes > 0 ? tx(" · {0} berkonfigurasi sama, pertimbangkan menghapus yang tidak dipakai", dupes) : "") : tx("Sumber model bahasa yang dipakai chatbot.")} label={tx("Penyedia AI")}>
        {providers.length === 0 ? (
          <EmptyState icon={Brain} title={tx("Belum ada penyedia AI")}>{tx("Tambahkan satu di bawah untuk mulai memakai Asisten AI.")}</EmptyState>
        ) : (
          <ul className="d-list u-mb-4">
            {providers.map((p) => (
              <li key={p.id}>
                <div className="d-list-main">
                  <div className="d-row">
                    <strong>{p.name}</strong>
                    {cfg.primary_provider_id === p.id && <Badge tone="brand">{tx("Utama")}</Badge>}
                    {cfg.fallback_provider_id === p.id && <Badge tone="info">{tx("Cadangan")}</Badge>}
                  </div>
                  <span className="d-muted">{p.source === "platform" ? tx("AI platform") : tx("{0} · {1}", p.provider_type, p.base_url)} · {p.model} · {p.status === "active" ? "aktif" : "nonaktif"}{p.key_hint ? tx(" · kunci {0}", p.key_hint) : ""}</span>
                </div>
                {manageProv && (
                  <div className="d-row">
                    {p.source === "byok" && <RotateKey onSave={(k) => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { api_key: k } }), tx("Kunci diganti."))} />}
                    <button type="button" className="d-btn d-btn-sm" onClick={() => run(() => api("PATCH", `${base}/providers/${p.id}`, { json: { active: p.status !== "active" } }), tx("Penyedia diperbarui."))}>{p.status === "active" ? tx("Nonaktifkan") : tx("Aktifkan")}</button>
                    <button type="button" className="d-btn d-btn-sm d-btn-ghost" aria-label={tx("Hapus penyedia {0}", p.name)} onClick={async () => {
                      if (await confirm({ title: tx("Hapus penyedia “{0}”?", p.name), body: tx("Chatbot berhenti memakai penyedia ini. Kunci API yang tersimpan ikut dihapus."), confirmLabel: tx("Hapus"), danger: true })) void run(() => api("DELETE", `${base}/providers/${p.id}`), tx("Penyedia dihapus."));
                    }}><Trash weight="bold" aria-hidden="true" /></button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {manageProv && <AddProvider view={view} platformBlocked={platformBlocked} subscriptionHref={`/subscription?tenant=${ctx.tenant.id}`} first={providers.length === 0} onAdd={(body) => run(() => api("POST", `${base}/providers`, { json: body }), tx("Penyedia ditambahkan."))} />}
      </Card>

      <Card title={tx("Pengaturan chatbot")} label={tx("Pengaturan chatbot")} description={tx("Cara chatbot menyapa, kapan menyerahkan ke staf, dan topik yang boleh dijawab.")}>
        <form onSubmit={(e) => { e.preventDefault(); void saveCfg(); }}>
          <fieldset disabled={!manage} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
            <label className="d-switch">
              <input type="checkbox" checked={cfg.enabled} onChange={(e) => setCfg({ ...cfg, enabled: e.target.checked })} />
              <span><strong>{tx("Aktifkan chatbot untuk pelanggan")}</strong><span className="d-muted">{tx("Tombol chat muncul di website toko.")}</span></span>
            </label>
            <div className="d-field" role="radiogroup" aria-label={tx("Mode")}>
              <span>{tx("Mode")}</span>
              <div className="d-choice-grid">
                {MODES.map((m) => (
                  <label key={m.value} className="d-choice">
                    <input type="radio" name="mode" value={m.value} checked={cfg.mode === m.value} onChange={() => setCfg({ ...cfg, mode: m.value, enabled: m.value === "human_only" ? true : cfg.enabled })} />
                    <span><strong>{m.title}</strong><span className="d-muted">{m.desc}</span></span>
                  </label>
                ))}
              </div>
            </div>
            <div className="d-grid">
              {(["primary_provider_id", "fallback_provider_id"] as const).map((f) => (
                <label key={f} className="d-field"><span>{f === "primary_provider_id" ? tx("Penyedia utama") : tx("Penyedia cadangan")}</span>
                  <select value={cfg[f] ?? ""} disabled={humanOnly} onChange={(e) => setCfg({ ...cfg, [f]: e.target.value || null })}>
                    <option value="">{f === "primary_provider_id" ? (humanOnly ? tx("Tidak dipakai (hanya staf)") : tx("Pilih penyedia…")) : tx("Tanpa cadangan")}</option>
                    {active.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                  </select>
                </label>
              ))}
            </div>
            {humanOnly && (
              <p className="d-hint">{tx("Mode ini tidak memanggil AI, jadi penyedia tidak diperlukan. Pelanggan tetap bisa memulai chat dan pesan masuk ke menu")}{" "}<strong>{tx("Percakapan")}</strong>.</p>
            )}
            <label className="d-field"><span>{tx("Panduan untuk AI")}{" "}<span className="d-opt">{tx("(gaya bahasa, info toko)")}</span></span><textarea rows={4} maxLength={4000} placeholder={tx("mis. Jawab ramah dan singkat, panggil pelanggan “kak”.")} value={cfg.system_prompt} onChange={(e) => setCfg({ ...cfg, system_prompt: e.target.value })} /></label>
            <div className="d-grid">
              <label className="d-field"><span>{tx("Pesan sambutan")}</span><input maxLength={500} value={cfg.welcome_message} onChange={(e) => setCfg({ ...cfg, welcome_message: e.target.value })} /></label>
              <label className="d-field"><span>{tx("Pesan saat dialihkan ke staf")}</span><input maxLength={500} value={cfg.fallback_message} onChange={(e) => setCfg({ ...cfg, fallback_message: e.target.value })} /></label>
              <label className="d-field"><span>{tx("Topik yang dilayani (pisahkan koma)")}</span><input defaultValue={cfg.allowed_topics.join(", ")} placeholder={tx("produk, harga, pengiriman")} onBlur={(e) => setCfg({ ...cfg, allowed_topics: topics(e.target.value) })} /></label>
              <label className="d-field"><span>{tx("Topik terlarang (langsung ke staf, pisahkan koma)")}</span><input defaultValue={cfg.restricted_topics.join(", ")} placeholder={tx("komplain, pengembalian dana")} onBlur={(e) => setCfg({ ...cfg, restricted_topics: topics(e.target.value) })} /></label>
            </div>
            <h3>{tx("Lanjutan")}</h3>
            <div className="d-grid">
              <label className="d-field"><span>{tx("Temperature (0–2)")}</span><input type="number" step="0.1" min={0} max={2} value={cfg.temperature} onChange={(e) => setCfg({ ...cfg, temperature: Number(e.target.value) })} /><span className="d-hint">{tx("Rendah = konsisten, tinggi = kreatif.")}</span></label>
              <label className="d-field"><span>{tx("Maks token jawaban")}</span><input type="number" min={16} max={4096} value={cfg.max_output_tokens} onChange={(e) => setCfg({ ...cfg, max_output_tokens: Number(e.target.value) })} /></label>
              <label className="d-field"><span>{tx("Batas waktu (ms)")}</span><input type="number" min={1000} max={60000} step={1000} value={cfg.timeout_ms} onChange={(e) => setCfg({ ...cfg, timeout_ms: Number(e.target.value) })} /></label>
              <label className="d-field"><span>{tx("Batas token bulanan untuk kunci sendiri")}{" "}<span className="d-opt">{tx("(opsional)")}</span></span><input type="number" min={1} value={cfg.monthly_token_budget ?? ""} onChange={(e) => setCfg({ ...cfg, monthly_token_budget: e.target.value ? Number(e.target.value) : null })} /></label>
            </div>
            <Alert kind="error">{cfgErr}</Alert>
            <BusyButton busy={saving}>{tx("Simpan pengaturan")}</BusyButton>
          </fieldset>
        </form>
      </Card>

      <Card title={tx("Kemampuan")} description={tx("Data apa saja yang boleh dipakai chatbot saat menjawab.")} label={tx("Kemampuan")}>
        <div className="d-choice-grid">
          {view.tools.map((t) => (
            <label key={t.name} className="d-switch u-m-0">
              <input type="checkbox" disabled={!manage} checked={t.enabled} onChange={(e) => run(() => api("PUT", `${base}/tools`, { json: { tools: [{ name: t.name, enabled: e.target.checked }] } }), tx("Kemampuan diperbarui."))} />
              <span><strong>{TOOLS[t.name] ?? t.name}</strong></span>
            </label>
          ))}
        </div>
      </Card>

      <Card title={tx("Basis pengetahuan")} description={tx("FAQ, kebijakan retur, info pengiriman: dipakai AI untuk menjawab dengan akurat.")} label={tx("Basis pengetahuan")}>
        {docs.length === 0 ? (
          <EmptyState icon={BookOpen} title={tx("Belum ada dokumen")}
            action={manage ? <button type="button" className="d-btn" onClick={() => document.getElementById("kb-title")?.focus()}>{tx("Mulai isi basis pengetahuan")}</button> : undefined}>
            {tx("Tambahkan FAQ, kebijakan retur, info pengiriman, dll.")}
          </EmptyState>
        ) : (
          <ul className="d-list u-mb-4">
            {docs.map((d) => (
              <li key={d.id}>
                <div className="d-list-main">
                  <strong>{d.title}</strong>
                  <span className="d-muted">{d.status === "indexed" ? tx("siap ({0} bagian)", d.chunk_count) : d.status === "pending" ? tx("sedang diproses") : tx("gagal{0}", d.error_code ? `: ${d.error_code}` : "")}</span>
                </div>
                <Badge tone={d.status === "indexed" ? "ok" : d.status === "pending" ? "info" : "danger"} plain>{d.status === "indexed" ? tx("Siap") : d.status === "pending" ? tx("Diproses") : tx("Gagal")}</Badge>
                {manage && (
                  <div className="d-row">
                    <button type="button" className="d-btn d-btn-sm" onClick={() => run(() => api("POST", `${base}/knowledge/${d.id}/reindex`), tx("Dijadwalkan ulang."))}>{tx("Proses ulang")}</button>
                    <button type="button" className="d-btn d-btn-sm d-btn-ghost" aria-label={tx("Hapus dokumen {0}", d.title)} onClick={async () => {
                      if (await confirm({ title: tx("Hapus dokumen “{0}”?", d.title), confirmLabel: tx("Hapus"), danger: true })) void run(() => api("DELETE", `${base}/knowledge/${d.id}`), tx("Dokumen dihapus."));
                    }}><Trash weight="bold" aria-hidden="true" /></button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {manage && <AddDoc onAdd={(title, body) => run(() => api("POST", `${base}/knowledge`, { json: { title, body } }), tx("Dokumen ditambahkan; akan diproses di latar."))} />}
      </Card>

      {manage && <TestConsole url={`${base}/test`} onDone={() => { void load(); }} />}
    </>
  );
}

function RotateKey({ onSave }: { onSave: (k: string) => void }) {
  const [open, setOpen] = useState(false);
  const [k, setK] = useState("");
  if (!open) return <button type="button" className="d-btn d-btn-sm" onClick={() => setOpen(true)}>{tx("Ganti kunci")}</button>;
  return (
    <form className="d-row" onSubmit={(e) => { e.preventDefault(); onSave(k); setK(""); setOpen(false); }}>
      <input aria-label={tx("Kunci API baru")} type="password" autoComplete="off" placeholder={tx("Kunci baru")} value={k} onChange={(e) => setK(e.target.value)} style={{ width: 180 }} autoFocus />
      <button type="submit" className="d-btn d-btn-sm d-btn-primary" disabled={k.length < 8}>{tx("Simpan")}</button>
      <button type="button" className="d-btn d-btn-sm d-btn-ghost" onClick={() => setOpen(false)}>{tx("Batal")}</button>
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
      <summary><Plus weight="bold" aria-hidden="true" />{" "}{tx("Penyedia baru")}</summary>
      <form onSubmit={submit}>
        <div className="d-seg u-mb-4" role="group" aria-label={tx("Sumber")}>
          {view.platform.available && <button type="button" aria-pressed={source === "platform"} onClick={() => setSource("platform")}>{tx("AI platform (kuota bulanan)")}</button>}
          <button type="button" aria-pressed={source === "byok"} onClick={() => setSource("byok")}>{tx("Kunci API sendiri")}</button>
        </div>
        {source === "platform" && platformBlocked && (
          <p className="d-hint" role="note" style={{ marginTop: -8, marginBottom: 16 }}>
            <Tr s="Paket Anda belum menyertakan token AI platform, jadi chatbot dengan penyedia ini tidak akan menjawab.{0}" a={[" "]} />
            <a href={subscriptionHref}>{tx("Lihat paket Pro")}</a>{" "}{tx("atau pilih “Kunci API sendiri”.")}
          </p>
        )}
        <div className="d-grid">
          <label className="d-field"><span>{tx("Nama")}</span><input required maxLength={60} placeholder={tx("mis. AI Utama")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></label>
          {source === "platform" ? (
            <label className="d-field"><span>{tx("Model")}</span>
              <select value={f.model} onChange={(e) => setF({ ...f, model: e.target.value })}>{view.platform.models.map((m) => <option key={m}>{m}</option>)}</select>
            </label>
          ) : (
            <>
              <label className="d-field"><span>{tx("Jenis")}</span>
                <select value={f.provider_type} onChange={(e) => setF({ ...f, provider_type: e.target.value })}>
                  {["openai", "openrouter", "groq", "together", "custom"].map((t) => <option key={t}>{t}</option>)}
                </select>
              </label>
              <label className="d-field"><span>{tx("Model")}</span><input required value={f.model} placeholder="mis. gpt-4o-mini" onChange={(e) => setF({ ...f, model: e.target.value })} /></label>
              <label className="d-field"><span>{tx("Alamat endpoint (HTTPS)")}</span><input required type="url" autoComplete="off" value={f.base_url} onChange={(e) => setF({ ...f, base_url: e.target.value })} /></label>
            </>
          )}
        </div>
        {source !== "platform" && (
          <label className="d-field"><span>{tx("Kunci API")}</span><input required type="password" autoComplete="off" minLength={8} value={f.api_key} onChange={(e) => setF({ ...f, api_key: e.target.value })} /><span className="d-hint">{tx("Disimpan terenkripsi dan tidak ditampilkan lagi.")}</span></label>
        )}
        <button type="submit" className="d-btn d-btn-primary">{tx("Tambah penyedia")}</button>
      </form>
    </details>
  );
}

function AddDoc({ onAdd }: { onAdd: (t: string, b: string) => void }) {
  const [t, setT] = useState("");
  const [b, setB] = useState("");
  return (
    <details className="d-details d-subpanel" open>
      <summary><Plus weight="bold" aria-hidden="true" />{" "}{tx("Dokumen baru")}</summary>
      <label className="d-field"><span>{tx("Judul dokumen")}</span><input id="kb-title" maxLength={200} placeholder={tx("mis. Kebijakan pengiriman")} value={t} onChange={(e) => setT(e.target.value)} /></label>
      <label className="d-field"><span>{tx("Isi")}</span><textarea rows={5} maxLength={50000} value={b} onChange={(e) => setB(e.target.value)} /></label>
      <button type="button" className="d-btn" disabled={!t.trim() || !b.trim()} onClick={() => { onAdd(t, b); setT(""); setB(""); }}>{tx("Tambah dokumen")}</button>
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
    <Card title={tx("Uji coba")} description={tx("Coba pertanyaan pelanggan sebelum chatbot diaktifkan. Dihitung ke pemakaian token.")} label={tx("Uji coba")}>
      <form className="d-row u-nowrap" onSubmit={(e) => { e.preventDefault(); if (q.trim()) void ask(); }}>
        <input aria-label={tx("Pertanyaan uji")} placeholder={tx("mis. Jam berapa toko buka?")} maxLength={2000} value={q} onChange={(e) => setQ(e.target.value)} />
        <BusyButton busy={busy} disabled={!q.trim()} busyLabel={tx("Menunggu…")}>{tx("Tanya")}</BusyButton>
      </form>
      <Alert kind="error">{err}</Alert>
      {r && (
        <div aria-label={tx("Hasil uji")} className="cv-timeline u-mt-4">
          <div className="cv-msg cv-customer"><div className="cv-body">{q}</div></div>
          <div className="cv-msg cv-ai">
            <div className="cv-head"><Robot weight="bold" aria-hidden="true" className="u-va-icon" /> <strong>{DECISION[r.decision] ?? r.decision}</strong>{r.status !== "ok" && <span className="d-muted"> ({r.status}{r.error_code ? tx(": {0}", r.error_code) : ""})</span>}</div>
            {r.reply && <div className="cv-body">{r.reply}</div>}
            <div className="cv-run d-muted">
              {r.tool_calls.length > 0 && <><Tr s="Tool: {0} ·" a={[r.tool_calls.map((c) => tx("{0} ({1})", TOOLS[c.name] ?? c.name, c.status)).join(", ")]} />{" "}</>}
              {r.prompt_tokens + r.completion_tokens} token
            </div>
          </div>
        </div>
      )}
    </Card>
  );
}
