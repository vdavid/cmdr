# Raw `invoke('...')` call sites outside the documented exclusion list

**Severity:** medium
**Lens:** D — IPC boundary
**Confidence:** high

## Location

All sites carry `// eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up`
(or close variant). None of these command names appear in the typed `apps/desktop/src/lib/ipc/bindings.ts`.

- `apps/desktop/src/lib/tauri-commands/app-state.ts`
  - line 51: `await invoke('update_pin_tab_menu', { isPinned })`
  - line 57: `await invoke('set_reopen_closed_tab_enabled', { enabled })` (comment cites: generic over Runtime)
  - line 90: `await invoke('update_menu_context', { path, filename })`
  - line 100: `await invoke('set_menu_context', { context })`
  - line 116: `return invoke<boolean>('toggle_hidden_files')`
  - line 127: `await invoke('sync_menu_show_hidden', { checked })`
  - line 145: `await invoke('update_view_mode_menu', { activePane, leftMode, rightMode })`
  - line 158: `await invoke('show_main_window')`
- `apps/desktop/src/lib/tauri-commands/file-actions.ts`
  - line 45: `await invoke('show_file_context_menu', { … 6 args … })`
  - line 90: `await invoke('show_breadcrumb_context_menu', { … })`
  - line 113: `await invoke('copy_to_clipboard', { text })`
- `apps/desktop/src/lib/tauri-commands/settings.ts`
  - line 124: `await invoke('set_mcp_enabled', { enabled, port })`
  - line 130: `await invoke('set_mcp_port', { port })`
  - line 272: `await invoke('start_ai_download')`
  - line 309: `await invoke('configure_ai', { provider, contextSize, cloudApiKey, cloudBaseUrl, cloudModel })`
  - line 320: `await invoke('start_ai_server', { ctxSize })`

Documented exclusions per `lib/ipc/CLAUDE.md` § "Excluded commands" — these do NOT cover the above:
`record_breadcrumb`, `prepare_error_report_preview`, `store_font_metrics`, `stream_folder_suggestions`,
`cancel_folder_suggestions`.

## What

The repo's typed-IPC rule (`AGENTS.md` § Critical rules, `lib/ipc/CLAUDE.md`) requires every Tauri command call to go
through `commands.commandName(…)` from `lib/ipc/bindings.ts`. The eslint rule `cmdr/no-raw-tauri-invoke` enforces this,
with two narrow exempt directories (`lib/ipc/`, `routes/debug/`) and a small documented exclusion list of commands
that specta genuinely can't type. Sixteen call sites carry per-line opt-outs whose justification is "not in typed
bindings; tracked for follow-up." None of the named commands have a documented specta blocker. They simply weren't
added to `ipc::collect_*_types()`.

In particular:

- `set_reopen_closed_tab_enabled`'s comment says "generic over Runtime; not in typed bindings" — but its Rust
  signature is the same `<R: Runtime>` shape as `update_pin_tab_menu` (which has the bland "not in typed bindings;
  tracked for follow-up" comment). Generic-over-Runtime IS a real specta blocker per `lib/ipc/CLAUDE.md` (see
  `store_font_metrics`), so that one MIGHT be a legitimate exclusion — but the rest aren't.
- `configure_ai` takes a sensitive `cloudApiKey: String`; bypassing typed bindings means the wire-field name isn't
  checked against the Rust serde name at build time. A future rename (`cloud_api_key` → `api_key`) silently fails to
  call through.
- `show_file_context_menu` carries six positional args via inline object — exactly the multi-arg shape that
  `lib/ipc/CLAUDE.md` § "IPC contract testing" calls out as high-risk ("multi-positional-arg ordering bugs … easy to
  swap two by accident").

## Why it matters

1. **The exclusion list is the load-bearing contract.** Either a command has a documented specta blocker (and it's in
   the excluded-commands table with a conversion plan), or it's in the typed bindings. "Tracked for follow-up" with
   no tracker reference and no blocker is neither.
2. **Renames break runtime IPC silently.** This is the exact failure mode the rule was created to prevent. With 16
   raw call sites today, the surface for that bug is real, not theoretical.
3. **Worst offender takes secrets.** `configure_ai`'s `cloudApiKey` argument crosses IPC via untyped invoke. A
   serde-rename mismatch wouldn't throw — the Rust side would just receive an empty string and the user's BYOK
   provider would 401 with no clue why.

## Evidence

`app-state.ts:48-52`:

```ts
// eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
await invoke('update_pin_tab_menu', { isPinned })
```

`settings.ts:307-310`:

```ts
// eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
await invoke('configure_ai', { provider, contextSize, cloudApiKey, cloudBaseUrl, cloudModel })
```

Doc rule violated (`AGENTS.md` § Critical rules):

> Type-safe IPC: no raw `invoke('...')` outside the typed bindings folder. Tauri command names are duplicated across
> the Rust `#[tauri::command]` site and every TS call site, with no compile-time link.

Doc rule for legitimate opt-outs (`lib/ipc/CLAUDE.md` § "Excluded commands"):

> These commands stay on raw `invoke()` for now. Each call site has: `// eslint-disable-next-line cmdr/no-raw-tauri-
> invoke -- excluded from typed bindings (see ipc/CLAUDE.md); …`

The opt-outs in question don't reference `ipc/CLAUDE.md` and don't name a blocker.

## Suggested fix

1. **Triage the list.** For each of the 16 sites, confirm whether there's a real specta blocker:
   - Commands generic over `<R: Runtime>` are blocked (per existing `store_font_metrics` precedent). Candidates from
     the list: `update_pin_tab_menu`, `set_reopen_closed_tab_enabled`, `update_menu_context`, `set_menu_context`,
     `toggle_hidden_files`, `sync_menu_show_hidden`, `update_view_mode_menu`. Each Rust signature is `<R: Runtime>`.
   - Commands NOT generic over `R` and with no other blocker: `show_main_window`, `show_file_context_menu`,
     `show_breadcrumb_context_menu`, `copy_to_clipboard`, `set_mcp_enabled`, `set_mcp_port`, `start_ai_download`,
     `configure_ai`, `start_ai_server`. These have no blocker today; they were just not added to
     `collect_*_types()`.
2. **For the no-blocker set**, add them to `ipc.rs::collect_*_types()`, run `pnpm bindings:regen`, replace the raw
   `invoke('foo', { … })` with `commands.foo(…)`, and remove the eslint-disable. Prioritise `configure_ai` (secrets)
   and `show_file_context_menu` (six positional args).
3. **For the genuine `<R: Runtime>` blocker set**, either (a) drop the runtime generic where the command doesn't
   actually need it (most don't — they call `app.state::<MenuState<R>>()` but could take `AppHandle<tauri::Wry>` like
   `show_tab_context_menu` does on `ui.rs:562`), or (b) add them to the `lib/ipc/CLAUDE.md` § "Excluded commands"
   table with the runtime-generic blocker named, and rewrite the opt-out comments to reference that table.
4. **Tighten the eslint rule's allowed comment text** to require the form documented in `lib/ipc/CLAUDE.md`:
   `excluded from typed bindings (see ipc/CLAUDE.md); <blocker>`. That prevents future "tracked for follow-up" sneak-
   throughs.

## Notes

The 16 raw-invoke sites also bypass the IPC contract tests pattern in `lib/ipc/CLAUDE.md` § "IPC contract testing"
(it operates on `commands.*` via `installIpcMock`). Migrating them to typed bindings unlocks contract tests on the
same surfaces.
