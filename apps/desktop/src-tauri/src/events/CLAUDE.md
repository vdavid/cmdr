# App-side event mapping

The wire format for subsystems that don't speak Tauri. A subsystem describes what happened as a typed value; a module
here turns it into the payload the frontend subscribes to, so wire names, payload shapes, and the words a human reads
all live on this side of the boundary.

## Must-knows

- **`index_mapping.rs` owns every drive-index / media-index frontend payload.** ❌ Don't add a `tauri_specta::Event`
  struct inside `indexing/`, `media_index/`, or `importance/` — those emit `IndexEvent` and name no wire format. A new
  frontend event needs three things: a variant in `indexing/events/sink.rs`, an arm in `route`, and a line in `ipc.rs`'s
  `collect_events!`. The `route` match is exhaustive, so the compiler catches the second; nothing catches a missing
  third but the frontend never seeing the event.
- **`route` emits AND returns the wire name it emitted under**, read off the payload's own `Event::NAME`. Don't add a
  separate name-lookup table: it would drift from what actually ships, and the completeness test would then be checking
  the table instead of the behavior.
- **The coverage-branch pair is the ONE thing the sink doesn't forward on sight.** `WalkAnnouncer` holds a branch-started
  event for a second and drops it entirely if the walk ends first, because a phase announces 50-150 branches and most
  finish in well under a second. The rule lives here, ❌ never in `cmdr-index` (the crate reports what it's doing) and ❌
  never in the frontend (which then holds no timers and renders what it's told). An END is never held back, and a run's
  terminal event closes any branch still open.
- **`route(event, None)` suppresses only the Tauri emit.** The error-report and restricted-path arms still run, which is
  how `tests.rs` proves a crate-side failure reaches `auto_dispatcher` without standing up an app.
- **`IndexEvent::Error` is the index's only path to a shipped error report** (a subsystem can't invoke the crate-root
  `log_error!` macro). The English sentence is written here because that's what a human reads; the subsystem ships the
  numbers. The backtrace is still the failure's — `emit` is a synchronous call from the failing code.
- **Payload structs live here; the values they carry don't.** `ScanRunKind`, `CoveragePhase`, `RescanReason`,
  `ActivityPhase`, `Freshness`, `AggregationPhase`, `MediaEnrichTerminalReason`, and `IndexFailure` keep their
  `specta::Type` derives with their subsystems. A schema derive on a value is fine there; a presentation decision
  isn't — and there is no presentation type for the drive-index phase on this side: what each phase is CALLED is a
  message key in the frontend's catalog (`src/lib/indexing/indexing-steps.ts`).
- **One event, one wire name, no exceptions.** `every_event_maps_to_a_destination_with_a_non_empty_name` checks every
  routed name is unique, and ❌ nothing is excused from it: an exclusion there would hide exactly the collision it is
  written to catch.

## Module map

- `index_mapping.rs` — the 18 payload structs, `route`, the error-report rendering, and `TauriEventSink`.
  `index_mapping/walk_announcer.rs` — the one-second hold on the coverage-branch pair (below).
- `volume_mapping.rs` — `TauriVolumeEvents`, which turns a storage backend's typed connection transitions into
  `VolumeConnectionChanged`, mapping `cmdr-fs`'s `VolumeConnection` onto `network`'s wire enum in the one match where
  the two meet.

There are TWO `TauriEventSink` types in the crate: this one (for `IndexEvent`) and
`file_system::write_operations::TauriEventSink` (for `OperationEventSink`). Deliberate — each is its area's Tauri
sink, and both are always constructed through their module path — but a bare grep for the name returns both.

The typed side of the boundary (`IndexEvent`, `EventSink`, `IndexErrorReport`, `Diagnostic`) and the full variant
catalog: `crates/cmdr-index/src/indexing/events/DETAILS.md`. Rationale and the naming rules for this side: `DETAILS.md`.
