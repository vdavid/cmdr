# App-side event mapping details

Read this before any non-trivial work in `events/`: editing, planning, reorganizing, or advising. Must-know invariants
are in `CLAUDE.md`.

## Why this module exists

The index subsystems are being pulled into a standalone crate with no `tauri` in its dependency tree. A crate that
derives `tauri_specta::Event` can't be that crate, and a crate that writes the sentence a user reads has quietly taken
a product decision it has no business taking. So the split is: **schema derives on data are fine anywhere;
presentation decisions live here.** Events are where presentation lives — an event name is a contract with the
frontend, and an error message is copy.

The cost is real (a few hundred lines of mapping) and it buys three things: "no user-facing strings in the crate"
becomes enforceable rather than aspirational, copy gets exactly one home, and the frontend wire format can change
without touching the index.

## `route` — one match, two jobs

`route(event: IndexEvent, app: Option<&AppHandle>) -> Destination` is the whole mapping. It's a single exhaustive
match, and each arm both builds-and-emits its payload and reports where the event went.

Returning the destination from the same match that emits is deliberate. The alternative — a `wire_name(&IndexEvent)`
lookup beside a separate `emit(&IndexEvent)` — gives you two exhaustive matches over one enum that can disagree
silently: a variant could map to `"index-scan-started"` in the table while emitting an `IndexScanCompleteEvent` on the
wire, and every test would still pass. Here the name comes from `E::NAME` on the payload the arm just emitted, so a
test reading it is reading what shipped.

`Destination` names every place an event can end up, one variant each:

- `Frontend(&'static str)` — a Tauri event under that wire name.
- `ErrorReport` — routed to `raise`, which renders the report and calls `log_error!`.
- `RestrictedPaths` — routed to `restricted_paths::record_denial`, which filters to known TCC prefixes itself.
- `AnalyticsOnly` — taken off the stream by `analytics/first_index.rs` before routing; nothing renders it.
- `AgentWake` — `FolderActivity`'s rollups, handed to `agent::wake::send_rollup`.

❌ A new host-side destination never reuses a neighbour. This enum's whole job is saying where an event went, so a
shared variant makes the one type that answers that question lie.

`app: None` is the test seam: it skips the `payload.emit(app)` call and changes nothing else, so a test exercises the
real routing (including every host-side arm) with no app. `TauriEventSink::emit` is `route(event, Some(&self.app))`.

## The tap adapter (`FolderActivity`)

The one arm that feeds another subsystem rather than a UI or a report. `cmdr-index` may never name the agent
(`index-crate-isolation`), so its per-batch, per-folder rollups cross on the `IndexEvent` seam and this arm maps each
one into an `agent::wake::FolderActivity`.

Two rules it must keep, both of which fail silently if broken:

- ⚠️ **Through the process-global channel, ❌ never `app.state()`.** `app` is `None` in the completeness test and in
  every window before `agent::start` registers anything, including launch replay — the busiest window the tap will ever
  see. `PathAccessDenied` → `restricted_paths::record_denial` is the precedent.
- ⚠️ **This runs on the LIVE-LOOP thread**, synchronously, because `emit` calls `route` on the caller's thread.
  `send_rollup` takes no lock and opens no connection. The importance lookup, the quantization, and the admit all
  happen on the wake loop's own thread; canonical rationale is `agent/wake/DETAILS.md`.

## `raise` — the error-report rendering

`IndexErrorReport` describes what broke; `raise` writes the sentence. Four variants, four target categories:

- `MemoryWatchdog` → `cmdr::indexing::memory_watchdog`
- `StorageFailed` → `cmdr::indexing::store`
- `LiveEventLoopUnavailable` → `cmdr::indexing::watch::live`
- `WalkWorkerSpawnFailed` → `cmdr::indexing::scanner::walker`

The targets are stable strings, NOT `module_path!()`. The auto-dispatcher groups reports by category, and
`module_path!()` here would say `events::index_mapping` for every one of them, collapsing four distinct incidents into
a single Discord thread.

`log_error!` captures a backtrace at its call site, which is inside `raise` — but `EventSink::emit` is a synchronous
call from the failing code, so the captured stack still contains the whole index call chain, two frames deeper than
before. No diagnostic quality is lost by the indirection.

**Why an event rather than a direct call**: `log_error!` is a `#[macro_export]` macro at the app crate root. A separate
crate can't invoke it, and there's no legal alternative that reaches `auto_dispatcher::on_error_logged` — the
`desktop-rust-log-error-macro` check exists precisely to stop a raw `log::error!` from being used instead. Dropping the
five call sites during the extraction would have compiled, shipped, and silently cost the error feedback loop, which is
why `an_error_event_reaches_the_auto_dispatcher` pins it end to end through the real dispatcher statics.

## Testing

`tests.rs` holds three contracts plus one serde shape:

1. **Every kind has a sample.** `one_of_every_kind()` must cover `IndexEventKind::ALL`. The crate keeps `ALL` complete
   at compile time (the mechanism is in `crates/cmdr-index/src/indexing/events/DETAILS.md`), so a new variant reaches
   this test, which then fails until a sample exists. Together they make the next check meaningful; without them it
   would silently check a shrinking subset.
2. **Every event maps to a destination with a non-empty, unique wire name.** Duplicates are the failure that matters:
   two events on one name are indistinguishable to the frontend.
3. **An `Error` event reaches the auto-dispatcher** with its extended SQLite code intact and a stable category. Holds
   `auto_dispatcher::TEST_LOCK`, since the dispatcher's `STATE` and `ENABLED` are process-global.
4. The phase payload's camelCase serde shape, which the frontend binding reads by key.

The crate side of the boundary is tested at `indexing/tests/event_stream_tests.rs` (a real scan against a
`RecordingSink`) and `indexing/lifecycle/failure.rs` (a fatal storage error reports exactly once).
