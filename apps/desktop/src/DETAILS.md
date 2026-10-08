# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else
(architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## Leading

One scale for the whole app: the four `--font-line-height-*` tokens, picked per surface in `docs/design-system.md` §
Leading. The text surfaces (`.modal-dialog`, `.toast`, the sheet, the secondary windows) INHERIT `normal` from
`app.css`, which is why a component usually writes no `line-height` at all. The main window and the file lists
deliberately inherit no ratio: their rows size from the density tiers, and a ratio there would fight them.

## Dev gates

A `.svelte` script gates dev-only behavior on `isDevBuild()` (`$lib/app-mode`), never on an inline
`import.meta.env.DEV`. knip collects a Svelte file's imports with a regex, and a bare `import.meta` matches its import
pattern: the match runs on to the next quoted string, the synthetic module knip parses comes out broken, and every
`await import(...)` in that file disappears from its graph. One `import.meta.env.DEV` in `routes/(main)/+page.svelte`
was enough to make knip call `lib/debug/debug-window.ts` an unused file while the app was opening the debug window from
it (knip 6.36.0, `dist/compilers/compilers.js` § `importMatcher`, verified 2026-09-21). A `.ts` file is parsed by
TypeScript and may write `import.meta.env.DEV` directly.

`{#if}` MARKUP is the exception and keeps the literal (`routes/(main)/+layout.svelte`): Vite replaces
`import.meta.env.DEV` at build time, so the whole dev-only subtree leaves the prod bundle. A function call survives
minification, so gating markup on `isDevBuild()` would ship the dialog gallery and the dev pages to users.

## Glass material

Every frosted-glass surface (menus, selects, popovers, toasts, tooltips) draws from the tokens in `app.css` §
Frosted-glass material, and `$lib/glass-material` (inited per window) feeds them the two macOS settings that shape it:

- **Reduced transparency.** WKWebView never reflects `@media (prefers-reduced-transparency)`, so the app can't key a
  fallback off it. The backend reads the `NSWorkspace` value and the module toggles an `html.reduce-transparency` CLASS
  instead. Under it, `app.css` § Reduced transparency flips `--color-bg-glass` / `--color-bg-glass-steady` /
  `--color-border-glass` to opaque and `--glass-backdrop` to `none`, so a surface using the tokens needs no rule of its
  own. `prefers-reduced-motion` WKWebView does honor, so that one stays a media query.
- **The Liquid Glass slider** (macOS 27 Appearance). The backend reads the undocumented `NSGlassTintAmount` global
  default, re-reading it each time the app becomes active (`apps/desktop/src-tauri/src/glass_tint.rs` has why no
  notification works), and the module sets `--glass-tint` (0 clearest to 1 most tinted, 0.5 when macOS reports none).
  The glass fill's opacity and the blur grow with it, as native menus do. Toasts ride it with a higher opacity floor
  (`--glass-toast-opacity`), since they hold paragraphs over busy lists; tooltips and the unblurred Ask Cmdr drop hint
  use the fixed `--color-bg-glass-steady` instead.

Menu-like surfaces (`Menu`, `Select`, the breadcrumb popup) also take the macOS 26+ menu shape: `--radius-menu`,
`--shadow-glass` plus the `--shadow-glass-rim` top highlight, and rows highlighted as inset pills.

## Window drag strips sit at `--z-sticky`, under every menu

Each secondary window (settings, debug, shortcuts, queue) paints an invisible `.window-drag-region` over its top strip
so the user can move the window from the empty space beside the traffic lights. It's absolutely positioned, so it
already paints over the in-flow layout — a rung is only needed to beat other POSITIONED chrome, and `--z-sticky` is as
high as it should ever go. All four sat at `--z-dropdown` once, which put an invisible strip on the same rung as the
app's menus and let it swallow clicks on their top rows: the AI provider pop-up in settings opens over its trigger, so
its first two options landed under the strip and couldn't be picked. Anything a menu can open into belongs BELOW
`--z-dropdown`. (A menu clears the strip only if its own rung really applies, which depends on the element carrying it:
`lib/ui/DETAILS.md` § Select.)

## Global stylesheets

Seven sheets, all global (no Svelte scoping). Who owns what:

- `app.css`: the design tokens (`:root`, plus the dark-mode, `prefers-contrast`, reduced-transparency, and old-WebKit
  override blocks), then the base element styles (focus ring, typography layer, `html` / `body`, `#app-root`).
- `app-palette.css`: the static Tailwind color scale. Scheme-independent reference data with no `var()` dependencies, so
  it's order-independent.
- `app-reset.css`: the ress-derived reset, inside `@layer ress-reset`.
- `app-field.css`: the `.text-field*` chrome behind `lib/ui/TextInput.svelte` and `lib/ui/TextArea.svelte`. Nothing else
  may use those classes.
- `app-utilities.css`: class-per-value utilities applied via class bindings (`.size-*` size tiers, `.age-*` date ages,
  `.spinner*`).
- `app-tooltip.css`: `.cmdr-tooltip`, the singleton element the tooltip action creates.
- `app-file-list.css`: the `.file-entry` row chrome that `FullList.svelte` and `BriefList.svelte` share (stripe,
  selection fill, selected-row hairline, cursor fill and outline). Only rules that were identical in both views live
  here; see `lib/file-explorer/views/DETAILS.md` for what stayed per-view and why.

### Cascade order is load order, and it's manual

`app.css` `@import`s the two order-independent sheets (palette, reset) at its top. The other four are imported from
`routes/+layout.svelte`, in a fixed order, AFTER `app.css`.

**Why:** a CSS `@import` must sit at the top of its sheet, so `@import`-ing a sheet whose rules belong at the END would
put them BEFORE everything that precedes them today. Wherever specificity ties, the winner flips, and the regression is
invisible until someone notices a wrong border weeks later. Importing from the layout is what reproduces the original
single-file order exactly. So: don't convert those four to `@import`s, and don't reorder the imports in the layout.

### Moving a rule out of a component costs a class of specificity

Svelte scopes a component rule by appending `.svelte-<hash>` to its FIRST compound selector (the rest get a
zero-specificity `:where(.svelte-<hash>)`). So lifting `.file-entry.is-selected` out of a `<style>` block drops it from
(0,3,0) to (0,2,0), and it starts tying with things it used to beat. `app-file-list.css` pays this back by prefixing
every selector with the view's container class (`.full-list-container` / `.brief-list-container`), which restores the
original specificity exactly. Without that, `DualPaneExplorer`'s `:global(.file-entry.folder-drop-target)` (also
(0,2,0), and emitted LATER: component styles ride the route chunk, these sheets ride the root-layout chunk) would win
the tie and paint the drag-over highlight over the selection and cursor fills.

Load order between the two chunks is real but should never be load-bearing: keep a lifted rule's specificity at or above
what it had inside the component.

The dark-mode / `prefers-contrast` / reduced-transparency blocks in `app.css` are order-load-bearing for the same reason
(they override the light defaults above them). Leave them where they are.

**Verifying a move.** Vite's content hashes make this cheap: build the frontend before and after (`vite build` in
`apps/desktop`), then compare `build/_app/immutable/assets/*.css`. A move between two global sheets leaves every emitted
file byte-identical, hashed filenames included. A move OUT of a component rewrites selectors, so compare declarations
instead: extract every `(selector, property, value)` triple from the before and after bundles, normalize away the
`.svelte-<hash>` classes and the added container prefix, and diff the sets. Anything that shows up on one side only is a
rule you changed, not moved.
