# Studio Multi-Page, Navbar Top/Side, Background In-Through, and Canvas Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the Studio website builder into a modular suite with multi-page management (Starter 3 standard pages vs Pro unlimited), top/side navbar options, custom background image with in-through blur/transparency, image focal point/crop controls, freehand brush & eraser drawing layers, and compact grouped tools.

**Architecture:** Extend backend `schema_v2.rs` with `navbar_style`, background properties, and vector `drawings` strokes. Implement multi-page CRUD and tier enforcement in `website_repo` and `website.rs`. Overhaul Studio frontend into an icon-rail layout with compact categorized drawer, canvas drawing overlays, background in-through effects, and image manipulation modals.

**Tech Stack:** Rust 2024 (axum, sqlx, serde_json, utoipa), Next.js 16 (React 19, TypeScript), Phosphor Icons, Vitest, SVG vector canvas.

## Global Constraints

- Backend validation must stay fail-closed: strict whitelist of object keys, no unknown fields.
- Document size ceiling remains 256 KiB (`MAX_DOCUMENT_BYTES`). Max 200 drawing strokes per page.
- Starter plan is entitled to 3 standard pages: `home` (`/`), `about` (`/tentang`), `contact` (`/kontak`). Custom pages require Pro plan (`website_pages_pro`).
- When a tenant downgrades, custom pages are preserved in database but unentitled pages/blocks are hidden from public rendering.
- No third-party freeform HTML/script injection (CSP compliant).
- All frontend UI copy must use Indonesian, following project conventions.

---

## Chunk 1 — Domain & Schema Contracts

### Task 1: Extend `schema_v2.rs` for Navbar, Background, and Drawings

**Files:**
- Modify: `crates/domain/src/website/schema_v2.rs`
- Test: `crates/domain/src/website/schema_v2.rs` (unit tests)

**Interfaces:**
- Produces:
  - Whitelist keys in `root`: `schema_version`, `template`, `theme`, `seo`, `blocks`, `drawings`.
  - Whitelist keys in `theme`: `primary`, `font`, `surface`, `text_scale`, `navbar_style`, `navbar_sticky`, `background_image`, `background_size`, `background_position`, `background_opacity`.
  - Validation rules for `drawings`: max 200 items, each with `id`, `target` (string), `color` (hex/rgb), `size` (1..=64), `opacity` (0.0..=1.0), `points` (array of `[i64, i64]`, max 1000 points per stroke).

- [ ] **Step 1: Write failing unit tests for new schema fields.**

In `crates/domain/src/website/schema_v2.rs` inside `mod tests`, add:

```rust
#[test]
fn validates_extended_theme_and_drawings() {
    let mut d = json!({
        "schema_version": 2,
        "template": "coffee",
        "theme": {
            "primary": "#4F46E5",
            "font": "inter",
            "surface": "cream",
            "text_scale": 1.0,
            "navbar_style": "side",
            "navbar_sticky": true,
            "background_image": "asset:01b2ad6c-8e8d-4e26-b97f-4f1f4cf6b0ee",
            "background_size": "cover",
            "background_position": "center",
            "background_opacity": 25
        },
        "blocks": [],
        "drawings": [
            {
                "id": "stroke-1",
                "target": "background",
                "color": "#FF0000",
                "size": 4,
                "opacity": 0.8,
                "points": [[10, 20], [30, 40], [50, 60]]
            }
        ]
    });
    let res = validate_page_v2(&d, &[]);
    assert!(res.is_ok(), "extended v2 doc should be valid: {res:?}");

    // Invalid navbar style
    d["theme"]["navbar_style"] = json!("invalid");
    let err = validate_page_v2(&d, &[]).unwrap_err();
    assert!(err.iter().any(|e| e.path == "theme.navbar_style"));

    // Invalid background size
    d["theme"]["navbar_style"] = json!("top");
    d["theme"]["background_size"] = json!("stretched");
    let err = validate_page_v2(&d, &[]).unwrap_err();
    assert!(err.iter().any(|e| e.path == "theme.background_size"));
}
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `cargo test -p umkm-domain website::schema_v2::tests::validates_extended_theme_and_drawings`
Expected: FAIL with unknown field in theme / root.

- [ ] **Step 3: Implement validation in `schema_v2.rs`.**

Update `ROOT_KEYS`:
```rust
pub const ROOT_KEYS: [&str; 6] = ["schema_version", "template", "theme", "seo", "blocks", "drawings"];
pub const THEME_KEYS: [&str; 10] = [
    "primary", "font", "surface", "text_scale",
    "navbar_style", "navbar_sticky",
    "background_image", "background_size", "background_position", "background_opacity",
];
pub const NAVBAR_STYLES: [&str; 2] = ["top", "side"];
pub const BACKGROUND_SIZES: [&str; 3] = ["cover", "contain", "repeat"];
pub const MAX_DRAWING_STROKES: usize = 200;
pub const MAX_STROKE_POINTS: usize = 1000;
```

Update `validate_page_v2` root validation and `theme` validation:
```rust
    let Some(root) = v.object(val, "", &ROOT_KEYS) else {
        return v.finish();
    };
