# Typed Tauri events via tauri-specta

Plan for extending the existing `tauri-specta` command machinery to **events**, so event names and payload types become
generated and typed (folding into the `bindings-fresh` check) instead of raw strings emitted from Rust and hand-mirrored
in TS.

The pattern below is **proven green** end-to-end on one event (`volume-space-changed`). The rest of this doc is the
inventory + partition for migrating the remaining events in parallel.

## Why

- Today every event is a raw string on both sides: Rust `app.emit("volume-space-changed", &payload)` and TS
  `listen<{...}>('volume-space-changed', ...)`. The payload shape is hand-mirrored. Renaming a field or the event on the
  Rust side silently breaks the FE at runtime with no compile-time link, the same class of bug `tauri-specta` already
  closed for commands.
- We already pay for `tauri-specta` (rc.24) for commands. Events use the same `Builder`, the same `bindings.ts`, the
  same `bindings-fresh` check. Extending to events is incremental, not a new dependency.

## The proven pattern (copy-pasteable)

Verified by a green `./scripts/check.sh --check bindings-fresh --check clippy --check svelte-check` plus the
`space_poller` unit tests. The reference migration is `volume-space-changed`.

### 1. Rust payload struct — derive `Event`

The struct name kebab-cases to the wire event name. `VolumeSpaceChanged` → `volume-space-changed`. So **name the struct
after the event** (drop any `Payload` suffix); only reach for the `#[tauri_specta(event_name = "...")]` override when
the kebab-case of the struct name wouldn't match the desired wire name.

```rust
use serde::{Deserialize, Serialize};
use tauri_specta::Event;

#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceChanged {
    pub volume_id: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}
```

- `Event` requires `Type` (specta) and, because the FE deserializes the payload, `Deserialize`. `Clone + Serialize` were
  already there for emit. The fields must be `pub` (the emit site is in another module).
- `NAME` is derived as `ident.to_string().to_kebab_case()` (from `tauri-specta-macros` rc.24). Override only via
  `#[tauri_specta(event_name = "literal-name")]` on the struct.

### 2. Register the event in the builder (`ipc.rs`)

```rust
use tauri_specta::{Builder, collect_events};
use crate::space_poller::VolumeSpaceChanged;

// in builder():
Builder::<tauri::Wry>::new()
    .commands(combined_commands)
    .events(collect_events![VolumeSpaceChanged])
```

`collect_events![A, B, C]` takes struct paths. This is the **single serialized chokepoint** for the whole migration (see
§ Orchestration).

### 3. Mount events on the app (`lib.rs`, in `setup`) — one-time wiring, already done

`Event::emit` / `Event::listen` resolve the event name from an `EventRegistry` that `mount_events` installs. Without it
they panic at runtime. The `Builder` is needed both for `invoke_handler()` (the command chain) and `mount_events()` (in
`setup`), both `&self`, so grab the handler into a local before moving the builder into the `setup` closure:

```rust
let specta_builder = ipc::builder();
let invoke_handler = specta_builder.invoke_handler();   // owned closure (clones the cmd map)
// ...
tauri::Builder::default()
    .setup(move |app| {
        specta_builder.mount_events(app);                // registers all collect_events! types
        // ...rest of setup...
        Ok(())
    })
    .invoke_handler(invoke_handler)
    // ...
```

This is **already wired** — future event migrations only touch steps 1, 2, 4, 5.

### 4. Emit site — `payload.emit(handle)`

```rust
let payload = VolumeSpaceChanged { volume_id: id.to_string(), total_bytes, available_bytes };
if let Err(e) = payload.emit(app) {        // was: app.emit("volume-space-changed", &payload)
    warn!("Failed to emit volume-space-changed: {e}");
}
```

`Event` also provides `emit_to(handle, target)` and `emit_filter(handle, f)` for window-scoped emits.

### 5. Regenerate bindings + convert the TS listener

```
cd apps/desktop && pnpm bindings:regen
```

This generates into `bindings.ts`:

```ts
export const events = {
  volumeSpaceChanged: makeEvent<VolumeSpaceChanged>('volume-space-changed'),
}
export type VolumeSpaceChanged = { volumeId: string; totalBytes: number; availableBytes: number }
```

`events.volumeSpaceChanged.listen(cb)` gives a `cb` typed as `EventCallback<VolumeSpaceChanged>` (so `event.payload` is
typed). The convention (matching the `commands.*` wrappers) is a **thin named wrapper in `tauri-commands/`**,
re-exported from the barrel, not a raw `events.*` call in components:

