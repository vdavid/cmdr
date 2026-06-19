# Svelte frontend (`src/`)

The Svelte 5 + TypeScript frontend (SvelteKit static adapter, custom CSS with design tokens). Feature must-knows live in
each directory's colocated `CLAUDE.md`; the subsystem map is [`/docs/architecture.md`](../../../docs/architecture.md).
These rules apply to all frontend code under here.

## Frontend rules

- Always use the CSS variables in `app.css` (stylelint rejects undefined ones). Don't hardcode a `px` value that has a
  matching `--spacing-*` / `--font-size-*` / `--radius-*` token (or `z-index` ≥ 10, or a non-token `font-family`):
  stylelint's `declaration-property-value-disallowed-list` flags exactly those values, so use the token. Token-less
  values (1px borders, negative nudges, display font sizes) may stay raw, with a `stylelint-disable` + `-- reason`.
- A translucent / frosted-glass surface MUST degrade when "reduce transparency" is on. Key the opaque fallback off the
  `html.reduce-transparency` CLASS, NOT `@media (prefers-reduced-transparency)` — WKWebView never reflects that media
  query (it parses it and reflects `prefers-color-scheme`, but not this one), so the class is toggled from the backend
  `NSWorkspace` value via `$lib/reduce-transparency` (inited per window). Drop the `backdrop-filter` (and `-webkit-`
  twin) under `:global(html.reduce-transparency)`, and use the shared `--color-bg-glass` / `--color-border-glass`
  tokens, which flip to opaque under that class in `app.css` § Reduced transparency. (Same spirit as
  `prefers-reduced-motion` — except that one WKWebView does honor.)
- ❌ No raw `invoke('…')` outside `lib/ipc/`. Call the typed `commands.*` wrappers (regenerate with
  `pnpm bindings:regen`); prefer named locals over inline primitives at call sites. Enforced by
  `cmdr/no-raw-tauri-invoke`. See [`lib/ipc/CLAUDE.md`](lib/ipc/CLAUDE.md).
- A new user-facing action needs its id in `COMMAND_IDS`, an entry in `command-registry.ts`, and a handler in
  `routes/(main)/command-handlers/` (a missing handler is a compile error). Enforced by `cmdr/no-raw-command-dispatch`.
  See [`lib/commands/CLAUDE.md`](lib/commands/CLAUDE.md).
- ❌ Render inline glyphs via `<Icon name size>` (`$lib/ui/Icon.svelte`) and loading spinners via `<Spinner size>`
  (`$lib/ui/Spinner.svelte`). Don't import `~icons/lucide/*` in feature code or hand-roll a spinner ring. Add a new
  glyph to `lib/ui/icons/icon-map.ts` (the one place lucide is imported, enforced by `cmdr/no-raw-lucide-import`);
  custom non-Lucide glyphs live as components in `lib/ui/icons/` and register there too. `Icon` has no `color` prop (set
  `color` on the wrapping span). Every glyph and spinner appears in the Debug "Graphics" catalog
  (`routes/dev/graphics/`). See [`/docs/guides/icons.md`](../../../docs/guides/icons.md).
- Stay aligned to Ark UI's naming. When wrapping an `@ark-ui/svelte` primitive in `lib/ui/`, name the wrapper after
  Ark's component (`Select`, `Combobox`, `Popover`, `Menu`, …) so it maps 1:1 to Ark. Flag any divergence (raise it,
  don't silently rename).
- When adding code that loads remote content (`fetch`, `iframe`), ask whether to disable it in dev mode:
  `withGlobalTauri: true` is on in dev, which makes remote content a security risk.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