```
In `fn theme(v: &mut V, t: &Value)`:
Validate `navbar_style` (must be in `NAVBAR_STYLES`), `navbar_sticky` (bool), `background_image` (asset ref string), `background_size` (must be in `BACKGROUND_SIZES`), `background_position` (max 40 chars), `background_opacity` (integer 0..=100).

Add `fn drawings(v: &mut V, d: &Value)`:
Validate array length <= `MAX_DRAWING_STROKES`. Each stroke object must have `id`, `target`, `color`, `size` (1..=64), `opacity` (0.0..=1.0), `points` (array of 2-element integer arrays <= 1000).

- [ ] **Step 4: Run test to verify it passes.**

Run: `cargo test -p umkm-domain website::schema_v2`
Expected: PASS with all existing and new tests green.

- [ ] **Step 5: Commit.**

```bash
git add crates/domain/src/website/schema_v2.rs
git commit -m "feat(domain): perluas schema_v2 untuk navbar, background, dan drawings"
```

---

### Task 2: Sync Frontend `doc.ts` and Types

**Files:**
- Modify: `frontend/src/studio/doc.ts`
- Modify: `frontend/src/renderer/types.ts`
- Test: `frontend/src/studio/doc.test.ts`

**Interfaces:**
- Produces:
  - `DrawingStroke` interface: `{ id: string; target: "background" | string; color: string; size: number; opacity: number; points: [number, number][] }`.
  - Updated `StudioState` and `PageV2` with `drawings`, `navbar_style`, and `background_*` fields.
  - `toDoc` and `fromDoc` roundtrip fidelity for new fields.

- [ ] **Step 1: Write failing test in `doc.test.ts`.**

In `frontend/src/studio/doc.test.ts`, add:

```typescript
it("preserves navbar, background, and drawings in toDoc and fromDoc", () => {
  const initial: StudioState = {
    primary: "#15803D",
    font: "inter",
    surface: "light",
    navbarStyle: "side",
    navbarSticky: true,
    backgroundImage: "asset:bg-123",
    backgroundSize: "cover",
    backgroundPosition: "center",
    backgroundOpacity: 40,
    installed: ["hero"],
    cells: [],
    drawings: [
      { id: "s1", target: "background", color: "#FF0000", size: 4, opacity: 1, points: [[0, 0], [10, 10]] },
    ],
  };
  const doc = toDoc(initial, "coffee");
  expect(doc.theme.navbar_style).toBe("side");
  expect(doc.theme.navbar_sticky).toBe(true);
  expect(doc.theme.background_image).toBe("asset:bg-123");
  expect(doc.theme.background_opacity).toBe(40);
  expect(doc.drawings).toHaveLength(1);

  const restored = fromDoc(doc, () => ({ minW: 4, minH: 2 }));
  expect(restored.navbarStyle).toBe("side");
  expect(restored.navbarSticky).toBe(true);
  expect(restored.backgroundImage).toBe("asset:bg-123");
  expect(restored.backgroundOpacity).toBe(40);
  expect(restored.drawings).toHaveLength(1);
});
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `cd frontend && npm test -- src/studio/doc.test.ts`
Expected: FAIL due to missing properties on `StudioState` and `doc.theme`.

