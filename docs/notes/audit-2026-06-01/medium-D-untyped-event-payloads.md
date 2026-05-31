# IPC event payloads are entirely untyped — no specta link between Rust emit and FE listen

**Severity:** medium **Lens:** D — IPC boundary **Confidence:** high

## What

Cmdr enforces typed bindings for _commands_ (tauri-specta, raw `invoke` banned), but the _event_ half of the IPC
boundary has no equivalent. All 140 `app.emit(...)` call sites pass hand-built `serde_json::json!({...})` payloads, and
the frontend hand-declares the matching TypeScript type at each `listen<T>(name, ...)` call with no compile-time link
back to the Rust shape. `bindings.ts` carries zero event types; the project uses no `tauri_specta::Event` /
`collect_events!`. So an event is exactly the cross-layer magic-string + magic-field-name coupling the command-bindings
discipline was built to eliminate, just on the channel the rule doesn't cover.

## Why it matters

Rename a field in a Rust `json!` payload (or the event name string) and nothing fails to compile on either side — the FE
listener silently receives `undefined` for that field, or never fires. Concrete: `network/mod.rs:179` emits
`network-host-lost` as `json!({ "id": id })`; `network-store.svelte.ts:171` listens with
`listen<{ id: string }>('network-host-lost', ...)`. Change the Rust key to `"hostId"` (or rename the event) and the
host-removal handler quietly stops clearing discovered hosts, with no build error and no runtime exception — exactly the
failure class flagged for commands in `ipc/CLAUDE.md`. The blast radius is every user-facing event: `settings-changed`,
`mtp-permission-error`, `global-shortcut-fired`, the file-operation progress events, etc.

## Evidence

```rust
// network/mod.rs:179
let _ = app_handle.emit("network-host-lost", serde_json::json!({ "id": id }));
// commands/ui.rs:204
app.emit("settings-changed", serde_json::json!({ "showHiddenFiles": new_state }))
// mtp/connection/mod.rs:259
let _ = app.emit("mtp-permission-error", serde_json::json!({ "deviceId": device_id }));
```

```ts
// file-explorer/network/network-store.svelte.ts:171  — type declared by hand, unlinked to Rust
unlistenHostLost = await listen<{ id: string }>('network-host-lost', (event) => { ... })
// settings-store.ts:85
return listen<Partial<Settings>>('settings-changed', (event) => { ... })
// tauri-commands/mtp.ts:179,214 — bespoke interface re-declared FE-side
listen<MtpPermissionErrorEvent>('mtp-permission-error', (event) => { ... })
```

`grep` for `tauri_specta::Event` / `collect_events!` / `#[derive(..Event..)]` across `src-tauri/src` returns nothing.

## Suggested fix

Adopt tauri-specta typed events for the user-facing event surface: define
`#[derive(serde::Serialize, Clone, specta::Type, tauri_specta::Event)]` payload structs (e.g.
`NetworkHostLost { id: String }`, `SettingsChanged { ... }`), register them via `collect_events![]` in
`ipc.rs::builder()`, emit with the typed `Event::emit`, and consume on the FE via the generated
`events.networkHostLost.listen(...)`. This closes the rename-drift gap symmetrically with commands. It's a sizable
migration across ~140 sites, so it's reasonable to stage it (start with the highest-churn / highest-blast-radius events)
— but the current state is a genuine untyped boundary, not a documented trade-off.

## Notes

The internal `mcp-*` bridge events (`mcp/executor/*.rs`) are a deliberate exception: they're an MCP-automation
round-trip channel, intentionally loose JSON, and shouldn't be force-typed. This finding targets the user-facing app
events only. No CLAUDE.md acknowledges the untyped-event choice, so it isn't a recorded trade-off.
