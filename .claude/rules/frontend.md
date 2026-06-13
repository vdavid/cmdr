# Frontend rules (`apps/desktop/src/`)

- Always use the CSS variables in `app.css` (stylelint rejects undefined ones). Never raw `px` for `font-size`,
  `border-radius`, `font-family`, or `z-index` ≥ 10: use the `--font-size-*` / `--radius-*` / `--font-*` / `--z-*`
  tokens.
- ❌ No raw `invoke('…')` outside `lib/ipc/`. Call the typed `commands.*` wrappers (regenerate with
  `pnpm bindings:regen`); prefer named locals over inline primitives at call sites. Enforced by
  `cmdr/no-raw-tauri-invoke`. See `lib/ipc/CLAUDE.md`.
- A new user-facing action needs its id in `COMMAND_IDS`, an entry in `command-registry.ts`, and a handler in
  `routes/(main)/command-handlers/` (a missing handler is a compile error). Enforced by `cmdr/no-raw-command-dispatch`.
  See `lib/commands/CLAUDE.md`.
- Stay aligned to Ark UI's naming. When wrapping an `@ark-ui/svelte` primitive in `lib/ui/`, name the wrapper after
  Ark's component (`Select`, `Combobox`, `Popover`, `Menu`, …) so the wrapper layer maps 1:1 to Ark and stays
  predictable. Flag any divergence from Ark's vocabulary (raise it, don't silently rename); a wrapper whose name drifts
  from the underlying Ark part forces every reader to keep a translation table.