```ts
// tauri-commands/storage.ts
import { events, type VolumeSpaceChanged } from '$lib/ipc/bindings'
export async function onVolumeSpaceChanged(cb: (payload: VolumeSpaceChanged) => void): Promise<UnlistenFn> {
  return events.volumeSpaceChanged.listen((event) => cb(event.payload))
}
```

```svelte
<!-- FilePane.svelte -->
void onVolumeSpaceChanged((payload) => {
    if (payload.volumeId === volumeId) { volumeSpace = { totalBytes: payload.totalBytes, availableBytes: payload.availableBytes } }
}).then((fn) => { unlistenSpaceChanged = fn })
```

### Gotchas proven during the reference migration

- **`mount_events` is mandatory and was not previously called** (no events existed yet). The `ipc.rs` doc comment
  already anticipated it; the wiring in § 3 is the actual hookup. Skipping it makes every `Event::emit` panic.
- **Kebab-case naming.** Keep the struct name == the event name's PascalCase. A `…Payload` suffix would change the wire
  name to `…-payload` and silently break the FE; rename the struct instead, or use `event_name`.
- **Same type-shape constraints as commands** apply (see `lib/ipc/CLAUDE.md` § Type shape constraints): no
  `skip_serializing_if`, no `serde_json::Value`, internally-tagged enums need `rename_all_fields`. An event payload
  carrying `serde_json::Value` can't be typed and must stay string-based (this is what rules out the dynamic relays
  below).
- **`bindings-fresh` is macOS-only / `NotInCI`.** Regenerate locally and commit `bindings.ts` with each batch.

## Viability verdict

**The pattern works cleanly in rc.24.** `tauri_specta::Event` + `collect_events!` + `Builder::events()` +
`mount_events()` are all present and compose with the existing command builder. No blocker. The only one-time cost
(`mount_events` wiring) is already paid.

## Event inventory

Counts from an authoritative multi-line-aware scan of `apps/desktop/src-tauri/src/` (comments and `*test*` files
excluded). The first arg of `.emit(...)` is the event name; the first arg of `.emit_to(label, "event", ...)` is a
**window label, not an event** — `"main"`, `"settings"`, and the `label` variable are emit-to targets, NOT events (this
is the miscount to avoid).

**~101 distinct literal event names** via `.emit("…")` / `.emit_to(label, "…")`, **plus** 3 const-named, **plus** 2
dynamic relays. One (`volume-space-changed`) is already migrated.

### (a) Cleanly typeable — static name + a payload struct

These have a fixed event name and a serde payload that already (or can trivially) derive `specta::Type`. The bulk of the
work. Grouped by emitting subsystem:

- **Write-operations sink** (`file_system/write_operations/types.rs`, `TauriEventSink`): `write-progress`,
  `write-complete`, `write-cancelled`, `write-error`, `write-conflict`, `write-source-item-done`, `scan-progress`,
  `scan-conflict`, `dry-run-complete`, `write-settled`. **All emitted from ONE `OperationEventSink` impl** with existing
  typed payload structs (`WriteProgressEvent`, etc.) — convert the impl, not scattered call sites. (See category (d).)
- **Listing sink** (`file_system/listing/streaming.rs`): `listing-opening`, `listing-progress`, `listing-read-complete`,
  `listing-complete`, `listing-error`, `listing-cancelled`. Same shape — one sink impl, typed payloads. (Also (d).)
- **Scan-preview** (`write_operations/scan.rs` area): `scan-preview-progress`, `scan-preview-complete`,
  `scan-preview-error`, `scan-preview-cancelled`.
- **Volumes** (`volumes/`, `volumes_linux/`, `file_system`): `volumes-changed`, `volume-mounted`, `volume-unmounted`,
  `volumes-busy-changed`, `volume-context-action`, `low-disk-space`, **`volume-space-changed` ✅ done**.
- **Indexing** (`indexing/`): `index-dir-updated`, `index-aggregation-progress`, `index-aggregation-complete`,
  `index-scan-started`, `index-scan-progress`, `index-scan-complete`, `index-replay-progress`, `index-replay-complete`,
  `index-rescan-notification`, `index-memory-warning`, `search-index-ready`.
