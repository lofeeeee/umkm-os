# Studio UI Audit — Step 1

Scope: `frontend/src/app/(studio)/studio/page.tsx`, `src/studio/Canvas.tsx`, `src/studio/plugins.ts`, `sx-*` / `st-*` block of `src/app/globals.css`.

Findings verified against code; screenshots referenced as IMG1 (mobile, drawer open) and IMG2 (palette).

---

## 1. Responsivity Audit

### 1.1 Mobile panel (IMG1) — the awkward one

```css
/* globals.css:1389 */
@media (max-width: 900px) {
  .sx-body { grid-template-columns: minmax(0, 1fr); }
  .sx-panel { position: fixed; z-index: 20; top: 52px; bottom: 0; left: 0;
              width: min(320px, 90vw); box-shadow: var(--shadow-dialog); }
  .sx-canvas { padding: 16px; }
}
```

Issues:

- **No scrim / no in-panel close.** Fixed overlay with no backdrop; only close target is the
  `SidebarSimple` icon buried in `sx-top` — which is `overflow-x: auto` with ~9 other controls,
  so on a phone the toggle is scrolled off-screen.
- **FAB floats above the drawer.** `.st-float` is `z-index: 30` (globals.css:1324),
  `.sx-panel` is `z-index: 20` → the green chatbot bubble straddles the panel edge (IMG1).
- **No intermediate layout 900–1099px**; below 900 nothing adapts the panel *content* —
  plugin list, palette, SEO fields, tips all stack into one unbounded scroll.
- **Canvas zoom math:** `DESIGN_WIDTH = 1080` (Canvas.tsx:19) scaled to fit. With the panel
  eating 90vw, the preview lives in a sliver — unusable.

**Fix plan (priority order):**

1. Bottom sheet below 900px:
   ```css
   .sx-panel { position: fixed; left: 0; right: 0; bottom: 0; top: auto;
               max-height: 70vh; border-radius: 16px 16px 0 0; }
   ```
   plus a scrim div (tap to close) and an in-panel `X` / drag handle.
2. Z-index: `[data-panel="open"] .st-dock { display: none }` under the 900px query
   (or raise the scrim above z-30).
3. Segment panel content on mobile: `Terpasang / Cari plugin / Warna / SEO` as tabs
   instead of one long scroll. Note the aside's `aria-label` already flips between
   "Pengaturan plugin" and "Plugin" — the mixed content is felt, not accidental.
4. Keep drag-to-canvas hint desktop-only; the `+` button is the real mobile path.

### 1.2 `sx-top` toolbar

- `sx-top > * { flex: none }` + `overflow-x: auto` → horizontal scroll on ~360px phones;
  Template / Riwayat / Terbitkan unreachable without scrolling.
- `SaveBadge` text ("Draft disimpan pukul …") clips to "Draf t…" (IMG1) — no truncation.
- 9 controls in a 52px bar: back, panel toggle, brand, save badge, undo, redo,
  device segment, zoom, template, riwayat, fullscreen, lihat situs, publish.

**Fix:** collapse on mobile to `← Website | [project] | ✓ | ⋯` (overflow menu),
device segment icon-only below 720px, SaveBadge → icon + relative time
("✓ 2 mnt lalu"), `text-overflow: ellipsis` on the badge.

### 1.3 Misc responsive findings

- **`st-bar` at `top: -38px`:** on `pointer: coarse` the toolbar is always visible
  (globals.css:1273) — the first plugin at row 0 gets its grip clipped above the stage.
  Snap inside (`top: 4px`) for row-0 items or pad the stage.
- `.st-phone` fixed `min-height: 640px` is taller than many phone viewports inside the sheet.
- `st-tpl-thumb`/`st-mini` template previews hard-code `width: 1080px; scale(0.3)` —
  verify they don't overflow inside `.sx-scroll` on small screens.

---

## 2. Palette / Theme Contradictions (IMG2)

```ts
// plugins.ts:70
export const PALETTES: Palette[] = [
  { name: "Senja",  primary: "#C2410C", surface: "cream" },
  { name: "Indigo", primary: "#4F46E5", surface: "light" },
  { name: "Daun",   primary: "#15803D", surface: "light" },
  { name: "Laut",   primary: "#0369A1", surface: "light" },
  { name: "Mawar",  primary: "#BE185D", surface: "cream" },
  { name: "Kopi",   primary: "#92400E", surface: "cream" },
  { name: "Malam",  primary: "#A78BFA", surface: "dark" },
  { name: "Emas",   primary: "#CA8A04", surface: "dark" },
];
```

