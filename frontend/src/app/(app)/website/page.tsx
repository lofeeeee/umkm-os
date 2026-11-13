"use client";

import { useCallback, useEffect, useMemo, useState } from "react";

import Link from "next/link";

import { Sparkle } from "@phosphor-icons/react/ssr";
import { ArrowDown, ArrowSquareOut, ArrowUp, CaretDown, CaretRight, DesktopTower, DeviceMobile, Eye, EyeSlash, Plus, Trash } from "@phosphor-icons/react/ssr";

import { prepareImage } from "@/lib/image";
import { Assets, FieldInput, type Upload } from "@/dashboard/editor/fields";
import { Alert, WithTenant, type TenantCtx } from "@/dashboard/shell";
import { waktu } from "@/lib/labels";
import { confirm, Sheet } from "@/ui/dialog";
import { Badge, BusyButton, Card, PageHeader, PageSkeleton } from "@/ui/page";
import { toast } from "@/ui/toast";
import { isV2 } from "@/studio/doc";
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
  const [open, setOpen] = useState<string | null>(null);
  const [panel, setPanel] = useState<"sections" | "theme" | "history">("sections");
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
          const u = await api<Upload>("POST", `${base}/uploads`, { bytes: await prepareImage(file) });
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
    try {
      await fn();
      if (label) toast.ok(label);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.message.includes("Draft")) {
        setError("Draft sudah diubah di tempat lain. Muat ulang untuk melihat versi terbaru (perubahan Anda belum tersimpan).");
      } else setError(describe(e));
    } finally {
      setBusy(false);
    }
  }

  const save = () => run("Draf tersimpan.", async () => {
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

  const rollback = async (version: number) => {
    if (!(await confirm({ title: `Kembalikan ke versi ${version}?`, body: "Situs publik akan menampilkan versi ini. Draf yang sedang Anda ubah tidak berubah.", confirmLabel: "Kembalikan" }))) return;
    void run(`Publik kini menampilkan versi ${version}.`, async () => {
      await api("POST", `${base}/website/pages/home/rollback`, { json: { version } });
      await loadDraft();
    });
  };

  const showVersion = (version: number) => run("", async () => {
    const d = await api<DraftView>("GET", `${base}/website/pages/home/versions/${version}`);
    setViewing({ version, content: d.content, assets: d.assets });
  });

  if (site === undefined) return <>{error ? <Alert kind="error">{error}</Alert> : <PageSkeleton />}</>;
  if (site === null) return <TemplatePicker base={base} canEdit={canEdit} business={business} onSelected={async () => { setSite(await api<WebsiteView>("GET", `${base}/website`)); await loadDraft(); }} />;
  if (!content) return <Alert kind="error">{error}</Alert>;
  // ADR-0017: draf sudah berformat Studio (v2) → editor lama tidak boleh menimpanya.
  if (isV2(content)) {
    return (
      <>
        <PageHeader title="Website" description="Website toko Anda kini disusun di Studio: kanvas grid, plugin, dan pratinjau langsung." />
        <Card>
          <div className="d-empty">
            <span className="d-empty-icon" aria-hidden="true"><Sparkle weight="duotone" /></span>
            <p className="d-empty-title">Website ini dikelola di Studio</p>
            <p>Ubah tampilan, terbitkan, dan pulihkan versi lama langsung dari Studio.</p>
            <Link className="d-btn d-btn-primary" href={`/studio?tenant=${ctx.tenant.id}`}>Buka Studio</Link>
          </div>
        </Card>
      </>
    );
  }

  const previewContent = viewing?.content ?? content;
  const previewAssets = viewing
    ? (r: string) => (viewing.assets[r] ? apiUrl(viewing.assets[r]!) : undefined)
    : assets.url;
  const state = dirty
    ? <Badge tone="warn">Perubahan belum disimpan</Badge>
    : published ? <Badge tone="ok">Terbit · versi {published}</Badge> : <Badge>Belum terbit</Badge>;

  return (
    <Assets.Provider value={assets}>
      <PageHeader
        title="Website"
        description={<>Ubah tampilan situs toko, cek pratinjau, lalu terbitkan. Templat {site.template_key} v{site.template_version}. {state}</>}
        actions={
          <>
            <Link className="d-btn d-btn-ghost" href={`/studio?tenant=${ctx.tenant.id}`}><Sparkle weight="bold" aria-hidden="true" />Buka Studio</Link>
            {ctx.tenant.primary_hostname && published && (
              <a className="d-btn d-btn-ghost" href={`https://${ctx.tenant.primary_hostname}`} target="_blank" rel="noopener noreferrer"><ArrowSquareOut weight="bold" aria-hidden="true" />Lihat situs</a>
            )}
            <button className="d-btn" onClick={save} disabled={!canEdit || busy || !dirty}>Simpan draf</button>
            <BusyButton type="button" busy={busy} onClick={publish} disabled={!canPublish}>Terbitkan</BusyButton>
          </>
        }
      />
      <Alert kind="error">{error}</Alert>

      <div className="e-layout">
        <div className="e-side">
          <div className="d-seg" role="tablist" aria-label="Panel editor" style={{ justifySelf: "start" }}>
            {([["sections", `Bagian (${content.sections.length})`], ["theme", "Tema"], ["history", "Riwayat"]] as const).map(([k, l]) => (
              <button key={k} type="button" role="tab" aria-selected={panel === k} onClick={() => setPanel(k)}>{l}</button>
            ))}
          </div>

          {panel === "theme" && (
            <Card title="Tema" description="Warna dan font dipakai di seluruh situs.">
              <div className="d-grid">
                <label className="d-field"><span>Warna utama</span>
                  <span className="d-color">
                    <input type="color" value={/^#[0-9a-fA-F]{6}$/.test(content.theme.primary) ? content.theme.primary : "#2563eb"} disabled={!canEdit}
                      onChange={(e) => update((c) => ({ ...c, theme: { ...c.theme, primary: e.target.value.toUpperCase() } }))} />
                    <code>{content.theme.primary}</code>
                  </span>
                </label>
                <label className="d-field"><span>Font</span>
                  <select value={content.theme.font} disabled={!canEdit} onChange={(e) => update((c) => ({ ...c, theme: { ...c.theme, font: e.target.value } }))}>
                    {FONT_PRESETS.map((f) => <option key={f} value={f}>{f}</option>)}
                  </select>
                </label>
              </div>
            </Card>
          )}

          {panel === "sections" && (
            <div>
              <p className="d-muted d-small" style={{ margin: "0 0 8px" }}>Klik bagian untuk mengubah isinya. Ikon mata = tampil/sembunyi di situs.</p>
              {content.sections.map((s, i) => (
                <div key={s.id} className={`e-sec${s.enabled ? "" : " e-sec-off"}${open === s.id ? " e-sec-open" : ""}`}>
                  <div className="e-sec-head">
                    <button type="button" className="e-sec-toggle" aria-expanded={open === s.id} onClick={() => setOpen(open === s.id ? null : s.id)}>
                      {open === s.id ? <CaretDown weight="bold" aria-hidden="true" /> : <CaretRight weight="bold" aria-hidden="true" />}
                      <span className="e-name"><span className="sr-only">Ubah </span>{SECTION_LABELS[s.type]}</span>
                    </button>
                    <label className="d-icon-btn" title={s.enabled ? "Tampil di situs" : "Disembunyikan"}>
                      <input type="checkbox" className="sr-only" checked={s.enabled} disabled={!canEdit} aria-label={`Aktifkan ${SECTION_LABELS[s.type]}`}
                        onChange={(e) => updateSection(s.id, (x) => ({ ...x, enabled: e.target.checked }))} />
                      {s.enabled ? <Eye weight="bold" aria-hidden="true" /> : <EyeSlash weight="bold" aria-hidden="true" />}
                    </label>
                    <button type="button" className="d-icon-btn" disabled={!canEdit || i === 0} onClick={() => move(i, -1)} aria-label="Naikkan"><ArrowUp weight="bold" /></button>
                    <button type="button" className="d-icon-btn" disabled={!canEdit || i === content.sections.length - 1} onClick={() => move(i, 1)} aria-label="Turunkan"><ArrowDown weight="bold" /></button>
                  </div>
                  {open === s.id && (
                    <div className="e-sec-body">
                      {FIELDS[s.type].map((f) => (
                        <FieldInput key={f.key} f={f} data={s.data} disabled={!canEdit} onChange={(d) => updateSection(s.id, (x) => ({ ...x, data: d }))} />
                      ))}
                      <button type="button" className="d-btn d-btn-sm d-btn-ghost" style={{ color: "var(--danger)" }} disabled={!canEdit}
                        onClick={async () => {
                          if (await confirm({ title: `Hapus bagian “${SECTION_LABELS[s.type]}”?`, body: "Bagian dihapus dari draf. Situs publik tidak berubah sampai Anda menerbitkan.", confirmLabel: "Hapus", danger: true })) {
                            update((c) => ({ ...c, sections: c.sections.filter((x) => x.id !== s.id) }));
                          }
                        }}>
                        <Trash weight="bold" aria-hidden="true" />Hapus bagian
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
          )}

          {panel === "history" && (
            <Card title="Riwayat terbit" description="Lihat versi lama atau tampilkan kembali di situs publik.">
              {versions.length === 0 && <p className="d-muted">Belum pernah diterbitkan.</p>}
              <ul className="e-versions">
                {versions.map((v) => (
                  <li key={v.version}>
                    <span style={{ flex: 1 }}>
                      <strong>Versi {v.version}</strong> {v.is_current && <Badge tone="ok">Aktif</Badge>}
                      <span className="d-sub d-muted d-small" style={{ display: "block" }}>{waktu(v.created_at_unix * 1000)}</span>
                    </span>
                    <button className="d-btn d-btn-sm" onClick={() => showVersion(v.version)}>Lihat</button>
                    {!v.is_current && <button className="d-btn d-btn-sm" disabled={!canPublish || busy} onClick={() => void rollback(v.version)}>Kembalikan</button>}
                  </li>
                ))}
              </ul>
            </Card>
          )}
        </div>

        <div className="e-preview-col">
          <div className="e-preview-bar">
            <span className="d-muted d-small">{viewing ? `Pratinjau versi ${viewing.version}` : "Pratinjau draf"}</span>
            {viewing && <button className="d-btn d-btn-sm" onClick={() => setViewing(null)}>Kembali ke draf</button>}
            <div className="d-seg" role="group" aria-label="Ukuran pratinjau" style={{ marginLeft: "auto" }}>
              <button type="button" aria-pressed={view === "mobile"} onClick={() => setView("mobile")}><DeviceMobile weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> Ponsel</button>
              <button type="button" aria-pressed={view === "desktop"} onClick={() => setView("desktop")}><DesktopTower weight="bold" aria-hidden="true" style={{ verticalAlign: "-2px" }} /> Layar lebar</button>
            </div>
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
    <div className="d-row e-add" style={{ flexWrap: "nowrap" }}>
      <select value={type} onChange={(e) => setType(e.target.value as SectionType)} disabled={disabled} aria-label="Tipe bagian baru">
        {SECTION_TYPES.map((t) => <option key={t} value={t}>{SECTION_LABELS[t]}</option>)}
      </select>
      <button type="button" className="d-btn" disabled={disabled} onClick={() => onAdd(type)}><Plus weight="bold" aria-hidden="true" />Tambah bagian</button>
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
      <PageHeader title="Website" description="Mulai dengan memilih template. Semua teks, foto, dan warna bisa diubah setelahnya." />
      <Alert kind="error">{error}</Alert>
      <div className="e-tpl d-stagger">
        {templates.map((t) => (
          <div className="d-card e-tpl-card" key={t.key}>
            <div className="e-tpl-thumb" aria-hidden="true"><span>{t.name.slice(0, 1)}</span></div>
            <h2>{t.name}</h2>
            <p className="d-muted d-small">{t.category} · v{t.version}</p>
            <div className="d-row">
              <button className="d-btn" onClick={async () => setPreview({ key: t.key, content: (await api<{ content: PageContent }>("GET", `${base}/website/templates/${t.key}/preview`)).content })}>Pratinjau</button>
              <BusyButton type="button" busy={busy} disabled={!canEdit} onClick={() => choose(t.key)}>Pakai template ini</BusyButton>
            </div>
          </div>
        ))}
      </div>
      <Sheet open={!!preview} onClose={() => setPreview(null)} wide title="Pratinjau template"
        footer={preview && <><button className="d-btn" onClick={() => setPreview(null)}>Tutup</button><BusyButton type="button" busy={busy} disabled={!canEdit} onClick={() => choose(preview.key)}>Pakai template ini</BusyButton></>}>
        {preview && (
          <div className="e-preview">
            <Site content={preview.content} business={{ ...business, profile: { ...business.profile, name: business.profile.name || "Nama Toko" } }} assetUrl={() => undefined} />
          </div>
        )}
      </Sheet>
    </>
  );
}
