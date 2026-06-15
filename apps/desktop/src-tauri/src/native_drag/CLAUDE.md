# Native drag (macOS)

macOS-only native drag-and-drop OUT of Cmdr. Builds the `NSDraggingSession` that carries dragged files to other apps
(Finder, terminals, editors). Driven by `start_selection_drag` / `start_drag_paths` in `commands/file_system/drag.rs`,
which hop to the AppKit main thread and call `start_drag`. The whole module is `#[cfg(target_os = "macos")]`. Full
details: [DETAILS.md](DETAILS.md).

## Files

- `mod.rs`: `start_drag` builds the `NSDraggingItem`s + drag image, attaches per-item pasteboard writers, begins the
  session. Local sessions get plain `NSPasteboardItem`s; virtual sessions get `NSFilePromiseProvider`s.
- `type_plan.rs`: pure, locality-aware pasteboard composition. Local = file-url + filenames (no path text, matching
  Finder; issue #28); virtual = empty (the textClipping fix).
- `source.rs`: `CmdrDragSource` (`MainThreadOnly`), the `NSDraggingSource`. Returns the operation mask and, on session
  end, tells the promise machinery the gesture ended so a virtual session's objects can be freed.
- `promises.rs`: the file-promise providers + delegate (`CmdrPromiseDelegate`), the shared serial queue, the
  session-lifetime storage, and `NSError` mapping.
- `fulfillment.rs`: plain-Rust fulfillment service, downloads a virtual file to the Finder-chosen destination. NO
  AppKit; unit-testable.
- `session_summary.rs`: pure per-session outcome accounting (`summarize`), folds per-item outcomes into the
  file/folder/failure counts the completion toast reads. NO AppKit/Tauri.
- `uti.rs`: pure filename-extension → UTI mapping for promise providers.

## Load-bearing invariants

Break any of these and a virtual drag-out silently produces no file, leaks objects, or leaves a partial. Mechanism for
each in [DETAILS.md](DETAILS.md).

- **Fulfillment cleanup contract**: on ANY `Err`, the destination this fulfillment created is removed before returning.
  `LocalPosixVolume::write_from_stream` self-cleans its partial ONLY on the cancel branch, NOT on a propagated
  source-read error (device unplugged mid-stream), which is exactly the promise failure mode. So the service removes the
  partial file or the whole created tree itself. Pinned by `read_failure_midstream_leaves_no_file_at_dest…` and
  `folder_error_midstream_removes_the_created_tree`.
- **Delegate lifetime**: `NSFilePromiseProvider.delegate` is WEAK. A delegate that's a drag-start local would drop when
  `start_drag` returns, zeroing the provider's weak ref, so Finder queries a nil delegate and silently produces no file.
  Each session's delegates + providers live in process-global storage in `promises.rs`, freed only when BOTH the gesture
  has ended AND every in-flight fulfillment has completed (freeing on session-end alone yanks a delegate mid-write,
  since Finder pumps the fulfillment queue AFTER the drop). Pinned by `session_counters_wait_for_in_flight_to_drain`.
- **Main-thread invariant**: the fulfillment service never does synchronous main-thread work from the queue thread (no
  `run_on_main_thread`), so `block_on`-ing it on the queue thread can't deadlock against a busy main thread.
- **Folder fulfillment is a hand-rolled recursive walk**, NOT the cross-volume copy engine: the copy engine derives
  landed names from source basenames and can't target a Finder-renamed root. The happy-path test asserts the landed
  filename EQUALS the Finder-chosen leaf (regression guard).

## Gotchas

- **The promise delegate is NOT `MainThreadOnly`.** `writePromiseToURL:completionHandler:` runs on the operation-queue
  thread, so the delegate must be usable off-main. The one main-thread-only method (`fileNameForType:`) gets its
  `MainThreadMarker` from the protocol signature; ivars are all `Send + Sync` (queue via the `SendQueue` wrapper). The
  drag SOURCE (`source.rs`) IS `MainThreadOnly` (`NSDraggingSource` requires it).
- **`session_key` is a monotonic counter, NOT the drag sequence number.** The promise delegates must register BEFORE the
  drag begins (weak refs alive the instant Finder might query them), but `draggingSequenceNumber` is only known AFTER
  `beginDraggingSessionWithItems:…` returns. A monotonic key generated up front and stashed on the source sidesteps the
  chicken-and-egg; the source reads its own key back in the end callback.
- **Completion toasts**: there's no Finder feedback while a promise downloads, so Cmdr emits typed
  `SessionStarted`/`SessionComplete` events (in `crate::system_events`, always-compiled) that
  `lib/file-explorer/drag/drag-out-event-bridge.ts` turns into ONE toast per drag session. Counts are top-level dragged
  items (one folder = one folder). A drag dropped back into Cmdr never fulfills, so it emits nothing. Detail in
  [DETAILS.md](DETAILS.md).
