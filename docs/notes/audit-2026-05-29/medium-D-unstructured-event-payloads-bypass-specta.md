# Tauri events emitted as `serde_json::json!` literals bypass the typed-binding contract

**Severity:** medium
**Lens:** D — IPC boundary
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/commands/ui.rs:204` — `settings-changed`
- `apps/desktop/src-tauri/src/commands/search.rs:117-122` — `search-index-ready`
- `apps/desktop/src-tauri/src/lib.rs:720,756,769,810,819,826,841,867,894,971` — `settings-changed`, view-mode change,
  `tab-close`, sort-context-action, breadcrumb actions, command-palette dispatch, etc. (10 emit sites in `lib.rs`
  alone, all with inline `serde_json::json!({…})` payloads)
- `apps/desktop/src-tauri/src/network/mod.rs:179,183,218,237` — `network-host-lost`, network discovery state events
- `apps/desktop/src-tauri/src/mtp/connection/mod.rs:235,259,379,445,588,618,1133,1140` — MTP permission/progress events
- `apps/desktop/src-tauri/src/indexing/memory_watchdog.rs:68` — indexing memory event

## What

`ipc/CLAUDE.md` § "Type shape constraints" bans `serde_json::Value` at the IPC boundary because specta can't represent
it and the wire shape stops being checked. The same reasoning applies to `app.emit(name, serde_json::json!({…}))`:
the payload type is `Value`, the binding generator can't emit a TypeScript shape for it, and FE listeners read fields
by string (`payload.showHiddenFiles`, `payload.action`, `payload.value`, `payload.commandId`, …) with no compile-time
contract. Renaming the Rust-side key is silent breakage; the FE just sees `undefined`.

There are ~30 such call sites across the codebase. The documented exclusion list in `lib/ipc/CLAUDE.md` covers
`record_breadcrumb` and `prepare_error_report_preview` only — neither involves these emit sites.

## Why it matters

1. **DTO drift detection lost.** The whole point of `tauri-specta` is that a wire-shape change on the Rust side
   surfaces as either a `bindings-fresh` CI failure or a TS compile error. Untyped `json!` events sidestep both. The
   class of bug the typed-bindings rule was created to prevent (the "renaming the Rust side silently breaks runtime
   IPC" footgun from `AGENTS.md`) reappears, just over events instead of commands.
2. **Event payload mismatches are silent.** `settings-changed` is emitted from two distinct sites with two distinct
   payload shapes (`{ "showHiddenFiles": bool }` in `commands/ui.rs::toggle_hidden_files` and `{ "action": …, "value":
   … }` in `lib.rs` menu handlers). A FE listener written against one shape silently fails on the other; we'd never
   know until a user reports the affected setting "doesn't stick."
3. **Multiplies the surface that has to be hand-verified.** `settings-changed`, `tab-context-action`,
   `network-host-lost`, `search-index-ready`, `mtp-permission-error`, and friends are all production-critical events.
   Typing them once eliminates ongoing review burden.

## Evidence

`commands/ui.rs:204`:

```rust
app.emit("settings-changed", serde_json::json!({ "showHiddenFiles": new_state }))
```

`lib.rs:756` (different shape, same event name):

```rust
let _ = app.emit("view-mode-changed", serde_json::json!({ "mode": mode_str, "pane": pane }));
```

`lib.rs:810` and `:819` (same event name, two payload shapes):

```rust
serde_json::json!({ "action": "sortBy", "value": column }),
serde_json::json!({ "action": "sortOrder", "value": order }),
```

Doc rule being skirted (`ipc/CLAUDE.md` § "Type shape constraints"):

> `serde_json::Value`: specta inlines the recursive Value enum but emits `Vec<Value>` / `Map<string, Value>` as Rust
> type names in the TS output. Workaround: replace `Value` with a typed struct or enum. … Try not to add new uses of
> `Value` at IPC boundaries.

## Suggested fix

Migrate each emit site to a typed struct registered through `tauri-specta`'s event collection:

```rust
#[derive(Debug, Clone, serde::Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SettingsChanged {
    pub show_hidden_files: Option<bool>,
    // …other fields as needed; consider splitting into per-key events instead of a union
}

// Emit:
SettingsChanged { show_hidden_files: Some(new_state) }.emit(&app)?;
```

…then add each event type to the `collect_events![]` block in `ipc.rs::builder()`, regenerate `bindings.ts`, and have
the FE consume via `events.settingsChanged.listen(…)` instead of raw `listen('settings-changed', …)`.

Priority order for migration (highest user-visible impact first):

1. `settings-changed` (multiple incompatible shapes today)
2. `search-index-ready`
3. `tab-context-action`, `sort-context-action`, `view-mode-changed`, the breadcrumb action events
4. `network-host-lost`, network discovery state changes
5. MTP progress/permission events
6. Indexing memory watchdog

Track the remaining `serde_json::json!` emit sites with a `cmdr/no-json-event-payload` lint mirroring
`cmdr/no-raw-tauri-invoke`, so new emits land typed by default.

## Notes

- Three of the lib.rs sites carry a single `{ "commandId": String }` shape and could share one event type.
- `network::mod.rs:183,237` carry `DiscoveryState` (already a typed enum) inside a `Value` wrapper, so the
  fix is purely mechanical: define a `NetworkDiscoveryStateChanged { state: DiscoveryState }` event.
- `mtp/connection/mod.rs:1133,1140` already build the payload as a `serde_json::Value` local before emitting; the
  callsites are the cleanest migration candidates because the field set is contained to one file.