- [ ] **Step 3: Update `frontend/src/studio/doc.ts`.**

Add `DrawingStroke` type:
```typescript
export interface DrawingStroke {
  id: string;
  target: "background" | string;
  color: string;
  size: number;
  opacity: number;
  points: [number, number][];
}
```
Update `StudioState` and `PageV2` to include `navbarStyle`, `navbarSticky`, `backgroundImage`, `backgroundSize`, `backgroundPosition`, `backgroundOpacity`, and `drawings?: DrawingStroke[]`.
Update `toDoc` to serialize them into `doc.theme` and `doc.drawings`.
Update `fromDoc` to restore them into `StudioState`.

- [ ] **Step 4: Run test to verify it passes.**

Run: `cd frontend && npm test -- src/studio/doc.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add frontend/src/studio/doc.ts frontend/src/renderer/types.ts frontend/src/studio/doc.test.ts
git commit -m "feat(studio): sinkronkan doc.ts dan types dengan ekstensi schema v2"
```

---

## Chunk 2 — Multi-Page Management API

### Task 3: Backend Multi-Page Management & Tiering

**Files:**
- Modify: `crates/infrastructure/src/website_repo.rs`
- Modify: `apps/api/src/website.rs`
- Test: `apps/api/tests/website_http.rs`

**Interfaces:**
- Produces:
  - `POST /api/v1/tenants/{tenant_id}/website/pages`: creates a new page with `{ slug, title }`.
  - `DELETE /api/v1/tenants/{tenant_id}/website/pages/{slug}`: deletes a non-home page.
  - Plan check: Starter allowed only standard slugs (`home`, `about`, `contact`). Creating custom slug on Starter returns 403 `plan_limit`.

- [ ] **Step 1: Write failing HTTP test in `apps/api/tests/website_http.rs`.**

