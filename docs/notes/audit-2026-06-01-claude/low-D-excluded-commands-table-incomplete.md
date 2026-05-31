# `ipc/CLAUDE.md` "Excluded commands" table omits ~13 raw-invoke survivors

**Severity:** low **Lens:** D — IPC boundary **Confidence:** high

## Location

`apps/desktop/src/lib/ipc/CLAUDE.md:98-111` (the "Excluded commands" table) vs. the raw `invoke()` call sites in
`apps/desktop/src/lib/tauri-commands/`

## What

The "Excluded commands" table documents five commands that stay on raw `invoke()` (`record_breadcrumb`,
`prepare_error_report_preview`, `store_font_metrics`, `stream_folder_suggestions`, `cancel_folder_suggestions`). In
reality there are ~18 raw-invoke survivors. The other ~13 — `set_mcp_enabled`, `set_mcp_port`, `start_ai_download`,
`configure_ai`, `start_ai_server`, `show_file_context_menu`, `show_breadcrumb_context_menu`, `copy_to_clipboard`,
`update_pin_tab_menu`, `set_reopen_closed_tab_enabled`, `update_menu_context`, `set_menu_context`,
`sync_menu_show_hidden`, `update_view_mode_menu`, `show_main_window` — are excluded because their Rust signatures are
generic over `<R: Runtime>`, which specta can't collect type info for (the same reason the table gives for
`store_font_metrics`). They're registered in `generate_handler![]` (runtime dispatch works) but absent from
`collect_*_types()`, so they're missing from `bindings.ts` and call sites carry
`// eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up`.

## Why it matters

`ipc/CLAUDE.md` presents the table as the authoritative list of what's allowed off the typed path, and says "When specta
gets a fix that closes one of these, drop the opt-out." A future maintainer reconciling opt-out comments against the
table will find 13 disable-comments with no table entry, can't tell whether they're sanctioned or stragglers to migrate,
and has no recorded reason (`<R: Runtime>` generic) or conversion plan for them. These are the largest concentration of
stringly-named IPC in the app, so the doc should name the constraint that keeps them there.

## Evidence

```rust
// commands/mcp.rs:10        pub async fn set_mcp_enabled<R: Runtime + 'static>(app: AppHandle<R>, ...)
// commands/ui.rs:30         pub fn show_file_context_menu<R: Runtime>(window: Window<R>, ...)
// commands/ui.rs:649        pub fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: String) ...
// ai/manager.rs:460         pub fn configure_ai<R: Runtime>(...)
```

```ts
// lib/tauri-commands/app-state.ts:56
// eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic over Runtime; not in typed bindings
await invoke('set_reopen_closed_tab_enabled', { enabled })
```

A `grep "async <camelName>(" lib/ipc/bindings.ts` for all 13 names returns "NOT in bindings" for every one.

## Suggested fix

Add a second row group to the "Excluded commands" table (or a short subsection) listing the `<R: Runtime>`-generic
commands with the reason "generic over `tauri::Runtime`; specta can't collect type info for generic commands" and the
conversion plan "drop the generic and take a concrete `AppHandle`/`Window` only if the command never needs to be called
from the MCP executor's generic-`Runtime` path; otherwise keep as-is." That makes every raw-invoke opt-out traceable to
a documented reason, matching the discipline the rest of the file sets.

## Notes

This is a documentation-completeness finding, not a code defect — each survivor has a valid per-line opt-out and the
generic-`Runtime` constraint is real (several of these are deliberately generic so the MCP executor, which is generic
over `Runtime`, can call them). No payload is weakly typed; the args are concrete. Filing low because it's an accuracy
gap in the canonical IPC doc that future IPC work keys off.
