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