Add tests:
```rust
#[sqlx::test(migrator = "MIGRATOR")]
async fn create_page_starter_vs_pro(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "site-pages").await;
    let o = member(&pool, t, "Owner").await;
    select(&a, t, &o, "coffee").await;

    // Starter can create standard page "about"
    let r = json(
        &a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/website/pages"),
        Some(&o),
        Some(json!({"slug": "about", "title": "Tentang Kami"})),
    ).await;
    assert_eq!(r.status, 201, "{}", r.json());

    // Starter cannot create custom page "promo-lebaran"
    let r = json(
        &a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/website/pages"),
        Some(&o),
        Some(json!({"slug": "promo-lebaran", "title": "Promo Lebaran"})),
    ).await;
    assert_eq!(r.status, 403, "{}", r.json());
    assert_eq!(r.json()["error"]["code"], "FORBIDDEN");
}
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `eval "$(scripts/dev-db.sh up)" && cargo test -p umkm-api --test website_http create_page_starter_vs_pro`
Expected: FAIL with 404 (route not found).

- [ ] **Step 3: Implement `create_page` and `delete_page` in `website_repo.rs`.**

```rust
pub async fn create_page(
    pool: &PgPool,
    actor: &Actor,
    slug: &str,
    title: &str,
    initial_content: &Value,
    ctx: Ctx<'_>,
) -> Result<PageView, WebsiteError> {
    let mut tx = tx(pool, actor.tenant_id).await?;
    let website_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM websites WHERE tenant_id = $1",
    )
    .bind(actor.tenant_id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;

    let page_id: Uuid = sqlx::query_scalar(
        "INSERT INTO website_pages (tenant_id, website_id, slug, title, draft_content, draft_updated_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(actor.tenant_id.as_uuid())
    .bind(website_id)
    .bind(slug)
    .bind(title)
    .bind(Json(initial_content))
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(PageView {
        slug: slug.to_string(),
        title: title.to_string(),
        draft_revision: 1,
        published_version: None,
    })
}
```

- [ ] **Step 4: Route endpoints and plan gate in `apps/api/src/website.rs`.**

Add `CreatePage` struct with `slug` and `title`.
In `create_page_handler`:
- Check permission `website.edit`.
- Check plan: `let plan = crate::billing::plan_of(&state, actor.tenant_id).await?;`
  If plan code != "pro" && !["home", "about", "contact"].contains(&req.slug.as_str()):
  return 403 `ApiError::new(ErrorCode::Forbidden).with_message("Halaman kustom memerlukan paket Pro")`.
- Call `repo::create_page`.
- Return 201 with `PageView`.
Add `delete_page_handler` (rejects deleting `"home"`).
Register routes in `website::routes`.

- [ ] **Step 5: Run tests to verify they pass.**

Run: `eval "$(scripts/dev-db.sh up)" && cargo test -p umkm-api --test website_http create_page_starter_vs_pro`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/infrastructure/src/website_repo.rs apps/api/src/website.rs apps/api/tests/website_http.rs
git commit -m "feat(api): tambah endpoint pembuatan & penghapusan halaman website beserta aturan paket"
```

---

## Chunk 3 — Studio Workspace & Navigation Architecture

### Task 4: Modular Studio Suite (Icon Rail & Grouped Plugin Drawer)

**Files:**
- Create: `frontend/src/studio/IconRail.tsx`
- Modify: `frontend/src/app/(studio)/studio/page.tsx`
- Modify: `frontend/src/renderer/site.css`

**Interfaces:**
- Produces:
  - `IconRail` component with tabs: `pages`, `elements`, `theme`, `draw`.
  - Grouped catalog in `studio/page.tsx`: 4 accordion/card sections (`Konten`, `Jualan`, `Layanan`, `Info Toko`).
  - Active drawer state with compact 2-column drag-and-drop cards.

- [ ] **Step 1: Create `frontend/src/studio/IconRail.tsx`.**

```tsx
"use client";

import { Files, PuzzlePiece, Palette, PaintBrush } from "@phosphor-icons/react/ssr";

export type StudioTab = "pages" | "elements" | "theme" | "draw";

interface IconRailProps {
  activeTab: StudioTab | null;
  onSelectTab: (tab: StudioTab) => void;
}

export function IconRail({ activeTab, onSelectTab }: IconRailProps) {
  const tabs: { id: StudioTab; label: string; icon: typeof Files }[] = [
    { id: "pages", label: "Halaman", icon: Files },
    { id: "elements", label: "Elemen", icon: PuzzlePiece },
    { id: "theme", label: "Desain", icon: Palette },
    { id: "draw", label: "Coretan", icon: PaintBrush },
  ];

  return (
    <nav className="st-rail" aria-label="Bilah Alat Studio">
      {tabs.map((t) => {
        const Icon = t.icon;
        const active = activeTab === t.id;
        return (
          <button
            key={t.id}
            type="button"
            className={`st-rail-btn${active ? " active" : ""}`}
            onClick={() => onSelectTab(t.id)}
            aria-pressed={active}
            title={t.label}
          >
            <Icon weight={active ? "fill" : "bold"} size={22} />
            <span>{t.label}</span>
          </button>
        );
      })}
    </nav>
  );
}
```

- [ ] **Step 2: Add styles in `frontend/src/renderer/site.css`.**

Add `.st-rail` (width 56px, vertical flex, border-right, dark/light theme background) and `.st-rail-btn` styling (stacked icon + micro-caption, transition, hover effect, active indicator bar).

- [ ] **Step 3: Integrate `IconRail` and Grouped Catalog into `studio/page.tsx`.**

Replace flat plugin list with categorized grid:
```tsx
const CATEGORIES: PluginCategory[] = ["Konten", "Jualan", "Layanan", "Info toko"];
// Render category sections with compact 2-column cards
```

- [ ] **Step 4: Verify frontend builds and passes linting.**

Run: `cd frontend && npm run lint && npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add frontend/src/studio/IconRail.tsx frontend/src/app/\(studio\)/studio/page.tsx frontend/src/renderer/site.css
git commit -m "feat(studio): tambahkan icon rail 56px dan pengelompokan plugin 2-kolom"
```

---

### Task 5: Multi-Page UI & Tiering in Studio

**Files:**
- Create: `frontend/src/studio/PagesPanel.tsx`
- Modify: `frontend/src/app/(studio)/studio/page.tsx`

**Interfaces:**
- Produces:
  - `PagesPanel` showing list of pages (`Beranda`, `Tentang Kami`, `Kontak`, and custom pages).
  - Ability to switch active page, editing that page's draft.
  - Modal "+ Tambah Halaman" with Starter (limited to standard pages) vs Pro (unlocked custom pages).

- [ ] **Step 1: Create `frontend/src/studio/PagesPanel.tsx`.**

Implement `PagesPanel`:
- List pages loaded from `/api/v1/tenants/${tenantId}/website`.
- Active page badge.
- Page title & slug editing.
- "+ Tambah Halaman" dialog:
  - Standard templates: `Tentang Kami` (`about`), `Kontak` (`contact`).
  - Custom page input: Title & Slug (with `Pro` badge if plan is Starter).
- Deletion button (disabled on `home`).

- [ ] **Step 2: Wire `PagesPanel` into Studio `page.tsx`.**

In `studio/page.tsx`:
- Maintain state `activeSlug: string` (defaults to `"home"`).
- When changing `activeSlug`: call `saveNow()`, fetch new draft for `${activeSlug}`, re-initialize `history`.
- In `SaveBadge` and `Header`: display current page name.

- [ ] **Step 3: Verify frontend typecheck and tests.**

Run: `cd frontend && npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add frontend/src/studio/PagesPanel.tsx frontend/src/app/\(studio\)/studio/page.tsx
git commit -m "feat(studio): integrasikan panel manajemen multi-halaman dan kontrol paket"
```

---

### Task 6: Navbar Top vs Side & Background In-Through

**Files:**
- Create: `frontend/src/studio/ThemePanel.tsx`
- Modify: `frontend/src/renderer/Site.tsx`
- Modify: `frontend/src/renderer/StudioSite.tsx`
- Modify: `frontend/src/renderer/site.css`

**Interfaces:**
- Produces:
  - Navbar style selector (`top` vs `side`, `sticky` toggle).
  - Background image uploader with size (`cover`, `contain`, `repeat`), position, and overlay opacity slider.
  - Renderer support for Side Navbar layout and In-Through backdrop blur.

- [ ] **Step 1: Write unit test for Side Navbar & Background rendering.**

In `frontend/src/renderer/Site.test.tsx`, add:
```tsx
it("renders side navigation layout when navbar_style is side", () => {
  const content: PageContent = {
    schema_version: 2,
    template: "coffee",
    theme: {
      primary: "#15803D",
      font: "inter",
      navbar_style: "side",
    },
    sections: [{ id: "h1", type: "header", enabled: true, data: { title: "Toko Side" } }],
  };
  const html = renderToStaticMarkup(<Site content={content} business={EMPTY_BIZ} assetUrl={() => ""} />);
  expect(html).toContain("s-nav-side");
});
```

- [ ] **Step 2: Implement Navbar Side & Top in `Site.tsx`.**

When `content.theme.navbar_style === "side"`:
Wrap site in `.site-with-side-nav` container:
- Desktop: Left sticky sidebar with logo, title, and nav links.
- Mobile (< 768px): Top bar with collapsible hamburger drawer.
When `content.theme.navbar_style === "top"` (default):
Render horizontal top bar as before, adding `.s-sticky` if `navbar_sticky` is true.

- [ ] **Step 3: Implement Background In-Through in `site.css` and `Site.tsx`.**

Apply CSS custom properties on `.site`:
`--s-bg-image`, `--s-bg-size`, `--s-bg-position`, `--s-bg-overlay`.
When background image exists:
Add overlay layer with `background-color: rgba(var(--surface-rgb), var(--s-bg-overlay))` and apply `backdrop-filter: blur(8px)` on cards.

- [ ] **Step 4: Create `ThemePanel.tsx` in `frontend/src/studio/ThemePanel.tsx`.**

Include:
- Color palette picker + free hex input.
- Font selector + text scale.
- Navbar selector: Bilah Atas (Top) vs Bilah Samping (Side) + Sticky toggle.
- Background Image: upload, size selector (`Cover`, `Contain`, `Pola Berulang`), focal position, and transparency slider (0–90%).

- [ ] **Step 5: Run tests and verify.**

Run: `cd frontend && npm test && npm run typecheck`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add frontend/src/studio/ThemePanel.tsx frontend/src/renderer/Site.tsx frontend/src/renderer/StudioSite.tsx frontend/src/renderer/site.css frontend/src/renderer/Site.test.tsx
git commit -m "feat(renderer): tambah dukungan navbar top/side dan background in-through"
```

---

## Chunk 4 — Image Controls & Drawing Tools

### Task 7: Image Reposition, Sizing (Contain vs Cover), and Crop Tool

**Files:**
- Create: `frontend/src/ui/ImageCropModal.tsx`
- Modify: `frontend/src/renderer/Site.tsx`
- Modify: `frontend/src/app/(studio)/studio/page.tsx`

**Interfaces:**
- Produces:
  - `<ImageCropModal>` component using HTML5 Canvas to crop images to standard ratios (1:1, 16:9, 4:3, Free).
  - Inspector controls for image plugins: Mode Penuh (Contain) vs Potong (Cover), and Focal Point selector (9 positions).

- [ ] **Step 1: Create `frontend/src/ui/ImageCropModal.tsx`.**

Interactive modal with canvas:
- Loads source image.
- Draggable / resizable crop rectangle with aspect ratio locks.
- "Selesai" button exports Blob/File and invokes `onCrop(file: File)`.

- [ ] **Step 2: Update `<Img>` in `frontend/src/renderer/Site.tsx`.**

Support `object-fit` (`cover` | `contain`) and `object-position` passed via plugin data:
```tsx
style={{
  objectFit: (d.image_fit as "cover" | "contain") ?? "cover",
  objectPosition: (d.image_position as string) ?? "center",
}}
```

- [ ] **Step 3: Add Crop & Reposition controls to Plugin Inspector in `studio/page.tsx`.**

For any plugin with `takesPhoto(c.kind)`:
Add:
- Toggle: "Potong Rapi (Cover)" vs "Tampil Utuh (Contain)".
- Focal point grid (9 buttons: ↖ ↑ ↗ ← • → ↙ ↓ ↘).
- Button: "Potong Foto" (opens `ImageCropModal`).

- [ ] **Step 4: Verify typecheck and tests.**

Run: `cd frontend && npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add frontend/src/ui/ImageCropModal.tsx frontend/src/renderer/Site.tsx frontend/src/app/\(studio\)/studio/page.tsx
git commit -m "feat(studio): tambah kontrol reposisi gambar, contain vs cover, dan modal crop"
```

---

### Task 8: Freehand Brush & Eraser Vector Tools

**Files:**
- Create: `frontend/src/studio/DrawingLayer.tsx`
- Create: `frontend/src/studio/DrawPanel.tsx`
- Modify: `frontend/src/studio/Canvas.tsx`
- Modify: `frontend/src/app/(studio)/studio/page.tsx`
- Test: `frontend/src/studio/DrawingLayer.test.tsx`

**Interfaces:**
- Produces:
  - `DrawingLayer`: SVG vector layer rendering `DrawingStroke[]`.
  - Pointer event handlers for drawing paths with smoothing.
  - `DrawPanel`: tool selection (Brush vs Eraser), stroke size, color, opacity, target layer (`Background` vs `Cell`).

- [ ] **Step 1: Write test for `DrawingLayer`.**

Create `frontend/src/studio/DrawingLayer.test.tsx`:
```tsx
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DrawingLayer } from "./DrawingLayer";

