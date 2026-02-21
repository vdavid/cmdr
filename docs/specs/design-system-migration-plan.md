# Design system migration plan

Migrate the desktop app and website to the unified design system defined in `/.interface-design/system.md`.

## Context

The app and website have divergent design languages — different accent colors, neutral palettes, fonts, and no shared
component patterns. The app has ~10 dialogs that each reimplement button/input styling with slight inconsistencies.
This plan brings both surfaces under one coherent system.

## Milestone 1a: Add new design tokens

Add tokens to `apps/desktop/src/app.css` without renaming or removing anything. Zero breakage — existing code keeps
working, new tokens are available for use.

**New token groups:**
- `--radius-sm: 4px`, `--radius-md: 6px`, `--radius-lg: 8px`, `--radius-full: 9999px`
- `--shadow-sm`, `--shadow-md`, `--shadow-lg`, `--shadow-focus` (with separate light/dark values per system.md)
- `--transition-fast: 100ms ease`, `--transition-base: 150ms ease`, `--transition-slow: 200ms ease`

**New z-index tokens:**
- `--z-base: 0`, `--z-sticky: 10`, `--z-dropdown: 100`, `--z-overlay: 200`, `--z-modal: 300`,
  `--z-notification: 400`

**New color tokens:**
- `--color-accent-subtle` as `color-mix(in oklch, var(--color-accent), transparent 85%)`
- `--color-border-subtle` (light: `#e8e8e8`, dark: `#333333`)

