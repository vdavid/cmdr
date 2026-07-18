# Out-of-process indexing (deferred escalation)

Status: deferred, 2026-07-18. This captures the design for moving drive and media indexing into a separate OS process,
the escalation option we chose NOT to take. The in-process safety net (thread QoS + bounded logging, below) closed the
actual levers, so this stays on the shelf as a documented fallback, not planned work.

## The problem it would solve

Indexing runs in the app process today. A pathological background loop (a dead index DB spinning a failing-retry loop, a
runaway scan) can contend with the thread that services the UI and IPC, and with a shared resource like the log file.
One real incident pegged ~190% CPU and froze the webview (a separate process) through CPU plus synchronous log-write
contention.

Moving indexing to its own process gives the OS scheduler a hard boundary: the indexer's threads can never be the same
threads that serve the UI, and the kernel arbitrates CPU, memory, and I/O between two processes. It's the only design
that makes "a runaway indexer can never starve the UI" a structural guarantee rather than a defended invariant.

## Why it's not needed now

Two in-process levers were closing the actual gap, and both are cheaper and lower-risk than a process split:

1. **Thread QoS.** The heavy indexing threads (writer, scanner, walker workers plus watchdog, local reconcile) now set
   `QOS_CLASS_UTILITY` at thread start (`src-tauri/src/thread_qos.rs`). Under contention macOS lets the UI's
   user-interactive threads preempt them, so indexing yields the core without a process boundary.
2. **Bounded logging.** The file-log writer now coalesces identical-line floods (`src-tauri/src/logging/coalesce.rs`),
   so a runaway loop can't peg a core through `write` syscalls or stall other threads on the log mutex. The synchronous
   write path is preserved, so the crash tail stays complete.

Plus the source of the original incident is fixed independently: fatal storage errors now stop and fail the index
instead of retrying forever (`indexing/CLAUDE.md` § "A fatal storage error STOPS"). With the source stopped and both
levers closed, the remaining risk doesn't justify the cost below. Revisit only if a new starvation path appears that
neither QoS nor coalescing can contain (see "When to revisit").

## What it would take in this codebase

Indexing is deeply woven into the app process. A process split is a large refactor, not a lift-and-shift. The main
seams:

- **`INDEX_REGISTRY` is the in-process authority.** `Mutex<HashMap<VolumeId, IndexInstance>>` guards every volume's
  lifecycle, and it's reached from around three dozen distinct call sites (roughly 100 line references across the
  module: lifecycle commands, the reconciler, freshness, MCP and IPC reads). In a split, the registry lives in the
  indexer process and every one of those reads or mutations that currently happens in the app process becomes an RPC or
  a cache. This is the bulk of the work.
- **`AppHandle`-bound event emits.** The indexer emits progress, phase, and completion events to the frontend via
  Tauri's `Event::emit` (about 25 emit sites in `indexing/`), and `IndexManager` itself carries an `AppHandle`
  (`manager.rs`, `pub(super) app: AppHandle`). Tauri events only exist in the app process. Across a boundary the indexer
  would emit onto an IPC channel (a pipe or local socket) that the app process forwards to the webview. The `APP_HANDLE`
  `OnceLock` in `commands/indexing.rs` and the manager's `app` field both need replacing with that channel.
- **Status reads become RPC.** The MCP `cmdr://state` resource and the IPC index-status commands read registry and
  per-volume state synchronously today. They'd become request/response calls to the indexer, with the app-side timeout
  discipline (`blocking_with_timeout`) extended to cover a possibly-busy or crashed indexer process.
- **Shared `Arc`s don't cross a process boundary.** `ReadPool` (per-volume read connections), `PendingSizes` (the global
  `LazyLock<Mutex<Option<Arc<PendingSizes>>>>`), and the per-volume `Freshness` arcs are shared in-memory state that the
  app and indexer both touch. Each has to be reassigned an owner: either it moves wholly into the indexer and the app
  reads it via RPC, or it stays app-side and the indexer stops depending on it. `ReadPool` is the interesting one:
  search opens the index read-only, so readers can stay in the app process (see below), but the write-side pool must go
  with the writer.

## Data-safety angle (the good news)

The storage model splits cleanly along the process boundary:

- **One WAL DB per volume, single-writer discipline.** Each volume has its own SQLite file in WAL mode
  (`indexing/store/mod.rs`), with exactly one writer thread and a separate write connection (`store/connection.rs`). WAL
  is designed for exactly this: one writing process, many reading processes/connections. Moving the single writer into
  the indexer process changes nothing about the invariant; it just relocates the writer.
- **Search already reads cross-boundary-safe.** Readers open the index read-only (`store/connection.rs`, the "global
  read-only store"), and WAL lets a reader in one process see a writer in another. So search could stay in the app
  process, reading the same DB files the indexer writes, with no new coordination beyond what WAL already provides.
- **No shared mutable file state to referee.** Because it's one-writer-per-file with read-only readers, there's no
  multi-writer contention to design around. This is what makes the split tractable at all: the hard part is the control
  plane (registry, events, status), not the data plane.

## Prior art in this repo

The AI feature already runs a sidecar: `ai/process.rs` spawns `llama-server` as a child process
(`std::process::Command`), writes its output to a log file, and manages its lifecycle (it's stateless, so `SIGKILL` is
safe and macOS reclaims its GPU/Metal/mmap resources on exit). That establishes the pattern for spawning, supervising,
and log-capturing a sidecar. An indexer sidecar is more involved (it's stateful and bidirectional, where llama-server is
stateless request/response), but the process-management scaffolding exists and is proven.

## Effort magnitude and tradeoffs

- **Magnitude: large.** The data plane is easy (WAL already supports it); the control plane is the cost. Reworking
  around three dozen `INDEX_REGISTRY` touch points, ~25 event emits, and the status reads into an RPC surface, plus a
  supervised sidecar with its own crash/restart handling, is a multi-week effort with real regression surface across the
  whole indexing lifecycle.
- **New failure modes.** A sidecar can crash, hang, or be killed by the OS independently of the app. The app then needs
  supervision (detect death, restart, back off), and the UI needs an honest "indexer unavailable" state distinct from
  "indexing disabled." This is more moving parts than the current in-process model, which fails as one unit.
- **IPC overhead.** Every status read and event emit crosses a process boundary (serialization plus a socket/pipe hop).
  Negligible for progress events, but the synchronous status reads on hot paths (volume switching, `cmdr://state`) would
  need caching to stay sub-millisecond.
- **What it buys.** A structural guarantee that indexing can't starve the UI, and independent resource accounting (the
  OS shows the indexer's CPU and memory separately, which also helps triage). Worth it only if the in-process levers
  stop being enough.

## When to revisit

Reconsider this escalation if any of these hold:

1. A new starvation incident traces to a path that thread QoS and log coalescing can't contain (for example, memory
   pressure or a syscall storm that QoS doesn't throttle).
2. Indexing grows a component that genuinely wants a separate address space (a heavy native library, an unsafe
   third-party indexer, a crash-prone codec) where in-process failure would take down the app.
3. We want independent, OS-visible resource accounting and throttling for indexing as a product feature, not just a
   safety net.

Absent those, the in-process design stays. This document is the starting point if the decision flips.
