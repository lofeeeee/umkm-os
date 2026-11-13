"use client";

import { useCallback, useEffect, useMemo, useState } from "react";

import { Assets, FieldInput, type Upload } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { api, ApiError, apiUrl, describe } from "@/lib/api";
import { FIELDS, FONT_PRESETS, SECTION_LABELS, defaultData, newSectionId } from "@/renderer/sections";
import { Site } from "@/renderer/Site";
import { SECTION_TYPES, type Business, type PageContent, type Section, type SectionType } from "@/renderer/types";

interface WebsiteView { template_key: string; template_version: number; status: string; pages: { slug: string; published_version: number | null }[] }
interface TemplateView { key: string; version: number; name: string; category: string }
interface DraftView { content: PageContent; revision: number; published_version: number | null; assets: Record<string, string> }
interface VersionView { version: number; created_at_unix: number; is_current: boolean }

const EMPTY_BUSINESS: Business = {
  profile: { name: "", socials: [], timezone: "Asia/Jakarta", pickup_available: false, delivery_available: false },
  addresses: [], hours: [], exceptions: [],
};

export default function WebsitePage() {
  return <WithTenant>{(ctx) => <Editor ctx={ctx} />}</WithTenant>;
}

function Editor({ ctx }: { ctx: TenantCtx }) {
  const base = `/api/v1/tenants/${ctx.tenant.id}`;
  const canEdit = ctx.can("website.edit");
  const canPublish = ctx.can("website.publish");
  const [site, setSite] = useState<WebsiteView | null | undefined>(undefined);
  const [content, setContent] = useState<PageContent | null>(null);
  const [revision, setRevision] = useState(0);
  const [published, setPublished] = useState<number | null>(null);
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({});
  const [uploads, setUploads] = useState<Upload[]>([]);
  const [business, setBusiness] = useState<Business>(EMPTY_BUSINESS);
  const [versions, setVersions] = useState<VersionView[]>([]);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [view, setView] = useState<"mobile" | "desktop">("desktop");
  const [viewing, setViewing] = useState<{ version: number; content: PageContent; assets: Record<string, string> } | null>(null);

  const loadDraft = useCallback(async () => {
    const d = await api<DraftView>("GET", `${base}/website/pages/home/draft`);
    setContent(d.content);
    setRevision(d.revision);
    setPublished(d.published_version);
    setAssetUrls((a) => ({ ...a, ...d.assets }));
    setDirty(false);
    setVersions(await api<VersionView[]>("GET", `${base}/website/pages/home/versions`));
  }, [base]);

  useEffect(() => {
    (async () => {
      const [w, ups, biz] = await Promise.all([
        api<WebsiteView>("GET", `${base}/website`).catch((e) => (e instanceof ApiError && e.status === 404 ? null : Promise.reject(e))),
        api<Upload[]>("GET", `${base}/uploads`),
        api<Business>("GET", `${base}/business/profile`).catch(() => EMPTY_BUSINESS),
      ]);
      setUploads(ups);
      setBusiness(biz);
      setSite(w);
      if (w) await loadDraft();
    })().catch((e) => setError(describe(e)));
  }, [base, loadDraft]);

  useEffect(() => {
    if (!dirty) return;
    const h = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", h);
    return () => window.removeEventListener("beforeunload", h);
  }, [dirty]);

  const assets = useMemo(() => {
    const map: Record<string, string> = { ...assetUrls };
    for (const u of uploads) map[u.ref] = u.url;
    return {
      uploads,
      canEdit,
      url: (r: string) => (map[r] ? apiUrl(map[r]!) : undefined),
      upload: async (file: File) => {
        try {
          const u = await api<Upload>("POST", `${base}/uploads`, { bytes: file });
          setUploads((x) => [u, ...x]);
          return u;
        } catch (e) {
          setError(describe(e));
          return null;
        }
      },
    };
  }, [assetUrls, uploads, canEdit, base]);

  function update(fn: (c: PageContent) => PageContent) {
    setContent((c) => (c ? fn(c) : c));
    setDirty(true);
    setNotice(null);
  }

  function updateSection(id: string, fn: (s: Section) => Section) {
    update((c) => ({ ...c, sections: c.sections.map((s) => (s.id === id ? fn(s) : s)) }));
  }

  function move(i: number, d: -1 | 1) {
    update((c) => {
      const s = [...c.sections];
      const j = i + d;
      if (j < 0 || j >= s.length) return c;
      [s[i], s[j]] = [s[j]!, s[i]!];
      return { ...c, sections: s };
    });
  }

  async function run(label: string, fn: () => Promise<void>) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await fn();
      setNotice(label);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.message.includes("Draft")) {
        setError("Draft sudah diubah di tempat lain. Muat ulang untuk melihat versi terbaru (perubahan Anda belum tersimpan).");
      } else setError(describe(e));
    } finally {
      setBusy(false);
    }
  }

  const save = () => run("Draft tersimpan.", async () => {
    const r = await api<{ revision: number }>("PUT", `${base}/website/pages/home/draft`, { json: { content, base_revision: revision } });
    setRevision(r.revision);
    setDirty(false);
  });

  const publish = () => run("Website diterbitkan.", async () => {
    if (dirty) {
      const r = await api<{ revision: number }>("PUT", `${base}/website/pages/home/draft`, { json: { content, base_revision: revision } });
      setRevision(r.revision);
      setDirty(false);
      await api("POST", `${base}/website/pages/home/publish`, { json: { revision: r.revision } });
    } else {
      await api("POST", `${base}/website/pages/home/publish`, { json: { revision } });
    }
    await loadDraft();
  });

  const rollback = (version: number) => {
    if (!window.confirm(`Tampilkan versi ${version} di situs publik? Draft Anda tidak berubah.`)) return;
    void run(`Publik kini menampilkan versi ${version}.`, async () => {
      await api("POST", `${base}/website/pages/home/rollback`, { json: { version } });
      await loadDraft();
    });
  };

  const showVersion = (version: number) => run("", async () => {
    const d = await api<DraftView>("GET", `${base}/website/pages/home/versions/${version}`);
    setViewing({ version, content: d.content, assets: d.assets });
  });

  if (site === undefined) return <p className="d-muted" aria-busy="true">Memuat…</p>;
  if (site === null) return <TemplatePicker base={base} canEdit={canEdit} business={business} onSelected={async () => { setSite(await api<WebsiteView>("GET", `${base}/website`)); await loadDraft(); }} />;
  if (!content) return <Alert kind="error">{error}</Alert>;

  const previewContent = viewing?.content ?? content;
  const previewAssets = viewing
    ? (r: string) => (viewing.assets[r] ? apiUrl(viewing.assets[r]!) : undefined)
    : assets.url;

  return (
    <Assets.Provider value={assets}>
      <div className="d-row" style={{ justifyContent: "space-between", marginBottom: 12 }}>
        <div>
          <h1 style={{ margin: 0, fontSize: "1.375rem" }}>Website</h1>
          <p className="d-muted" style={{ margin: 0, fontSize: "0.8125rem" }}>
            Templat {site.template_key} v{site.template_version} · {published ? `terbit versi ${published}` : "belum terbit"}
            {dirty ? " · perubahan belum disimpan" : ""}
          </p>
        </div>
        <div className="d-row">
          {ctx.tenant.primary_hostname && published && (
            <a className="d-btn" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noopener noreferrer">Lihat situs</a>
          )}
          <button className="d-btn" onClick={save} disabled={!canEdit || busy || !dirty}>Simpan draf</button>
          <button className="d-btn d-btn-primary" onClick={publish} disabled={!canPublish || busy}
            title={canPublish ? undefined : "Butuh izin website.publish"}>Terbitkan</button>
        </div>
      </div>
      <Alert kind="error">{error}</Alert>
      <Alert kind="ok">{notice}</Alert>

      <div className="e-layout">
        <div className="e-side">
          <div className="d-card">
            <h2>Tema</h2>
            <div className="d-grid2">
              <label className="d-field"><span>Warna utama</span>
                <input type="color" value={/^#[0-9a-fA-F]{6}$/.test(content.theme.primary) ? content.theme.primary : "#2563eb"} disabled={!canEdit}
                  onChange={(e) => update((c) => ({ ...c, theme: { ...c.theme, primary: e.target.value.toUpperCase() } }))} />
              </label>
              <label className="d-field"><span>Font</span>
                <select value={content.theme.font} disabled={!canEdit} onChange={(e) => update((c) => ({ ...c, theme: { ...c.theme, font: e.target.value } }))}>
                  {FONT_PRESETS.map((f) => <option key={f} value={f}>{f}</option>)}
                </select>
              </label>
            </div>
          </div>

          <div className="d-card">
            <h2>Bagian halaman ({content.sections.length}/30)</h2>
            {content.sections.map((s, i) => (
              <div key={s.id} className={`e-sec${s.enabled ? "" : " e-sec-off"}`} style={{ marginBottom: 8 }}>
                <div className="e-sec-head">
                  <button type="button" className="d-btn d-btn-sm" aria-expanded={open === s.id} onClick={() => setOpen(open === s.id ? null : s.id)}>
                    {open === s.id ? "▾" : "▸"}<span className="sr-only"> Ubah {SECTION_LABELS[s.type]}</span>
                  </button>
                  <span className="e-name">{SECTION_LABELS[s.type]}</span>
                  <label className="d-check" style={{ margin: 0 }} title="Tampilkan di publik">
                    <input type="checkbox" checked={s.enabled} disabled={!canEdit} aria-label={`Aktifkan ${SECTION_LABELS[s.type]}`}
                      onChange={(e) => updateSection(s.id, (x) => ({ ...x, enabled: e.target.checked }))} />
                  </label>
                  <button type="button" className="d-btn d-btn-sm" disabled={!canEdit || i === 0} onClick={() => move(i, -1)} aria-label="Naikkan">↑</button>
                  <button type="button" className="d-btn d-btn-sm" disabled={!canEdit || i === content.sections.length - 1} onClick={() => move(i, 1)} aria-label="Turunkan">↓</button>
                </div>
                {open === s.id && (
                  <div className="e-sec-body">
                    {FIELDS[s.type].map((f) => (
                      <FieldInput key={f.key} f={f} data={s.data} disabled={!canEdit} onChange={(d) => updateSection(s.id, (x) => ({ ...x, data: d }))} />
                    ))}
                    <button type="button" className="d-btn d-btn-sm d-btn-danger" disabled={!canEdit}
                      onClick={() => { if (window.confirm("Hapus bagian ini dari draf?")) update((c) => ({ ...c, sections: c.sections.filter((x) => x.id !== s.id) })); }}>
                      Hapus section
                    </button>
                  </div>
                )}
              </div>
            ))}
            <AddSection disabled={!canEdit || content.sections.length >= 30}
              onAdd={(type) => update((c) => {
                const id = newSectionId(type, c.sections);
                setOpen(id);
                return { ...c, sections: [...c.sections, { id, type, enabled: true, data: defaultData(type) }] };
              })} />
          </div>

          <div className="d-card">
            <h2>Riwayat terbit</h2>
            {versions.length === 0 && <p className="d-muted">Belum pernah diterbitkan.</p>}
            <ul className="e-versions">
              {versions.map((v) => (
                <li key={v.version}>
                  <span style={{ flex: 1 }}>
                    Versi {v.version} · {new Date(v.created_at_unix * 1000).toLocaleString("id-ID")}
                    {v.is_current && <strong> · aktif</strong>}
                  </span>
                  <button className="d-btn d-btn-sm" onClick={() => showVersion(v.version)}>Lihat</button>
                  {!v.is_current && <button className="d-btn d-btn-sm" disabled={!canPublish || busy} onClick={() => rollback(v.version)}>Kembalikan</button>}
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div>
          <div className="d-row" style={{ marginBottom: 8 }}>
            <span className="d-muted">Pratinjau{viewing ? ` versi ${viewing.version}` : " draf"}:</span>
            <button className="d-btn d-btn-sm" aria-pressed={view === "mobile"} onClick={() => setView("mobile")}>Ponsel</button>
            <button className="d-btn d-btn-sm" aria-pressed={view === "desktop"} onClick={() => setView("desktop")}>Layar lebar</button>
            {viewing && <button className="d-btn d-btn-sm" onClick={() => setViewing(null)}>Kembali ke draf</button>}
          </div>
          <div className="e-preview-wrap">
            <div className="e-preview" style={{ width: view === "mobile" ? 375 : "100%" }} data-testid="preview">
              <Site content={previewContent} business={business} assetUrl={previewAssets} preview={!viewing} />
            </div>
          </div>
        </div>
      </div>
    </Assets.Provider>
  );
}

function AddSection({ disabled, onAdd }: { disabled: boolean; onAdd: (t: SectionType) => void }) {
  const [type, setType] = useState<SectionType>("hero");
  return (
    <div className="d-row" style={{ marginTop: 8 }}>
      <select value={type} onChange={(e) => setType(e.target.value as SectionType)} disabled={disabled} aria-label="Tipe bagian baru">
        {SECTION_TYPES.map((t) => <option key={t} value={t}>{SECTION_LABELS[t]}</option>)}
      </select>
      <button type="button" className="d-btn d-btn-sm" disabled={disabled} onClick={() => onAdd(type)}>Tambah bagian</button>
    </div>
  );
}

function TemplatePicker({ base, canEdit, business, onSelected }: { base: string; canEdit: boolean; business: Business; onSelected: () => Promise<void> }) {
  const [templates, setTemplates] = useState<TemplateView[]>([]);
  const [preview, setPreview] = useState<{ key: string; content: PageContent } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api<TemplateView[]>("GET", `${base}/website/templates`).then(setTemplates).catch((e) => setError(describe(e)));
  }, [base]);
  async function choose(key: string) {
    setBusy(true);
    try {
      await api("PUT", `${base}/website`, { json: { template_key: key } });
      await onSelected();
    } catch (e) {
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <h1>Pilih template</h1>
      <Alert kind="error">{error}</Alert>
      <div className="e-tpl">
        {templates.map((t) => (
          <div className="d-card" key={t.key}>
            <h2>{t.name}</h2>
            <p className="d-muted">{t.category} · v{t.version}</p>
            <div className="d-row">
              <button className="d-btn" onClick={async () => setPreview({ key: t.key, content: (await api<{ content: PageContent }>("GET", `${base}/website/templates/${t.key}/preview`)).content })}>Preview</button>
              <button className="d-btn d-btn-primary" disabled={!canEdit || busy} onClick={() => choose(t.key)}>Pakai template ini</button>
            </div>
          </div>
        ))}
      </div>
      {preview && (
        <div className="e-preview-wrap" style={{ position: "static", maxHeight: "none" }}>
          <div className="e-preview">
            <Site content={preview.content} business={{ ...business, profile: { ...business.profile, name: business.profile.name || "Nama Toko" } }} assetUrl={() => undefined} />
          </div>
        </div>
      )}
    </>
  );
}
