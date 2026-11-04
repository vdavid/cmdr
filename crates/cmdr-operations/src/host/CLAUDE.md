# The engine's host (`host/`)

What the engine needs from the app around it, as named seams: the runtime (the `VolumeHost`'s), `OperationVolumes`,
`EngineEvents`, the two per-op sinks, `OperationSinkFactory`, `OwnWrites`, `OperationIndex`, `Pacing`, and `NameOrder`.
The app answers each in `apps/desktop/src-tauri/src/operations_host/`.

## Module map

One file per seam, its value types beside it; `mod.rs` holds `EngineHost` (every seam, as one value an `OpScope` shares)
and `announce` / `announce_on`; `testing.rs` holds the fakes (`RecordingEngineEvents`, `FakeVolumes`,
`RecordingOwnWrites`, `SettablePacing`, `pane_order_host`, `PaneListings`).

## Must-knows

- **Spawn on `host.runtime()`**, ❌ never an ambient `tokio::spawn` / `spawn_blocking` / bare `std::thread::spawn`:
  `crates/cmdr-operations/clippy.toml` refuses them. A thread the engine owns is a NAMED `std::thread::Builder` one. The
  engine's one ambient exception is `mutation_reply.rs`'s continuation, reasoned at the site; backends (`cmdr-local`,
  `cmdr_fs::deadline`) document their own.
- **A seam is called per operation, per directory, or per file landing, ❌ never per chunk or per listing entry.**
  `OwnWrites` and `Pacing` are the per-file ones; the Criterion bench is the referee.
- **❌ No seam, sink, or observer call while an engine lock is held.** Two exceptions, both so a stale report can never
  land last: the busy set reports under its dedupe lock (`EngineEvents`), and a transfer's progress emits under its
  ledger's `sent` lock (`transfer_driver/progress.rs::LeafProgressLedger::emit`).
- **A seam trait and its value types live here or in `types/`**, ❌ never in a module that calls the seam, or the two
  weld into a cycle `module-cycles` reports.
- **Every seam degrades to a no-op**; all but `OperationIndex` and `NameOrder` have a `testing` fake. Only the volumes
  and the events are required (the builder takes them positionally); the rest default in `EngineHost::new`.
- **A seam reports facts, ❌ never words**: an error report is a typed `OperationErrorReport`, an instant op's analytics
  an `InstantMutationFact` of categories. The app decides the event, the PostHog name, and the message.
- **A site passing a path calls `OperationVolumes::resolve`, ❌ never `get`**; one that stays on `get` says why.
- **A local-disk branch asks `OperationVolumes::local_disk(id)`, ❌ never `id == "root"`**: a host may have none (the
  browser demo's `root` is a volume), and then every op takes the volume engine.
- **An arrival listener runs INSIDE the registration, or a reconnect's report**: return at once, hand work to the
  runtime.

Each seam's contract, its app answer, and the runtime rules: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
