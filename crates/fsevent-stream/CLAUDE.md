# cmdr-fsevent-stream

macOS FSEvents as an async `Stream`: `create_event_stream` spawns a worker thread with its own `CFRunLoop`, and
`EventStreamHandler::abort` stops it. Our fork of the dead upstream `PhotonQuantum/fsevent-stream` (MIT, `LICENSE` keeps
the credit), now our code. Users: the drive watcher (`crates/cmdr-index/src/indexing/watch/CLAUDE.md`) and the pane
watcher (`apps/desktop/src-tauri/src/file_system/CLAUDE.md`).

## Module map

- **stream.rs**: `create_event_stream`, the worker thread, `abort`, and the four `extern "C"` callbacks (one per flag
  shape, from `define_callback!`).
- **ffi.rs**: raw CoreServices bindings, the owning `SysFSEventStream`, `SysFSEventStreamContext`,
  `impl_release_callback!`. **observer.rs**: the one-shot `BeforeWaiting` observer `abort` waits on.
- **tests.rs**: real-FSEvents tests plus context-ownership regressions; `stream.rs` holds the direct callback tests.

## Must-knows

- **Callbacks run on the worker's run loop only**, never after `FSEventStreamInvalidate`. The context outlives them
  because FSEvents frees it (via `release`) only when the stream is released, after stop + invalidate.
- **`SysFSEventStream::new` owns `context.info` on every path.** On a NULL return FSEvents has already released it;
  releasing again double frees (`ffi.rs` comment carries the evidence).
- **`eventPaths` is borrowed (Get rule)** and gone after the callback returns: collect into owned `Event`s, ❌ never
  keep a `CFArray` / `CStr` from it.
- **A callback must never unwind into C.** Its `catch_unwind` raises the overflow flag; any dropped event must too.
- **`abort` waits for the run loop to reach `BeforeWaiting`**, so a worker that never runs it would hang `abort`. Report
  failure through the run-loop channel instead.
- **First-party (`app` kind)**: every house scanner and the workspace lints apply. Tokio only; the upstream `async-std`
  arm is gone.

Context and observer ownership, the callback contract, and the verified FSEvents behaviors: `DETAILS.md`.
