# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else
(architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## Leading

One scale for the whole app: the four `--font-line-height-*` tokens, picked per surface in `docs/design-system.md` §
Leading. The text surfaces (`.modal-dialog`, `.toast`, the sheet, the secondary windows) INHERIT `normal` from
`app.css`, which is why a component usually writes no `line-height` at all. The main window and the file lists
deliberately inherit no ratio: their rows size from the density tiers, and a ratio there would fight them.

## Reduced transparency

WKWebView never reflects `@media (prefers-reduced-transparency)`, so the app can't key a frosted-glass fallback off it.
The backend reads the `NSWorkspace` value and `$lib/reduce-transparency` (inited per window) toggles an
`html.reduce-transparency` CLASS instead. Under that class, `app.css` § Reduced transparency flips
`--color-bg-glass` / `--color-border-glass` to opaque, and each surface drops its `backdrop-filter` (and the `-webkit-`
twin) via `:global(html.reduce-transparency)`. `prefers-reduced-motion` WKWebView does honor, so that one stays a media
query.

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
  selection fill, selected-row hairline, cursor fill and outline, the `.restricted-indicator` icon). Only rules that
  were identical in both views live here; see `lib/file-explorer/views/DETAILS.md` for what stayed per-view and why.

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