describe("DrawingLayer", () => {
  it("renders SVG paths for strokes matching the target", () => {
    const strokes = [
      { id: "1", target: "background" as const, color: "#FF0000", size: 4, opacity: 1, points: [[10, 10], [50, 50]] as [number, number][] },
      { id: "2", target: "cell-123", color: "#00FF00", size: 2, opacity: 0.5, points: [[0, 0], [20, 20]] as [number, number][] },
    ];
    const html = renderToStaticMarkup(<DrawingLayer target="background" strokes={strokes} width={1080} height={800} />);
    expect(html).toContain("path");
    expect(html).toContain('stroke="#FF0000"');
    expect(html).not.toContain('stroke="#00FF00"');
  });
});
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `cd frontend && npm test -- src/studio/DrawingLayer.test.tsx`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement `frontend/src/studio/DrawingLayer.tsx`.**

Implement:
- SVG path generator using smooth quadratic Bézier curve interpolation from points.
- `<DrawingLayer>` renders `<svg>` with viewBox matching target bounding box.
- Interactive mode (when active tool is `brush` or `eraser`): captures `onPointerDown`, `onPointerMove`, `onPointerUp` to record new stroke or remove clicked strokes.

- [ ] **Step 4: Create `DrawPanel.tsx` and integrate into Studio.**