**New spacing tokens (additive — don't touch existing `--spacing-md`):**
- `--spacing-lg: 16px`
- `--spacing-xl: 24px`
- `--spacing-2xl: 32px`

**New font-size tokens (additive — don't touch existing `--font-size-xs` or `--font-size-sm`):**
- `--font-size-md: 14px`
- `--font-size-lg: 16px`
- `--font-size-xl: 20px`

**Accent color fallback fix:**
- Change `--color-accent` from `#0078d4` (Windows blue) to `#007aff` (macOS default blue) in light mode
- Change from `#4da3ff` to `#0a84ff` in dark mode
- Update `--color-accent-hover` to use `color-mix(in oklch, var(--color-accent), white 15%)` (light) /
  `white 10%` (dark) — replacing the hardcoded `#005a9e`/`#6eb5ff`

## Milestone 1b: Rename and remove tokens

Now that new tokens exist, migrate consumers and remove old tokens. Each rename is a find-and-replace across the
codebase followed by a check run.

**Border naming (rename + value adjustment):**
- Rename `--color-border-primary` → `--color-border-strong` in app.css and all 22 consuming files (~56 occurrences)
- Rename `--color-border-secondary` → `--color-border-subtle` in app.css and consuming files (map to the token added
  in M1a)
- **Values also change** — not just a rename. The system adjusts border contrast for better hierarchy:
  - Strong: `#ccc` → `#bbb` (light), `#444` → `#555` (dark) — stronger emphasis
  - Subtle: `#e0e0e0` → `#e8e8e8` (light), `#3a3a3a` → `#333` (dark) — lighter touch
  - Default stays `#ddd` (light) but swaps from `#555` → `#444` (dark) — the current dark mode had default (#555)
    lighter than strong (#444), which was backwards. The system fixes this.

**Text color changes:**
- Update `--color-text-primary` from `#000000` → `#1a1a1a` (light) and `#ffffff` → `#e8e8e8` (dark)
- This is a global change affecting every text element — take before/after screenshots of the file list, settings, and
  a dialog for side-by-side comparison
- Consolidate `--color-text-muted` → `--color-text-tertiary` across 24 files (~58 occurrences), then remove
  `--color-text-muted` from app.css

**Font-size cleanup:**
- Current state: `--font-size-xs: 12px` and `--font-size-sm: 12px` are identical
- Change `--font-size-xs` to `10px` (the system's intended value)
- **This is high-risk** — every current `--font-size-xs` consumer was written expecting 12px. 10px is genuinely small.
  Audit each consumer individually: grep for `--font-size-xs`, list every file, and for each usage decide whether 10px
  is appropriate or the consumer should switch to `--font-size-sm` (12px). Expected volume: check how many usages exist
  before starting. Take screenshots of affected components at 10px to verify legibility
- Remove `--font-size-base: 16px` — the revised system drops this token name (the 5-step scale is xs/sm/md/lg/xl).
  The body element (`font-size: var(--font-size-base)`) should just inherit from `html { font-size: 16px }` — remove
  the `font-size` declaration from the `html, body` rule

**Font stack cleanup:**
- Remove `'Segoe UI'` from `--font-system` in app.css (Windows font, Cmdr is macOS-only)

**Spacing-md safe migration (critical — do in this exact order):**
1. `--spacing-lg: 16px` already added in M1a
2. Grep every `--spacing-md` usage. For each, decide: does this want 12px (component internal padding) or 16px (section
   padding)? Migrate the 16px-intent ones to `--spacing-lg`
3. Only after all 16px-intent consumers are migrated, redefine `--spacing-md` from `16px` to `12px`

**Verification:** Run `./scripts/check.sh --svelte`. Open app in light + dark mode, visually compare main file list,
settings, and one dialog. For the `--spacing-md` change specifically, screenshot before and after.

## Milestone 2a: macOS accent color — read and inject

Read the user's system accent color on the Rust side and inject it into the webview. Keep existing cursor tokens as
fallback until verified.

**Rust side** (`src-tauri/`):
- Enable the `NSColor` feature on `objc2-app-kit` in `Cargo.toml`
- Create a Tauri command `get_accent_color()` that calls `NSColor::controlAccentColor()`, extracts RGB components, and
  returns a hex string
- Observe `NSSystemColorsDidChangeNotification` and emit a Tauri event `accent-color-changed` with the new hex value

**Frontend side:**
- On app startup (in `settings-applier.ts` or similar init path), call the Tauri command and set
  `--color-accent` on `document.documentElement`
- Derive `--color-accent-hover` and `--color-accent-subtle` via `color-mix()` in CSS (no JS needed for derivation)
- Listen for the `accent-color-changed` event and update `--color-accent` live
- Fallback: if the command fails (or on unsupported macOS versions), keep the CSS default from `app.css`
  (`#007aff` light / `#0a84ff` dark). Log the failure at `warn` level

**Verification:** Run `./scripts/check.sh --rust`. Manual test: change macOS accent color in System Settings while the
app is running — verify `--color-accent` updates live. Test with all 8 macOS accent options (blue, purple, pink, red,
orange, yellow, green, graphite). Pay special attention to yellow and graphite on light backgrounds — these have the
lowest contrast against white.

**Rollback:** If accent reading is unreliable, the CSS fallback in `app.css` still works. The old cursor tokens still
exist at this point, so the app looks the same as before.

## Milestone 2b: macOS accent color — migrate consumers

Now that accent injection is verified, migrate cursor and interactive tokens to use it.

- Update cursor highlight in file list to use `--color-accent-subtle` instead of hardcoded blue
- Remove `--color-cursor-focused-bg` and `--color-cursor-focused-fg` — the accent-subtle tint is translucent, so text
  keeps its normal color (no need for a special foreground token)
- Replace `--color-cursor-unfocused-bg` with `var(--color-bg-tertiary)` in all 6 consuming components (FullList,
  BriefList, ShareBrowser, NetworkBrowser, PaneResizer, CommandPalette)

**Verification:** Manual test with all 8 macOS accent colors. Verify cursor highlight, buttons, focus rings, and
switches all follow the accent. Specifically check that yellow accent cursor is visible on `--color-bg-primary` in light
mode.

## Milestone 3: Shared button component

Extract a reusable `Button.svelte` from the repeated patterns across dialogs.

**Component API:**
```svelte
<Button variant="primary|secondary|danger" size="regular|mini" disabled={false}>
  Label
</Button>
```

**Implementation:**
- Create `apps/desktop/src/lib/ui/Button.svelte`
- Implement the 4 variants (primary, secondary, danger, mini) per the design system spec
- Standardize hover to explicit `--color-accent-hover` (remove all `brightness()` filter usage)
- Standardize padding to `7px 20px` (regular) / `3px 12px` (mini)
- Include focus-visible two-layer ring
- Include disabled state (`opacity: 0.4; pointer-events: none`)
- Add to `coverage-allowlist.json` if it depends on DOM/Tauri

**Verification:** Add Vitest tests for variant rendering. Visual spot-check in a dialog.

## Milestone 4a: Mechanical token replacements

Search-and-replace hardcoded values with design tokens. These are safe because the token values match the hardcoded
values they replace.

**Border-radius (~123 occurrences):**
- Replace `border-radius: 3px` and `border-radius: 4px` → `var(--radius-sm)` (4px)
- Replace `border-radius: 6px` → `var(--radius-md)`
- Replace `border-radius: 8px` → `var(--radius-lg)`
- Replace `border-radius: 50%` → `var(--radius-full)`
- Note: `12px`, `10px`, `16px` radius values are outliers — review case by case

**Box shadows (~28 occurrences):**
- Replace `0 1px 3px rgba(0, 0, 0, 0.2)` → `var(--shadow-sm)`
- Replace `0 4px 12px rgba(0, 0, 0, 0.15)` → `var(--shadow-md)`
- Replace `0 16px 48px rgba(0, 0, 0, 0.4)` → `var(--shadow-lg)`
- Replace `0 0 0 2px rgba(77, 163, 255, 0.2)` → `var(--shadow-focus)`
- Note: some shadows have slight value differences from the tokens — decide whether to align or keep

**Transitions (~37 occurrences):**
- Replace `0.15s ease` / `150ms ease` → `var(--transition-base)`
- Replace `0.1s` / `100ms` → `var(--transition-fast)`
- Replace `0.2s` / `200ms` → `var(--transition-slow)`

**Verification:** Run `./scripts/check.sh --svelte`. Open app, visually spot-check file list, settings, and one dialog
in both light and dark mode.

## Milestone 4b: Spacing and font-size migration

Migrate off-scale values. These require judgment calls, not just search-and-replace.

**6px spacing (~9 direct instances):**
- For each `6px` usage, decide: should this be `var(--spacing-xs)` (4px) or `var(--spacing-sm)` (8px)?
- `padding: 2px 6px` (badges) → likely `2px var(--spacing-xs)` (2px 4px)
- `padding: 6px 12px` (breadcrumb/dialog buttons) → `var(--spacing-sm) var(--spacing-md)` (8px 12px)
- `gap: 6px` → `var(--spacing-xs)` (4px) or `var(--spacing-sm)` (8px) depending on context
- `width: 6px` / `height: 6px` (scrollbar/divider sizing) → keep as-is (these are element dimensions, not spacing)

**Font-size consolidation:**
- Replace hardcoded `11px` → `var(--font-size-sm)` (12px)
- Replace hardcoded `18px` → `var(--font-size-lg)` (16px) or `var(--font-size-xl)` (20px)
- Replace remaining hardcoded font-size values with `var(--font-size-*)` tokens

**Remaining hardcoded spacing:**
- Replace hardcoded padding/margin/gap values with `var(--spacing-*)` tokens where they align with the scale

**Verification:** Run checks. Visual spot-check — pay attention to spacing changes since 6px → 4px or 8px is
perceptible.

## Milestone 4c: Button rollout, dialog restyle, and color cleanup

Roll out the `<Button>` component across all dialogs and clean up hardcoded colors.

**Button component rollout (9 dialogs + FullDiskAccessPrompt):**
- AlertDialog, FullDiskAccessPrompt, LicenseKeyDialog, NewFolderDialog, TransferDialog, RenameConflictDialog,
  CommercialReminderModal, ExpirationModal, PtpCameraDialog
- Replace inline button styles with `<Button variant="primary|secondary|danger">`
- For FullDiskAccessPrompt specifically: wrap in `ModalDialog.svelte` (currently a raw fixed-position overlay), use
  standardized body padding (`0 24px 24px`), max width 480px

**Hover token removal (9 files, ~17 occurrences):**
- Remove `--color-button-hover` and `--color-bg-hover` from app.css
- In consuming components (ModalDialog, Notification, DualPaneExplorer, SortableHeader, ShareBrowser, NetworkBrowser,
  viewer page, and settings components): replace with `--color-accent-subtle` for interactive hovers,
  `--color-bg-tertiary` for neutral hovers

**Hardcoded color cleanup:**
- Replace `#4caf50` (rename validation green) → `var(--color-allow)`
- Replace `#ff9e1b` / `#a13200` (LoadingIcon) → define `--color-loading-icon` or keep as-is (animation-specific)
- Replace hardcoded focus ring `rgba(77, 163, 255, ...)` → `var(--shadow-focus)` (~4 occurrences)
- Replace stray `#ffffff` in notifications → `var(--color-text-primary)` or `white` (on accent backgrounds)

**Verification:** Run `./scripts/check.sh --svelte`. Manual test: open every dialog, verify button styles, focus rings,
hover states. Test FullDiskAccessPrompt in all states (first prompt, revoked, granted).

## Milestone 5: Website font swap

Replace Inter with Geist Sans.

- Download Geist Sans variable woff2 from Vercel's GitHub releases
- Replace `inter-latin-variable.woff2` with Geist Sans in `apps/website/public/fonts/`
- Update `@font-face` in `global.css` to reference Geist Sans
- Update `--font-sans` in `@theme` block
- Update OG image font files (`inter-400.ttf`, `inter-700.ttf`) with Geist Sans equivalents
- Update OG image generation code to reference new font files

**Verification:** Visual check all website pages for layout shifts from the font change.

## Milestone 6a: Website dark palette warmup

Warm up the dark palette and refine interactions. No layout or mode changes — just token value updates.

**Dark palette update:**
- Change `--color-background` from `#0a0a0b` → `#14130f`
- Update `--color-surface` → `#1a1917`
- Update `--color-surface-elevated` → `#222120`
- Update `--color-border` → `#2e2d2a`
- Update `--color-border-subtle` → `#24231f`
- Update `meta theme-color` in `Layout.astro`
- Update Remark42 comments background to match (if applicable)

**CTA hover refinement:**
- Change `hover:scale-105` → `hover:scale-[1.02]` on primary CTAs
- Keep glow effect on primary CTA only

**Reduced motion:**
- Verify hero animation degrades to opacity-only under `prefers-reduced-motion`
- Check all `transition-all` usages respect the media query

**Verification:** Run `./scripts/check.sh --check website-build --check website-e2e`. Visual check all website pages in
dark mode.

## Milestone 6b: Website light mode for sub-pages

Add light mode support. This is the most visually impactful website change — every component on affected pages needs to
render correctly in both modes.

**Light mode tokens:**
- Add `@media (prefers-color-scheme: light)` block to `global.css` with light tokens per system.md
- Scope it to sub-page layouts: LegalLayout, BlogLayout, and individual pages (pricing, changelog, roadmap)
- Landing page (`index.astro`) stays dark-only — explicitly override to keep dark tokens

**Blog-specific handling:**
- Blog code blocks: wrap in a `[data-theme="dark"]` scope so they keep dark tokens in light mode
- Update blog-prose.css to reference theme-aware tokens
- Update Shiki syntax highlighting: add `github-light` theme alongside `github-dark`, switch based on color scheme

**Third-party integration:**
- Update Remark42 comments to detect and pass `theme: 'light'` or `'dark'`

**Verification:** Run `./scripts/check.sh --check website-build --check website-e2e`. Visual check every sub-page (blog
index, blog post, legal, pricing, changelog, roadmap) in both light and dark mode. Verify blog code blocks stay dark in
light mode. Verify landing page stays dark regardless of system preference.

## Milestone 7: Empty state and polish

- Add "Empty folder" message to file list when directory has no entries
- Centered, single line, `--color-text-tertiary`, `--font-size-sm`
- Run full check suite (`./scripts/check.sh`)
- Manual visual check: open the app in light mode, dark mode, with different macOS accent colors
- Manual visual check: browse all website pages in light and dark mode
- Update `apps/desktop/src/app.css` header comment to reference `/.interface-design/system.md`
- Remove all "Migration note" annotations from `/.interface-design/system.md` — it should be a clean reference

## Future: System font size support

Not part of this migration, but tracked as a future effort.

- Research how WKWebView responds to macOS Accessibility > Display > Text size
- Determine if/how to make chrome (settings, dialogs, buttons) follow system text size preferences
- Pane text would remain density-controlled, compact relative to chrome
- May require switching from fixed `px` to relative units for chrome elements
- See the "Future: system font size" note in `/.interface-design/system.md`

---

## Task list

### Milestone 1a: Add new design tokens
- [ ] Add radius tokens (`--radius-sm/md/lg/full`) to app.css
- [ ] Add shadow tokens (`--shadow-sm/md/lg/focus`) with light/dark values
- [ ] Add transition tokens (`--transition-fast/base/slow`)
- [ ] Add z-index tokens (`--z-base/sticky/dropdown/overlay/modal/notification`)
- [ ] Add `--color-accent-subtle`, `--color-border-subtle`
- [ ] Add `--spacing-lg: 16px`, `--spacing-xl: 24px`, `--spacing-2xl: 32px`
- [ ] Add `--font-size-md: 14px`, `--font-size-lg: 16px`, `--font-size-xl: 20px`
- [ ] Update accent fallback to macOS blue (`#007aff`/`#0a84ff`), derive `--color-accent-hover` via `color-mix`
- [ ] Run `./scripts/check.sh --svelte` and fix issues

### Milestone 1b: Rename and remove tokens
- [ ] Rename `--color-border-primary` → `--color-border-strong` in app.css and all components
- [ ] Rename `--color-border-secondary` → `--color-border-subtle` across components
- [ ] Update `--color-text-primary` to `#1a1a1a`/`#e8e8e8` — take before/after screenshots for comparison
- [ ] Consolidate `--color-text-muted` → `--color-text-tertiary`, remove `--color-text-muted`
- [ ] Change `--font-size-xs` to `10px` — audit each consumer individually, screenshot affected components
- [ ] Remove `--font-size-base`, let body inherit from html's 16px
- [ ] Remove `'Segoe UI'` from `--font-system` stack
- [ ] Safe-migrate `--spacing-md`: move 16px-intent consumers to `--spacing-lg`, then redefine to `12px`
- [ ] Run `./scripts/check.sh --svelte`, visual spot-check light + dark

### Milestone 2a: macOS accent color — read and inject
- [ ] Enable `NSColor` feature on `objc2-app-kit` in Cargo.toml
- [ ] Implement `get_accent_color` Tauri command
- [ ] Observe `NSSystemColorsDidChangeNotification`, emit Tauri event
- [ ] Frontend: call command on startup, set `--color-accent`, listen for changes
- [ ] Run `./scripts/check.sh --rust` and fix issues
- [ ] Manual test: change accent color live, verify all 8 macOS options (especially yellow and graphite)

### Milestone 2b: macOS accent color — migrate consumers
- [ ] Update cursor highlight to use `--color-accent-subtle`
- [ ] Remove `--color-cursor-focused-bg/fg`, replace `--color-cursor-unfocused-bg` with `--color-bg-tertiary`
- [ ] Manual test with all 8 accent colors, verify cursor, buttons, focus rings, switches

### Milestone 3: Shared button component
- [ ] Create `Button.svelte` with primary/secondary/danger/mini variants
- [ ] Include focus ring, disabled state, hover transitions
- [ ] Add Vitest tests for variant rendering
- [ ] Add to `coverage-allowlist.json` if needed

### Milestone 4a: Mechanical token replacements
- [ ] Migrate border-radius to `--radius-*` tokens across all components
- [ ] Migrate box-shadows to `--shadow-*` tokens
- [ ] Migrate transitions to `--transition-*` tokens
- [ ] Migrate raw z-index values to `--z-*` tokens
- [ ] Run `./scripts/check.sh --svelte`, visual spot-check light + dark

### Milestone 4b: Spacing and font-size migration
- [ ] Migrate off-scale 6px values to `--spacing-xs` or `--spacing-sm`
- [ ] Consolidate font-size (11px → 12px, 18px → 16/20px)
- [ ] Replace remaining hardcoded spacing with `--spacing-*` tokens
- [ ] Run checks, visual spot-check

### Milestone 4c: Button rollout, dialog restyle, and color cleanup
- [ ] Replace inline button styles with `<Button>` in all 9 dialogs
- [ ] Restyle FullDiskAccessPrompt to use ModalDialog and Button component
- [ ] Remove `--color-button-hover`/`--color-bg-hover`, replace in 9 consuming files
- [ ] Replace hardcoded hex colors with CSS variables (`#4caf50` → `--color-allow`, etc.)
- [ ] Replace hardcoded focus ring rgba with `var(--shadow-focus)`
- [ ] Manual test: open every dialog, verify all states
- [ ] Run `./scripts/check.sh --svelte`

### Milestone 5: Website font swap
- [ ] Download and self-host Geist Sans variable font
- [ ] Update global.css @font-face and @theme
- [ ] Update OG image font files and generation code
- [ ] Visual check all website pages

### Milestone 6a: Website dark palette warmup
- [ ] Update dark palette tokens to warmer values
- [ ] Refine CTA hover scale to 1.02
- [ ] Verify `prefers-reduced-motion` handling
- [ ] Run `./scripts/check.sh --check website-build --check website-e2e`

### Milestone 6b: Website light mode for sub-pages
- [ ] Add light mode tokens via `@media (prefers-color-scheme: light)` in global.css
- [ ] Keep landing page dark-only, scope light mode to sub-page layouts
- [ ] Handle blog code blocks (keep dark in light mode) and blog-prose.css
- [ ] Add Shiki `github-light` theme, switch based on color scheme
- [ ] Update Remark42 to pass correct theme
- [ ] Visual check every sub-page in both light and dark mode
- [ ] Run `./scripts/check.sh --check website-build --check website-e2e`

### Milestone 7: Polish
- [ ] Add empty folder state to file list
- [ ] Run full check suite (`./scripts/check.sh`)
- [ ] Manual test: app in light/dark with various accent colors
- [ ] Manual test: all website pages in light/dark
- [ ] Update app.css header comment to reference design system doc
- [ ] Remove all "Migration note" annotations from system.md