- **MTP** (`mtp/connection/`): `mtp-device-connected`, `mtp-device-disconnected`, `mtp-transfer-progress`,
  `mtp-permission-error`, `mtp-exclusive-access-error`, `mtp-ptpcamerad-suppressed`, `mtp-ptpcamerad-restored`,
  `mtp-storage-removed`. (Some are payloadless or carry a device-id string — still typeable as a unit struct or a
  single-field struct.)
- **Network / SMB** (`network/`): `network-host-found`, `network-host-lost`, `network-host-resolved`,
  `network-host-context-action`, `network-discovery-state-changed`, `smb-connection-changed`.
- **Git** (`file_system/git/`): `git-state-changed`.
- **AI** (`ai/`): `ai-starting`, `ai-server-ready`, `ai-verifying`, `ai-installing`, `ai-install-complete`,
  `ai-extracting`, `ai-download-progress`.
- **System / misc**: `system-text-size-changed`, `accent-color-changed`, `settings-changed`, `view-mode-changed`,
  `menu-sort`, `global-shortcut-fired`, `download-detected` (also a sink, category (d)), `directory-diff`,
  `directory-deleted`, `drag-modifiers`, `drag-image-size`, `quick-look-key`, `quick-look-closed`.
- **Const-named** (literal lives in a `const`, not at the emit): `restricted-paths-changed` (`restricted_paths/mod.rs`),
  `drag-out-session-started` + `drag-out-session-complete` (`native_drag/promises.rs`), `error-report-auto-sent`
  (`error_reporter/auto_dispatcher.rs`). Typeable — just chase the const to its emit.

### (b) The `mcp-*` family — generic MCP-dispatch relay → STAYS string-based

`mcp/executor/mod.rs` and `mcp/resources/mod.rs` share a
`mcp_round_trip_with_timeout(app, event: &str, payload: Value, …)` relay: it `app.emit(event, payload)` where **`event`
is a runtime `&str` and `payload` is `serde_json::Value`**, then awaits an `mcp-response` reply event. The many `mcp-*`
names (`mcp-copy`, `mcp-move`, `mcp-delete`, `mcp-key`, `mcp-mkdir`, `mcp-mkfile`, `mcp-sort`, `mcp-set-view-mode`,
`mcp-scroll-to`, `mcp-volume-select`, `mcp-nav-to-path`, `mcp-select`, `mcp-select-names`, `mcp-open-search-dialog`,
`mcp-open-under-cursor`, `mcp-refresh`, `mcp-move-cursor`, `mcp-confirm-dialog`, `mcp-tab`, `mcp-set-setting`,
`mcp-get-all-settings`, `mcp-protocol-version`, `mcp-session-id`, `mcp-settings-close`, `mcp-response`, …) are call-site
string constants funneled through that **one generic emit**.

- They're **NOT cleanly typeable** as-is: the emit is generic over a string and the payload is free-form `Value` (a
  specta blocker). They share a single relay emit, they're not distinct typed emit sites.
- **Verdict: keep string-based** for now. Typing them would mean replacing the generic relay with N typed events +
  typing each `Value` payload — a separate, larger refactor with its own design (the MCP request/response protocol), out
  of scope for the typed-events migration. Flag, don't convert.

### (c) Window-management events — `emit_to` specific windows

`close-file-viewer`, `close-all-file-viewers`, `close-about`, `close-confirmation`, `focus-file-viewer`,
`focus-settings`, `focus-about`, `focus-confirmation`, `open-file-viewer`, `open-settings`, `viewer-word-wrap-toggled`,
`mcp-settings-close`, `tab-context-action`, `persist-restricted-setting`, `execute-command`. Most are
`emit_to("main"|"settings"|&label, "event", payload)`.

- **Typeable**, but via `Event::emit_to(handle, target)` rather than `emit`. The payloads are small/typed-able. These
  carry the cross-window-targeting nuance, so batch them together and watch the per-window capability files
  (`capabilities/{default,settings,viewer}.json`) — a typed `emit_to` still needs the listening window's permission.
- `execute-command` carries a `{ commandId }` string — typeable as a small struct.

### (d) The sink families (`OperationEventSink`, listing sink, downloads sink) — typeable, convert AT THE SINK

`file_system/write_operations/` routes all its events through the `OperationEventSink` trait (prod impl
`TauriEventSink`, test impl observes the same events). `file_system/listing/streaming.rs` has its own listing sink.
`downloads/watcher.rs` has an `EventSink` trait (`AppHandleSink` prod). Each already has **typed payload structs** and a
**single emit point per event** inside the sink impl.

