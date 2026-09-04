# Svelte frontend (`src/`)

The Svelte 5 + TypeScript frontend (SvelteKit static adapter, custom CSS with design tokens). Feature must-knows live in
each directory's colocated `CLAUDE.md`; the subsystem map is `docs/architecture.md`. These rules cover all frontend
code.

## Frontend rules

- Always use the CSS variables in `app.css` (stylelint rejects undefined ones). Don't hardcode a `px` value that has a
  matching `--spacing-*` / `--font-size-*` / `--radius-*` token, a `z-index` ≥ 10, or a non-token `font-family`:
  `declaration-property-value-disallowed-list` flags exactly those. Token-less values (1px borders, negative nudges,
  display font sizes) may stay raw, with a `stylelint-disable` + `-- reason`.
- Global CSS is `app.css` (tokens, base) plus `app-field` / `app-utilities` / `app-tooltip` / `app-file-list`, loaded
  AFTER it by `routes/+layout.svelte`. Never `@import` those four: that hoists and inverts the cascade.
- ❌ Never write a raw `line-height` number (stylelint rejects all but `0`). Leading comes from the
  `--font-line-height-*` tokens, which the text surfaces already inherit, so a component usually writes nothing. Which
  token where: `docs/design-system.md` § Leading; who inherits what: `DETAILS.md` § Leading.
- A frosted-glass surface MUST degrade under "reduce transparency", keyed off the `html.reduce-transparency` CLASS, ❌
  never `@media (prefers-reduced-transparency)` (WKWebView never reflects it). Use the `--color-bg-glass` /
  `--color-border-glass` tokens and drop `backdrop-filter` under that class. `DETAILS.md` § Reduced transparency.
- ❌ No raw `invoke('…')` outside `lib/ipc/`. Call the typed `commands.*` wrappers (regenerate with
  `pnpm bindings:regen`); prefer named locals to inline primitives at call sites. `cmdr/no-raw-tauri-invoke`;
  `lib/ipc/CLAUDE.md`.
- A new user-facing action needs its id in `COMMAND_IDS`, a `command-registry.ts` entry, and a handler in
  `routes/(main)/command-handlers/` (a missing one is a compile error). `cmdr/no-raw-command-dispatch`;
  `lib/commands/CLAUDE.md`.
- ❌ A keydown handler matches the WHOLE combo — `eventMatchesCommand(e, 'some.command')` from `$lib/shortcuts` — never
  a hand-rolled `e.key === 'a' && e.metaKey`: that's a modifier SUPERSET, so `⌥⌘A` fires it too. `cmdr/no-raw-key-match`
  flags a required modifier paired with a literal key test leaving another unconstrained; opt out with a reasoned
  `eslint-disable-next-line`. See `lib/shortcuts/CLAUDE.md`.
- ❌ Render inline glyphs via `<Icon name size>` and spinners via `<Spinner size>` (`$lib/ui/`), never a
  `~icons/lucide/*` import in feature code or a hand-rolled spinner ring. New glyphs register in
  `lib/ui/icons/icon-map.ts`, the one place lucide is imported (`cmdr/no-raw-lucide-import`); custom ones are components
  in `lib/ui/icons/`. `Icon` has no `color` prop (set it on the wrapping span). Every glyph and spinner appears in the
  Debug "Graphics" catalog (`routes/dev/graphics/`). See `docs/guides/icons.md`.
- ❌ A size or duration becomes text in ONE place: `$lib/units` (`formatByteSize` / `formatDuration`), `<Size bytes>` in
  markup. Never a private `formatBytes` or a `1024` ladder: four copies once drifted and two windows disagreed.
  `cmdr/no-private-unit-format`.
- Stay aligned to Ark UI's naming: a wrapper in `lib/ui/` takes the name of the `@ark-ui/svelte` component it wraps
  (`Select`, `Combobox`, `Popover`, `Menu`, …), 1:1. Raise any divergence, don't silently rename. Feature code imports
  the house wrapper, never `@ark-ui/svelte` (`cmdr/no-raw-ark-import`, allowlisted to `lib/ui/` alone).
- ❌ Before hand-rolling a control or dialog/window chrome, reach for the `lib/ui` primitive (`ModalDialog`, `Checkbox`,
  `RadioGroup`, `ToggleGroup`, `Select`, `Combobox`, `Chip`, …) and check Debug > Components. Never a raw
  `<input type=checkbox|radio>` / `<select>`, nor a `<button>` / `<div>` wearing `role=switch|checkbox|radio`
  (`cmdr/prefer-ui-primitive`; opt out per element with a reason when a control is genuinely bespoke). Record and
  document a new primitive: `docs/guides/building-ui.md`.
- Adding code that loads remote content (`fetch`, `iframe`)? Ask whether to disable it in dev, where
  `withGlobalTauri: true` makes remote content a security risk.
- **`app.html`'s boot guard is ES5 on purpose**: one ES6 token kills it silently on old Safari, and `pnpm dev` won't
  hot-reload the shell. `lib/utils/DETAILS.md` § Old-WebKit boot guard.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