Options in `DrawPanel`:
- Mode: Kuas (Brush) vs Penghapus (Eraser).
- Lapisan Target: Latar Belakang (di bawah blok) vs Plugin Terpilih (menempel di atas blok).
- Ukuran Kuas: 2px, 4px, 8px, 18px.
- Warna Kuas: Palet tema + custom hex.
- Gaya Garis: Padat (100%) vs Stabilo (40% transparan).
- Tombol: "Hapus Semua Coretan".

Mount `<DrawingLayer target="background" ...>` underneath grid in `Canvas.tsx`.
Mount `<DrawingLayer target={c.id} ...>` on top of individual cells in `Canvas.tsx`.

- [ ] **Step 5: Run tests and verify.**

Run: `cd frontend && npm test -- src/studio/DrawingLayer.test.tsx && npm run typecheck`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add frontend/src/studio/DrawingLayer.tsx frontend/src/studio/DrawPanel.tsx frontend/src/studio/Canvas.tsx frontend/src/app/\(studio\)/studio/page.tsx frontend/src/studio/DrawingLayer.test.tsx
git commit -m "feat(studio): tambah lapisan gambar bebas brush dan eraser vektor SVG"
```

---

## Chunk 5 — Final Integration, Build, and Verification

### Task 9: Comprehensive Workspace Verification & Quality Gate

**Files:**
- Modify: `docs/superpowers/plans/2026-11-16-studio-pages-navbar-canvas-tools.md` (check off items)

- [ ] **Step 1: Run full frontend quality checks.**

```bash
cd /home/ryasr/umkm-os/frontend
npm run lint
npm run typecheck
npm test
npm run build
```
Verify exit code 0 and all 27+ pages prerendered with valid CSP hashes.

- [ ] **Step 2: Run backend tests and clippy.**

```bash
cd /home/ryasr/umkm-os
eval "$(scripts/dev-db.sh up)"
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny check
```
Verify zero test failures and zero clippy warnings.

- [ ] **Step 3: Verify git status and diff cleanliness.**

```bash
git status --short
git diff --check
```
Ensure no uncommitted artifacts or stray files.

- [ ] **Step 4: Commit.**

```bash
git commit -am "chore(studio): finalisasi dan verifikasi implementasi fitur studio suite"
```