- **Convert the sink impl's emit calls** (`self.app.emit("write-progress", &event)` → `event.clone().emit(&self.app)`),
  not the business logic. The payload structs gain the `Event` derive. The trait abstraction (kept for testability) is
  untouched — only the Tauri-backed impl changes. This is the cleanest sub-migration: ~16 events (10 write + 6 listing)
  collapse into two files' worth of edits.

### (e) Truly-variable relays — `emit(variable_name, …)` → MUST STAY string-based

These build the event name at runtime, so `collect_events!` (which needs a static struct → static `NAME`) can't model
them:

- **MCP relay** (`mcp/executor/mod.rs:174`, `mcp/resources/mod.rs:415`): `app.emit(event, payload)` with `event: &str`.
  Same as category (b).
- **File-viewer per-session** (`file_viewer/session.rs:1269`): `handle.emit(&event, payload)` where
  `event = format!("viewer:file-changed:{session_id}")`. The session id is interpolated into the name → inherently
  dynamic. Stays string-based (or gets redesigned to a static `viewer-file-changed` event with `session_id` in the
  payload — a behavior change, out of scope).

## Proposed partition for parallel workers

Non-overlapping groups by subsystem/file, each a self-contained batch (derive structs + convert emit sites + convert TS
listeners). All typeable events from categories (a), (c), (d) and the const-named ones. ~7 groups:

1. **Write + listing sinks (d).** `write_operations/types.rs` (`TauriEventSink`, 10 events) + `listing/streaming.rs`
   (listing sink, 6 events) + the `scan-preview-*` (4). FE consumers in `tauri-commands/write-operations.ts` +
   `file-listing.ts`. Largest, most mechanical (sink impls).
2. **Volumes + disk space.** `volumes/`, `volumes_linux/`, `space_poller.rs` (low-disk-space; volume-space-changed is
   the done reference). `volumes-changed`, `volume-mounted`, `volume-unmounted`, `volumes-busy-changed`,
   `volume-context-action`, `low-disk-space`. FE: `tauri-commands/storage.ts`.
3. **Indexing.** All 11 `index-*` / `search-index-ready` events. FE: `lib/indexing/`.
4. **MTP.** All 8 `mtp-*` events (excluding the `mcp-*` relay — different subsystem). FE: `lib/mtp/`.
5. **Network / SMB + Git.** 6 network/SMB events + `git-state-changed`. FE: `lib/file-explorer/network/`, `…/git/`.
6. **AI + system/misc.** 7 `ai-*` events + `system-text-size-changed`, `accent-color-changed`, `settings-changed`,
   `view-mode-changed`, `menu-sort`, `global-shortcut-fired`, `download-detected` (downloads sink (d)),
   `directory-diff`, `directory-deleted`, `drag-modifiers`, `drag-image-size`, `quick-look-key`, `quick-look-closed`,
   and the const-named `restricted-paths-changed` / `drag-out-session-*` / `error-report-auto-sent`.
7. **Window-management (c).** All `emit_to`-to-window events. Owned by one worker because they share the capability-file
   concern (`capabilities/{default,settings,viewer}.json`) and the `emit_to` form.

**Explicitly excluded (stay string-based):** the entire `mcp-*` relay family (b/e) and `viewer:file-changed:<id>` (e).
Note this in the migration's done-criteria so a later agent doesn't try to "finish" them.

### Orchestration — serialize the shared chokepoints

Two resources are shared and must NOT be edited concurrently by workers:

1. **`collect_events![…]` in `ipc.rs::builder()`** — every group adds its structs here. Workers should hand their list
   of struct paths to the orchestrator, who appends them in one serialized edit per merge (or workers append in
   sequence, one at a time, never in parallel — a concurrent edit here is a guaranteed conflict).
2. **`pnpm bindings:regen` + the committed `bindings.ts`** — one regen per merged batch, run by whoever merges, since
   every group's structs land in the same generated file. Parallel regens race on the file. Regenerate once after each
   group merges to `main`, commit `bindings.ts` with that group.

Everything else (the per-subsystem Rust emit sites, the per-subsystem FE listener wrappers in `tauri-commands/`) is
disjoint and parallelizable. Recommended flow: workers prep their Rust derives + emit conversions + FE wrappers on
branches, the orchestrator serializes the `collect_events!` append + `bindings:regen` at merge time, group by group, and
runs `./scripts/check.sh --check bindings-fresh --check clippy --check svelte-check` after each.
