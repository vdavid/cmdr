# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else
(architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## Global stylesheets

Six sheets, all global (no Svelte scoping). Who owns what:

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

### Cascade order is load order, and it's manual

`app.css` `@import`s the two order-independent sheets (palette, reset) at its top. The other three are imported from
`routes/+layout.svelte`, in a fixed order, AFTER `app.css`.

**Why:** a CSS `@import` must sit at the top of its sheet, so `@import`-ing a sheet whose rules belong at the END would
put them BEFORE everything that precedes them today. Wherever specificity ties, the winner flips, and the regression is
invisible until someone notices a wrong border weeks later. Importing from the layout is what reproduces the original
single-file order exactly. So: don't convert those three to `@import`s, and don't reorder the imports in the layout.

The dark-mode / `prefers-contrast` / reduced-transparency blocks in `app.css` are order-load-bearing for the same reason
(they override the light defaults above them). Leave them where they are.

**Verifying a move.** Vite's content hashes make this cheap: build the frontend before and after (`vite build` in
`apps/desktop`), then compare `build/_app/immutable/assets/*.css`. A pure move leaves every emitted file byte-identical,
hashed filenames included.
