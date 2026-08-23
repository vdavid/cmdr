# App-side event mapping

The wire format for subsystems that don't speak Tauri. A subsystem describes what happened as a typed value; a module
here turns it into the payload the frontend subscribes to, so wire names, payload shapes, and the words a human reads
all live on this side of the boundary.

## Must-knows

- **`index_mapping.rs` owns every drive-index / media-index frontend payload.** ❌ Don't add a `tauri_specta::Event`
  struct inside `indexing/`, `media_index/`, or `importance/`. A new frontend event needs a variant in
  `indexing/events/sink.rs`, an arm in `route`, and a line in `ipc.rs`'s `collect_events!`. Only the second is
  compiler-checked; a missing third just means the frontend never sees it.
- **`route(event, None)` suppresses only the Tauri emit**, and every host-side arm still runs. ❌ So no arm may reach
  for `app.state()`: it would drop everything in `tests.rs` and before `agent::start` runs. `FolderActivity` (the
  agent's tap) and `PathAccessDenied` use a process-global instead.
- **`route` returns the wire name it emitted**, read off the payload's own `Event::NAME`. ❌ No separate lookup table:
  it would drift, and the completeness test would then check the table instead of the behavior.
- **One event, one wire name, no exceptions.** ❌ Nothing is excused from
  `every_event_maps_to_a_destination_with_a_non_empty_name`; an exclusion hides the collision it exists to catch.
- **Payload structs live here; the values they carry don't.** `ScanRunKind`, `CoveragePhase`, and the six others keep
  their `specta::Type` derives with their subsystems: a schema derive on a value is fine there, a presentation decision
  isn't.
- **The coverage-branch hold is the one thing the sink doesn't forward on sight**, and the rule lives here: ❌ never in
  `cmdr-index` (the crate reports what it's doing), ❌ never in the frontend (which holds no timers).
- ❗ **Two `TauriEventSink` types exist in the crate**: this one (`IndexEvent`) and
  `file_system::write_operations::TauriEventSink` (`OperationEventSink`). Deliberate, but a bare grep returns both.

## Module map

- `index_mapping.rs` — the payload structs, `route`, the error-report rendering, and `TauriEventSink`.
  `index_mapping/walk_announcer.rs` — the one-second hold on the coverage-branch pair.
- `volume_mapping.rs` — `TauriVolumeEvents`, mapping `cmdr-fs`'s `VolumeConnection` onto `network`'s wire enum.

The typed side of the boundary (`IndexEvent`, `EventSink`, `IndexErrorReport`, `Diagnostic`) and the full variant
catalog: `crates/cmdr-index/src/indexing/events/DETAILS.md`. Rationale, the naming rules, the branch-hold numbers, and
why both `Copy` enums refuse to carry a payload: `DETAILS.md`.