Swatches render as 135° split: `linear-gradient(var(--sw) 0 50%, var(--sf) 50%)`
(globals.css:1232), so all 8 *look* the same shape, but the model is only
`{primary, surface}` — the root cause of the contradictions:

- **`Malam` broken by construction.** Every other primary is a saturated dark; `#A78BFA`
  is a light pastel. On `dark` surface (`--pg-bg: #0d0d12`) it reads as a different visual
  weight than the light-surface themes. `Emas`'s `--s-tint` mix on dark will look muddy.
- **Hue collisions:** `Indigo` #4F46E5 vs `Malam` #A78BFA — both violet family, near
  duplicates at 28px swatch size. `Senja` #C2410C vs `Kopi` #92400E — ~15° apart,
  distinguishable only by the surface half.
- **Contrast:** light primary (#A78BFA) + white text likely fails WCAG AA; `Emas`/`Kopi`
  borderline for tint backgrounds.
- **Selection ring uses `var(--brand)`** (dashboard indigo) — that's the blue/purple ring
  around the green `Daun` swatch in IMG2. Should be neutral (`--ink` outline + check).

**Fix plan:**

1. Extend the token model:
   ```ts
   interface Palette { name: string; primary: string; onPrimary: string; surface: Surface }
   ```
2. Re-tune: `Malam` primary → deep violet (keep light lavender only for text roles);
   `Senja` → `#EA580C` (brighter) vs `Kopi` → `#78350F` (deeper) to separate them.
3. Neutral selection ring on `.st-swatch[aria-pressed="true"]`.
4. Add tests in `plugins.test.ts`: (a) min ΔL/hue distance between any two primaries,
   (b) `contrast(primary, onPrimary) ≥ 4.5`.
5. Schema impact: drafts persist `primary`/`surface` — bump doc schema when adding
   `onPrimary`; separate branch.
6. Add 10 new palettes (§2.1) with the same token model — all must pass the same tests.

---

## 2.1 New Palettes (10)

Same model as §2 Fix: `{ name, primary, onPrimary, surface }`. Chosen to keep ≥25° hue
distance from every existing palette (and each other), all pairs ≥4.5:1 contrast.

| # | Name | Primary | onPrimary | Surface | Slot filled / rationale |
|---|---|---|---|---|---|
| 9 | `Sawi` | `#65A30D` | `#FFFFFF` | light | Yellow-green — distinct from Daun's forest green |
| 10 | `Terong` | `#7E22CE` | `#FFFFFF` | light | Purple, darker than Malam's violet, separates from Indigo |
| 11 | `Bata` | `#9F1239` | `#FFFFFF` | cream | Deep maroon-brick; warm, distinct from Mawar's pink |
| 12 | `Sian` | `#0E7490` | `#FFFFFF` | light | Cyan-teal, between Laut (blue) and Daun (green) |
| 13 | `Baru` | `#B45309` | `#FFFFFF` | cream | Amber-bronze; brighter than Kopi, deeper than Senja |
| 14 | `Lumut` | `#3F6212` | `#F7FEE7` | dark | Olive on dark surface — first green-dark option |
| 15 | `Abu` | `#3F3F46` | `#FFFFFF` | light | Neutral zinc — "no theme" minimal look |
| 16 | `Merah` | `#DC2626` | `#FFFFFF` | light | True red; distance kept from Bata (dark) via light surface |
| 17 | `Toska` | `#0F766E` | `#FFFFFF` | cream | Teal on cream; warm-cool pairing distinct from Sian |
| 18 | `Koral` | `#E11D48` | `#FFFFFF` | light | Vibrant rose-red; hue between Mawar and Merah, distinct from both |

```ts
// appended to PALETTES (plugins.ts) — full entries
{ name: "Sawi",   primary: "#65A30D", onPrimary: "#FFFFFF", surface: "light" },
{ name: "Terong", primary: "#7E22CE", onPrimary: "#FFFFFF", surface: "light" },
{ name: "Bata",    primary: "#9F1239", onPrimary: "#FFFFFF", surface: "cream" },
{ name: "Sian",    primary: "#0E7490", onPrimary: "#FFFFFF", surface: "light" },
{ name: "Baru",    primary: "#B45309", onPrimary: "#FFFFFF", surface: "cream" },
{ name: "Lumut",   primary: "#3F6212", onPrimary: "#F7FEE7", surface: "dark" },
{ name: "Abu",     primary: "#3F3F46", onPrimary: "#FFFFFF", surface: "light" },
{ name: "Merah",   primary: "#DC2626", onPrimary: "#FFFFFF", surface: "light" },
{ name: "Toska",   primary: "#0F766E", onPrimary: "#FFFFFF", surface: "cream" },
{ name: "Koral",   primary: "#E11D48", onPrimary: "#FFFFFF", surface: "light" },
```

Notes:

- `.st-palettes` grid is `repeat(4, 1fr)` — 18 palettes = 4.5 rows; keep as-is (scrolls
  fine in the panel) or switch to `repeat(auto-fill, minmax(64px, 1fr))` if it gets tall.
- All 18 must pass the §2 test suite (hue distance + contrast). If any pair fails,
  adjust the newcomer, never the shipped 8.
- Naming stays Indonesian one-word, matching existing convention (Senja/Indigo/Daun…).
  `Baru` is a placeholder name — consider `Kuningan` (brass) or `Ambra` before shipping.

---

## 2.2 Navbar Plugin (placement: top / left / right / bottom)

New plugin kind `navbar` for the Studio grid, consistent with the ADR-0017 prototype
model (whitelisted components, closed data).

**A. Plugin definition** (`plugins.ts`):

```ts
{
  kind: "navbar", name: "Navigasi", category: "Konten",
  desc: "Menu tautan toko — bisa di atas, samping, atau bawah halaman.",
  w: 12, h: 2, minW: 4, minH: 2,
  layouts: [
    { value: "top",    label: "Atas" },
    { value: "left",   label: "Kiri" },
    { value: "right",  label: "Kanan" },
    { value: "bottom", label: "Bawah" },
  ],
  keywords: "menu navigasi nav header tautan link",
}
```

Layout picks: top = w:12 h:2 (horizontal bar), left = w:2 h:12+ (vertical),
right = w:2 h:12+ (vertical), bottom = w:12 h:2. The plugin system already supports
`layouts` (product_grid uses it), so this rides existing machinery.

**B. Data shape** (`EXTRA_DATA`, mirror of server `schema_v2.rs` once promoted):

```ts
navbar: {
  brand: "",                 // store name shown at nav start; empty = profile name
  links: [                   // max 6 (keep validator parity in mind)
    { label: "Beranda", target: "top" },       // target: "top" | section id | URL
    { label: "Produk",  target: "products" },
    { label: "Kontak",  target: "contact" },
  ],
  style: "plain" | "card" | "tint" | "dark",   // reuse CELL_STYLES, zero new CSS surface
  sticky: false,              // sticky within the page on the public site
}
```

**C. Editor canvas** — grid cells render as today (absolute, 12-col grid). The four
placements are just different starting `w/h` presets when the layout changes:

```ts
// onLayout("left") → resize cell to { w: 2, h: max(h, 8) } and re-place
// onLayout("top")  → { w: 12, h: 2 }
```

Existing `place()` / grid machinery handles overlap resolution. No canvas changes needed.

**D. Public-site rendering** — this is the part that needs new CSS:

- `top` / `bottom`: flex row, wraps; `sticky: true` → `position: sticky` + z-index above
  sections (mirror `.d-topbar` pattern).
- `left` / `right`: fixed sidebar on ≥900px public viewports
  (`position: fixed; inset-block: 0; width: 220px`), main content gets matching
  padding; on mobile public view collapses to a hamburger + sheet (reuse `Sheet`
  from `@/ui/dialog` — already used by studio).
- Active link tracking optional for step 1; keep it inert anchors (scroll-to-section
  via `target: "top"` / section ids).

**E. Style variants** — reuse `st-style-plain | card | tint | dark` classes already
defined for every plugin (globals.css:1275+). The navbar gets themed text via
`--s-ink`/`--s-primary` from `primaryVars()` — no new design tokens needed.

**F. Readiness + limits** (`readiness.ts`, `EXTRA_LIMITS`):

```ts
EXTRA_LIMITS.navbar = { brand: { max: 40 } };  // links validated structurally, not by text length alone
readiness: warn if navbar present but links reference a section id that doesn't exist.
```

**G. Scope guard for step 1:**

- One navbar per page (add to `SINGLE` list alongside chatbot/whatsapp — ADR-0019
  max_per_page parity).
- Links target only in-page sections ("top", section ids) in step 1; external URLs
  can come later with a URL validator.
- Public renderer (`renderer/StudioSite.tsx`) needs the biggest chunk of work:
  ~1 section component + sidebar CSS. Editor canvas is nearly free.
- Server validator (`schema_v2.rs`) doesn't know `navbar` yet — same prototype status
  as chatbot/text/whatsapp per page.tsx header comment; localStorage until ADR lands.

**Verify:** install Navbar → appears as 12×2 "Atas" by default; switch layout to
Kiri → cell becomes narrow vertical; drop to mobile preview (st-phone) → nav renders
as horizontal bar regardless of side placement (public mobile behavior); ready-check
flags a link to a missing section.

## 3. Animation Audit

### Design-system motion contract (globals.css:1–6)

> "Aturan motion: hanya transform/opacity; ≤ 250ms; ease-out kustom; tidak ada animasi
> untuk aksi keyboard berulang (⌘K, navigasi)."

Tokens (globals.css:75–81):

```css
--ease-out: cubic-bezier(0.23, 1, 0.32, 1);
--ease-drawer: cubic-bezier(0.32, 0.72, 0, 1);
--dur-fast: 120ms;  --dur: 180ms;  --dur-slow: 240ms;
```

Reduced-motion (globals.css:1047–1049) — **verified, it covers both animations and
transitions**, and `animation-delay: 0ms !important` is included (many implementations
forget the delay and elements sit invisible under `both` fill):

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.01ms !important;
    animation-delay: 0ms !important; transition-duration: 0.01ms !important; }
}
```

`src/renderer/site.css:147` and `src/shop/shop.css:113` carry their own reduced-motion
rules too — public-site and editor both covered. This is solid; no gap here.

### Inventory — Studio (`st-*` / `sx-*`, globals.css)

| Selector | Effect | Timing | Contract |
|---|---|---|---|
| `.st-item` | transform/width/height | `--dur-slow` (240ms) `--ease-out` | ⚠ exceeds "transform/opacity only" (animates width/height → layout) |
| `.st-item` | outline-color, box-shadow | `--dur-fast` (120ms) / `--dur` (180ms) | ⚠ box-shadow animates paint, not compositable |
| `.st-active` | `transition: none` | instant during drag | ✓ |
| `.st-slot, .st-ghost` | transform/width/height | 120ms `--ease-out` | ⚠ width/height |
| `.st-bar` | opacity + translateY(-2px) reveal | `--dur-fast` | ✓ |
| `.st-resize` | opacity reveal | `--dur-fast` | ✓ |
| `.st-plugins li` | border-color; `scale(0.98)` on `:active` | `--dur-fast` | ✓ good press feedback |
| `.st-tpl` | `translateY(-3px)` hover, `scale(0.99)` active, `fadeIn` entrance | `--dur` / `--dur-slow` | ✓ |
| `.st-float-panel` / `.st-chatbar-panel` | `d-pop-in` (opacity 0→1, scale 0.97→1, translateY) | **200ms** `--ease-out` `both` | ✓ transform+opacity |
| `.st-float-bubble` | `scale(0.94)` on `:active` | `--dur-fast` | ✓ |
| `.st-wa-float` | `scale(0.96)` on `:active` | 160ms | ✓ |
| `.st-tool` | `scale(0.92)` on `:active` | `--dur-fast` | ✓ |
| `.s-edit` | outline-color / background | 120ms ease | ✓ |
| `.d-pop-in` (menus, globals.css:779) | opacity/scale/translateY | 150ms `--ease-out` `both` | ✓ |
| `.d-page` (globals.css:374) | `d-page-in` | `--dur-slow` | ✓ |
| `.d-stagger > *` (globals.css:1040) | `fadeIn` with 30ms-per-child delay, capped 150ms at n+6 | `--dur-slow` | ✓ |
| Drawer (globals.css:348, 817) | transform | `--dur-slow` `--ease-drawer` + `display/overlay allow-discrete` | ✓ modern transition-of-display pattern |

### Findings

1. **Contract violation: `width`/`height` transitions on `.st-item` (240ms).** The file's
   own header rule says "hanya transform/opacity" (layout-affecting properties cause
   reflow every frame), yet the canvas animates width/height — the most layout-expensive
   path there is, run on *every grid reflow during drag*. With `touch-action: none` and
   pointer-driven updates this is the single most likely source of jank on low-end
   Android. Options: (a) accept for resize (rare, intentional) but drop to `--dur-fast`
   for reflow; (b) animate `transform: scale()` visually and commit layout at end.
   At minimum:
   ```css
   .st-dragging .st-item:not(.st-active) { transition-duration: 120ms; }
   ```
   Displaced items currently reflow at 240ms while the drag ghost updates at 120ms —
   the grid feels rubbery/laggy behind the finger.
2. **`box-shadow` transition on `.st-item` (180ms)** — paint-only, cheap-ish, but it runs
   on every hover of every cell. Fine on desktop; consider `@media (pointer: fine)` only.
3. **`d-pop-in` durations are inconsistent:** 150ms for menus, 200ms for the chat
   panels. Two entrances of the same "pop" family should share one duration — pick 150ms.
4. **Chat panel toggle animates *in* but never *out*.** `st-chat` height is fixed 420px;
   toggling the chatbot on/off pops with `d-pop-in ... both` on open and just vanishes
   on close (no exit animation, no height transition). Asymmetric — either animate both
   ways (height on the wrapper, transform on the bubble) or neither; the current mix
   reads as a glitch on close. This is also the same z-30 bubble that overlaps the
   mobile panel (§1.1).
5. **No entrance animation for the ≤900px fixed panel** — it appears instantly over the
   canvas. When converting to a bottom sheet (§1.1), add `translateY(100%) → 0` at
   `--dur-slow --ease-drawer` (the token already exists and is used for exactly this
   pattern at globals.css:817) + scrim fade ~150ms. Reduced-motion is already handled
   globally, so no extra work there.
6. **`.st-drop-hint` `backdrop-filter: blur(1px)` (globals.css:1343)** — 1px blur is
   imperceptible but backdrop-filter forces an offscreen composite; on low-end Android
   it's real cost for zero effect. Either raise to 4px (actual frosted look) or delete
   the property.
7. **`fadeIn` on `.st-tpl` template cards is un-staggered** — all cards animate
   simultaneously on a gallery that can hold 6+ cards. The `d-stagger` utility
   (globals.css:1040) already exists in this codebase and does exactly this with capped
   delays. Reuse it: `className="st-tpls d-stagger"`-style application on the gallery
   container (note `d-stagger` targets `> *`, so it applies cleanly to `.st-tpls`).
8. **Motion contract is otherwise well kept:** durations all ≤ 250ms as documented,
   `--ease-out` used consistently, no spring/bounce chaos, keyboard actions unanimated
   as the header rule requires, press-scale feedback (`0.92`–`0.98`) is present on every
   touchable — good tactile grounding, keep it.

### Verdict

The motion system is genuinely disciplined — one ease family, three durations, documented
rules, correct reduced-motion handling including animation-delay. The real issues are:
(1) width/height + box-shadow transitions on the drag hot path violating the project's
own transform/opacity-only contract, (2) drag reflow timing mismatch (240ms vs 120ms),
(3) asymmetric chat panel open/close, (4) the missing bottom-sheet entrance once §1.1
lands, and (5) the 1px backdrop blur. All are small, contained CSS edits.

---

## 4. Fix Plan — Step 1

Every fix below is scoped, has a file target, and can be verified. Ordering = priority.

### Fix 1.1 — Mobile panel → bottom sheet + scrim + close (IMG1)

**Files:** `globals.css` (≤900px block, ~line 1389), `page.tsx` (scrim JSX + close button).

a) Replace the fixed left panel with a bottom sheet:

```css
@media (max-width: 900px) {
  .sx-body { grid-template-columns: minmax(0, 1fr); }
  .sx-panel {
    position: fixed; left: 0; right: 0; bottom: 0; top: auto;
    max-height: 70vh; overflow-y: auto;
    border-radius: 16px 16px 0 0; border: 0; border-top: 1px solid var(--line);
    box-shadow: var(--shadow-dialog);
    z-index: 40;
    transform: translateY(100%);
    transition: transform var(--dur-slow) var(--ease-drawer);
  }
  .sx-body[data-panel="open"] .sx-panel { transform: translateY(0); }
  .sx-scrim {
    position: fixed; inset: 52px 0 0 0; z-index: 35;
    background: rgb(0 0 0 / 0.4);
    opacity: 0; pointer-events: none;
    transition: opacity var(--dur-fast) ease;
  }
  .sx-body[data-panel="open"] .sx-scrim { opacity: 1; pointer-events: auto; }
}
```

b) Kill the FAB-over-panel overlap (the IMG1 bug) — `.st-float` is z-30, panel now z-40
with scrim z-35, so the bubble no longer rides on top; belt-and-braces:

```css
@media (max-width: 900px) {
  .sx-body[data-panel="open"] .st-dock { display: none; }
}
```

c) In `page.tsx`: render `<div className="sx-scrim" onClick={() => setPanelOpen(false)} />`
inside `.sx-body` (always rendered, visibility CSS-gated), and add an in-panel header row
with an `X` close button (reuse `d-icon-btn`) shown only ≤900px (CSS-gate a
`.sx-panel-close` class). The existing `SidebarSimple` toggle stays for desktop.

**Verify:** open studio on 360px viewport → panel slides up from bottom, canvas fully
visible behind scrim, tap scrim closes, chat bubble hidden while open, `X` works.
Reduced-motion: covered globally (globals.css:1047).

---

### Fix 1.2 — `sx-top` mobile collapse + SaveBadge truncation (IMG1)

**Files:** `page.tsx` (toolbar JSX), `globals.css` (new `sx-top` mobile rules).

a) CSS: collapse the toolbar below 720px.

```css
@media (max-width: 719px) {
  .sx-top { overflow-x: visible; }
  .sx-brand { min-width: 0; }
  .sx-brand, .sx-brand > :not(.d-avatar) { /* keep avatar, truncate the rest */
    max-width: 40vw; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: inline-block;
  }
  .sx-top .d-seg button span-label { display: none; } /* if labels are wrapped */
  .sx-save { max-width: 90px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sx-more { display: inline-flex; } /* overflow menu button, hidden ≥720px */
}
.sx-more { display: none; }
```

b) JSX: move `Template`, `Riwayat`, `Lihat situs`, `Fullscreen` into a `⋮` overflow
(`d-menu`/popover pattern already exists for menus — globals.css:779 uses it). Keep
visible on mobile: back, panel toggle, brand, save badge, undo/redo, device toggle
(icon-only), publish.

c) `SaveBadge` (page.tsx:~712): shorten label to `✓` + relative time
(`waktu()` from `@/lib/labels` already exists) — "✓ 2 mnt lalu" instead of
"Draft disimpan pukul 20.15" — and let CSS truncate as backstop.

**Verify:** no horizontal scrollbar on `sx-top` at 360px; publish reachable in one tap;
badge never clips mid-word.

---

### Fix 1.3 — `st-bar` clipping at row 0 (coarse pointers)

**Files:** `globals.css` (~line 1255 + 1273).

The toolbar renders at `top: -38px` outside the item; the first plugin on the canvas
(esp. with `pointer: coarse` forcing it visible) gets clipped by the stage edge.

```css
/* Toolbar of the topmost row flips inside the item instead of above it */
.st-item:first-child .st-bar, .st-item[data-row="0"] .st-bar {
  top: 4px; height: 32px; padding-bottom: 0; align-items: center;
}
```

Simpler alternative if per-row class isn't wanted: give `.st-stage`/`.sx-canvas`
`padding-top: 44px` under `pointer: coarse`. Pick one; test on the template with a
locked hero at y=0.

**Verify:** on touch device, topmost plugin's grip ("Banner 12/12") fully visible and
tappable.

---

### Fix 2 — Palette tokens + re-tune + neutral ring (IMG2)

**Files:** `src/studio/plugins.ts`, `src/renderer/safe.ts`, `src/studio/plugins.test.ts`,
`globals.css` (swatch selected state).

a) Token model (plugins.ts):

```ts
export interface Palette {
  name: string; primary: string; onPrimary: string;
  surface: "light" | "cream" | "dark";
}
export const PALETTES: Palette[] = [
  { name: "Senja",  primary: "#EA580C", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Indigo", primary: "#4F46E5", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Daun",   primary: "#15803D", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Laut",   primary: "#0369A1", onPrimary: "#FFFFFF", surface: "light" },
  { name: "Mawar",  primary: "#BE185D", onPrimary: "#FFFFFF", surface: "cream" },
  { name: "Kopi",   primary: "#78350F", onPrimary: "#FEF3C7", surface: "cream" },
  { name: "Malam",  primary: "#7C3AED", onPrimary: "#FFFFFF", surface: "dark" },
  { name: "Emas",   primary: "#CA8A04", onPrimary: "#1C1917", surface: "dark" },
];
```

Changes vs current: `Senja` #C2410C→#EA580C (separates from Kopi), `Kopi` #92400E→#78350F
(deeper) with cream text, `Malam` #A78BFA→#7C3AED (saturated violet — same weight class
as the others; the old pastel only survives as a *link/text* color in dark mode, not a
button fill), `Emas` keeps gold but flips `onPrimary` to near-black (white on #CA8A04
fails AA at ~2.2:1).

b) `primaryVars()` in `renderer/safe.ts`: derive `--s-on-primary` from the token instead
of hard-coding white.

c) Neutral selection ring (globals.css:1234):

```css
.st-swatch[aria-pressed="true"] { border-color: var(--line-strong); box-shadow: 0 0 0 2px var(--ink); }
```

Optionally add a check mark (`.st-swatch[aria-pressed="true"]::after`) so selection is
visible without relying on ring hue at all.

d) Tests (`plugins.test.ts`):

```ts
// pseudo
for (const [a, b] of pairs(PALETTES)) {
  expect(deltaHueOrL(a.primary, b.primary)).toBeGreaterThanOrEqual(25); // hue° or ΔL
}
for (const p of PALETTES) {
  expect(wcagContrast(p.primary, p.onPrimary)).toBeGreaterThanOrEqual(4.5);
}
```

(Implement small local helpers — no new deps.)

e) **Schema note:** drafts persist `primary`/`surface` via `toDoc`. Adding `onPrimary`
needs either a schema bump + server validator update (schema_v2.rs parity) or deriving
it client-side from primary luminance for legacy docs. Do the derive-on-read fallback
in `doc.ts` `fromDoc` for step 1; full token persistence can wait for ADR sign-off.
Same caveat as the header comment in `page.tsx`: layout is prototype-stage.

**Verify:** 8 swatches visibly distinct at 28px; Malam buttons readable on dark surface;
selected swatch shows neutral ring (no blue-on-green); `npm test` passes new checks.

---

### Fix 3 — Animation fixes (all `globals.css`, one commit)

a) **Drag hot path** — cap displaced-item reflow to match the ghost:

```css
.st-dragging .st-item:not(.st-active) { transition-duration: 120ms; }
```

b) **Unify `d-pop-in`** — chat panels 200ms → 150ms (lines 1326, 1418) to match menus.

c) **Symmetric chat close** — add exit for the chat panel toggle. Cheapest version:

```css
@keyframes d-pop-out { to { opacity: 0; transform: scale(0.97) translateY(2px); } }
/* applied via a closing class toggled in ChatPreview before unmount */
```

If timing the class is too fiddly for step 1, acceptable fallback: remove the *entrance*
animation on toggle (keep it on first open) so open/close are both instant — symmetry
matters more than the pop.

d) **Sheet entrance** — covered by Fix 1.1's `transform`/scrim rules (uses existing
`--ease-drawer` + `allow-discrete` pattern from globals.css:817).

e) **Drop `backdrop-filter: blur(1px)`** from `.st-drop-hint` (line 1343). Either delete
outright or `blur(4px)` — recommend delete for step 1.

f) **Stagger template gallery** — in `page.tsx`, `TemplateGallery`'s container:
add `d-stagger` to the `.st-tpls` element's className. Zero new CSS; the utility
(globals.css:1040) handles delays and capping.

**Verify:** drag a plugin — grid settles in ~120ms, no rubber-band lag; toggle chatbot
open/close symmetric; template gallery cards cascade in; reduced-motion still neutral
(EMULATE prefers-reduced-motion in devtools).

---

### Order of execution

1. **Fix 1.1** (sheet + scrim + z-index) — the "super awkward" complaint, highest impact.
2. **Fix 1.2** (toolbar collapse) — same viewport, do together with 1.1 to test once.
3. **Fix 3** (animation CSS) — pure CSS, no logic risk, ride along with 1.1/1.2 commit.
4. **Fix 1.3** (`st-bar` row-0) — quick, verify on touch.
5. **Fix 2** (palette) — token + schema-adjacent; own commit/branch because of the
   `doc.ts` fallback logic and new tests.

Estimated effort: 1+2+3 ≈ one sitting of CSS/JSX + manual QA at 360px/768px/1440px;
2 (palette) ≈ another sitting incl. tests.

---

## 5. Implementation Log

Status of each item as it lands. One task per commit, per agreed workflow.

### Done — Commit `55fa23b`: §2.1 + §2 palettes (partial)

- ✅ **§2.1 — 10 new palettes** added to `PALETTES` (plugins.ts): Sawi, Terong, Bata,
  Sian, Kuningan, Lumut, Abu, Merah, Toska, Koral.
- ✅ **§2 token model** — `Palette` now carries `onPrimary`; interface updated.
- ✅ **§2 tests** — `contrast.test.ts` gained the `onPrimary ≥ 4.5:1` assertion.
- ✅ Swatch forwards `--s-on` to CSS (page.tsx).
- ⚠️ **Deviations from spec (caught by the new test — the test doing its job):**
  - `Senja` spec'd `#EA580C` → actually 3.56:1 with white. **Kept `#C2410C`** (original).
    The §2 "separate from Kopi" concern is instead addressed by Kopi deepening to
    `#78350F` + cream text, which increases the pair distance more than brightening
    Senja would.
  - `Sawi` spec'd `#65A30D` → 3.09:1. **Shipped `#4D7C0F`** (deeper lime, AA-passing).
  - `Baru` placeholder name → **shipped as `Kuningan`** as suggested in the notes.
- ⚠️ **Known pre-existing failure (not ours):** `templates.contract.test.ts` "fixture
  sama dengan keluaran template" fails on clean master too (verified via stash).
  Do not chase it inside step-1 work.
- 📋 **Still open from this commit's scope:**
  - `.st-swatch span` gradient doesn't *use* `--s-on` yet (only forwarded). Next CSS
    pass should show the true text color on the swatch, e.g. an inner dot/pill:
    `background: linear-gradient(135deg, var(--sw) 0 50%, var(--sf) 50%)` stays, plus
    a small centered chip `background: var(--sw); color: var(--s-on)`.
  - Neutral selection ring (§2 Fix plan item 3) — `.st-swatch[aria-pressed="true"]`
    still uses `var(--brand)`.
  - `doc.ts` `onPrimary` derive-on-read fallback for legacy drafts (§2 item 5 / Fix 2e).
  - Hue-distance test (min ΔL/hue between any two primaries) — only the contrast half
    of the test suite was added; hue test still to write.

### Done — Follow-up commit: sisa lingkup palet (Fix 2e dst.)

- ✅ **Swatch "Aa" chip** — `.st-swatch span::after` kini menampilkan pil kecil dengan
  `background: var(--sw); color: var(--s-on)` sehingga warna teks palet terlihat nyata.
- ✅ **Cincin seleksi netral** — `.st-swatch[aria-pressed="true"]` pakai
  `var(--line-strong)` + ring `var(--ink)` (bukan `var(--brand)`).
- ✅ **`doc.ts` fallback derive-on-read** — `StudioState.onPrimary?` + `onPrimaryOf()`:
  cocokkan `PALETTES`, selain itu `onColor()`. Tidak menyentuh bentuk dokumen server.
- ✅ **Test jarak rona** — `plugins.test.ts`: dua palet kromatik tidak boleh
  Δhue<12° *sekaligus* ΔL<15 (Abu dikecualikan, s≈5).
- ⚠️ **Re-tune tambahan yang dipaksa test jarak rona** (semua tetap AA ≥4.5:1):
  - `Kopi` #78350F→**#5C2A0D** — Senja↔Kopi dulu 4°/ΔL14 (temuan audit asli!); kini ΔL≈19.
  - `Kuningan` #B45309→**#7A7008** — pertama bentrok Senja (8°), lalu bentrok Emas
    (5°); kuningan/brass sejati (hue 55) menyelesaikan keduanya.
  - `Laut` #0369A1→**#075C9E**, `Sian` #0E7490→**#0A6F82** — dulu 8° satu sama lain;
    kini Laut–Sian 16°, Sian–Toska 15°.
  - `Mawar` #BE185D→**#A61D6D** — Mawar↔Bata dulu 8°; kini 18°.
  - `Terong` #7E22CE→**#7C1FA0** — Terong↔Malam dulu 10°/ΔL11; kini 21°.
  - `Sawi` #4D7C0F→**#33850C** — Sawi↔Lumut dulu 0°/ΔL5; kini 15°.
- Verifikasi: `tsc --noEmit` bersih; plugins/contrast/doc tests 11/11; full suite
  73/74 (1 gagal = `templates.contract` yang sudah gagal di master bersih).

### Next up

1. §2.2 Navbar plugin (A–G) — editor canvas first, then public renderer.
2. Fix 1.1 → 1.2 → Fix 3 (one CSS sitting), then Fix 1.3.

