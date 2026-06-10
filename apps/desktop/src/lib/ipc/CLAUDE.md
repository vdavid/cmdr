# IPC bindings

Auto-generated `tauri-specta` bindings (`bindings.ts`) plus the typed-events plumbing that crosses the Rust↔TS
boundary. The Rust side lives in `apps/desktop/src-tauri/src/ipc.rs`.

## Module map

- `bindings.ts`: generated `commands.*` + `events.*` (don't hand-edit). Rust source of truth: `ipc.rs::builder()`.
- Call sites import typed wrappers from `$lib/tauri-commands/` (the canonical path), not `bindings.ts` directly.

## Must-knows

- **Don't hand-edit `bindings.ts`.** `pnpm check` runs `bindings-fresh`, which regenerates it on drift (outside `--ci`).
  Regen explicitly with `cd apps/desktop && pnpm bindings:regen`; review and commit the diff alongside the Rust change.
- **Never call `commands.*` / `events.*` raw in components.** Wrap each in `$lib/tauri-commands/` (an `on<Event>(cb)`
  for events). Enforced by `cmdr/no-raw-tauri-invoke`.
- **Name your positional args.** Specta wrappers take positional args, not an object. With >2 primitive args (bool /
  number / null), extract named locals at the call site; don't bury meaning in a thin helper.
- **Specta rc.24 can't handle two patterns; new code must avoid both.** (1) `#[serde(skip_serializing_if = …)]` (it
  splits the type and `validate_exported_command` rejects it in Unified mode; let `Option<T>`→`null`, `Vec<T>`→`[]`
  instead). (2) `serde_json::Value` at an IPC boundary (replace with a typed struct/enum). Genuinely free-form data
  stays on raw `invoke()` with the documented opt-out comment (see DETAILS § Excluded commands).
- **Internally-tagged enums with struct variants need `rename_all_fields = "camelCase"`** alongside the tag attribute;
  the tag `rename_all` does NOT cascade into variant fields, so multi-word fields silently ship snake_case and read
  `undefined` on the FE. Guarded by `ipc-enum-camelcase`.
- **Switching a raw-string emit to a typed `Event` must NOT change the wire name** (listening windows hold the
  capability permission under that name). Match the struct's kebab-cased name, or pin `#[tauri_specta(event_name = "…")]`.
- **On the JS side, compare IPC optionals with `!= null`, not `!== undefined`.** Because `skip_serializing_if` is banned,
  `Option::None` crosses as JSON `null`. A `=== undefined`-only check accepts `null` as a real value (renders literal
  `"null"`), and inside a `$effect`/`$derived` a throw on `null` silently corrupts the reactive graph for sibling
  effects (a `$state` write lands but a dependent effect never re-runs). Suspect every site passing an optional field to
  a typed function (`Intl.*`, `(n: number) => …`).
- **Two event families stay string-based** (can't be typed): the generic `mcp-*` dispatch relay (runtime-built name +
  free-form `Value`) and `viewer:file-changed:<session-id>` (session id interpolated at runtime).

For the add-a-command steps, the full typed-events rules, the excluded-commands table, IPC contract testing, and the
specta version-bump procedure, see DETAILS.md.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
