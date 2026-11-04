# cmdr-fsevent-stream details

## Why a fork, and why it lives here

- Upstream `PhotonQuantum/fsevent-stream` is dead (last push 2024-10-18). We needed changes it never took: the overflow
  flag, `from_bits_retain` for unknown flag bits, and the soundness fixes below.
- The separate mirror `vdavid/cmdr-fsevent-stream` is archived; the monorepo crate is the only copy, never published to
  crates.io. The package keeps the `cmdr-fsevent-stream` name so dependents don't change.
- The upstream MIT `LICENSE` stays, and `THIRD-PARTY-NOTICES.md` keeps crediting it. That's why the manifest has no
  `publish = false`: cargo-about's `[private] ignore` would drop the crate, credit and all.

## Lifecycle and threading

1. `create_event_stream` boxes a `StreamContextInfo` (channel sender + overflow flag) into the FSEvents context and
   creates the stream on the caller's thread.
2. The worker thread schedules it on its own run loop, starts it, sends the `CFRunLoop` back, and runs the loop. If
   `FSEventStreamStart` fails it sends `None` instead and `create_event_stream` returns `Err`.
3. `abort` adds a one-shot `BeforeWaiting` observer, waits until the loop is idle (so the stop isn't lost before the
   loop starts), then `CFRunLoopStop`s it and joins the worker. The worker stops, invalidates, and drops the stream.
4. The drop's `FSEventStreamRelease` makes FSEvents call `release_context`, which frees the boxed context.

The `Send` impls rest on CF retain/release and the CFRunLoop calls `abort` makes being callable from any thread (Apple's
Thread Safety Summary and Threading Programming Guide § Run Loops).

## Ownership rules

- **Stream context**: `FSEventStreamCreate` copies the struct; with `retain` NULL, the stream owns `info` and calls
  `release` once, on deallocation. `SysFSEventStream::new` releases it itself when it fails before calling FSEvents.
- **Observer context**: `CFRunLoopObserverCreate` copies the struct too, so it's stack-allocated; the observer owns the
  boxed `ObserverContextInfo` and releases it on deallocation.
- **Stream ref**: Create rule, one `FSEventStreamRelease` in `Drop`. Observer ref: Create rule, adopted by
  `wrap_under_create_rule`.

## Callback contract

The flags pick the callback, and the callback trusts the `eventPaths` shape they imply: `CFArray<CFString>` with
`UseCFTypes`, `CFArray<CFDictionary>` (`path` → `CFString`, `fileID` → `CFNumber`) with `UseExtendedData`, and `char **`
without `UseCFTypes`. The full contract is the doc comment on `define_callback!` in `stream.rs`. The extended-data keys
are `CFSTR` macros in the SDK, not symbols, so `extended_data_path_key()` builds them with
`CFString::from_static_string`.

## Verified FSEvents behaviors

All verified on macOS 27.0.1 with throwaway probe tests, 2026-11-03:

- `FSEventStreamCreate` with an empty path array returns NULL, and calls the context's `release` before returning.
- `FSEventStreamStart` on an unscheduled stream returns false; `FSEventStreamStop`, `FSEventStreamFlushSync`, and a
  repeated `FSEventStreamInvalidate` on an unscheduled stream are no-ops. That's why `SysFSEventStream`'s methods can be
  safe.
- A path `CFURLCreateFromFileSystemRepresentation` rejects (the empty string) fails in `new` before FSEvents sees it.

## The real-FSEvents delivery test

`must_receive_fs_events` runs five flag combinations against a live `fseventsd`. Its producer paces each create and
delete on the consumer echoing that step's delivery back (`wait_for_delivery`, bounded at 2 s a step), never on a fixed
sleep: a create+delete with no gap coalesces into one event, and a just-armed watch can drop the first mutation, so it
keeps making fresh pairs until one lands whole.

## Soundness fixes over upstream (regression anchors)

- NULL stream from an empty path list used to be scheduled, started, and released:
  `empty_path_list_is_an_error_and_releases_the_context_once`, `create_event_stream_rejects_an_empty_path_list`.
- A rejected path leaked the boxed context: `invalid_path_is_an_error_and_releases_the_context_once`.
- The boxed observer context struct leaked on every `abort`: `dropping_the_observer_releases_its_context` pins the
  ownership rule.
- A callback panic dropped the batch without raising overflow: `panic_while_building_a_batch_flags_overflow`.
- A failed `FSEventStreamStart` left `abort` waiting forever (no test: Apple says start "ought to always succeed").
