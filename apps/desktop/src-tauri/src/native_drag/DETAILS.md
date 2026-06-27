# Native drag (macOS) details

Depth and rationale for the macOS native drag-out module. `CLAUDE.md` holds the must-knows that prevent silent
breakage; the mechanism lives here.

## How drag-out works

1. The FE starts a drag; the command resolves the session's **locality** (`locality_for_volume`, keyed on
   `Volume::supports_local_fs_access()`) and the source volume id, and calls `start_drag`.
2. `start_drag` (on the main thread) builds one `NSDraggingItem` per file:
   - **Local session**: writer is an `NSPasteboardItem` filled from the pure type plan (file-url + legacy filenames, no
     path text, matching Finder). Source carries `NO_PROMISE_SESSION`.
   - **Virtual session** (MTP, direct SMB, search-results): writer is an `NSFilePromiseProvider` per item, carrying NO
     legacy types. The providers register their delegates under a fresh `session_key`; the source carries that key.
3. Dropping on **Finder** invokes the promise delegate, which streams the real bytes off the device into the
   Finder-chosen destination via the fulfillment service. Dropping back **into Cmdr** still fires wry's drop event
   (empty paths, no panic) and the recorded-identity self-drag path handles it. Dropping on a **terminal** from a
   virtual pane is a clean no-op (no text to insert).

## File promises (the drag-out-to-Finder feature)

When Finder accepts a promise drop, it calls the delegate per item:

- `filePromiseProvider:fileNameForType:` (MAIN thread): returns the leaf name we already know, zero I/O.
- `filePromiseProvider:writePromiseToURL:completionHandler:` (operation-queue thread): `block_on`s the async
  `fulfillment::fulfill` and calls the completion block with `null` (success) or a mapped `NSError` (failure).
- `operationQueueForFilePromiseProvider:`: returns ONE shared serial queue per drag session.

### Fulfillment service (`fulfillment.rs`)

`fulfill(source_volume_id, source_path, dest_path)`: resolve the volume → busy-register the source (eject guard) →
`note_pending_write_for_cmdr(dest)` (suppress the downloads toast) → stream to the EXACT Finder leaf. A **file** goes
`open_read_stream` → `write_from_stream(dest, …)`; a **folder** is a hand-rolled recursive walk (`create_dir` → list →
mkdir → per-file stream), because the cross-volume copy engine derives landed names from source basenames and can't
target a Finder-renamed root.

**Cleanup contract (load-bearing)**: on ANY `Err`, the destination this fulfillment created is removed before
returning. `LocalPosixVolume::write_from_stream` self-cleans its partial ONLY on the cancel branch, NOT on a propagated
source-read error (device unplugged mid-stream), exactly the promise failure mode. So the service removes the partial
file (single file) or the whole created tree (`remove_dir_all`, safe because the dest is a fresh Finder-created
directory) itself. Pinned by `read_failure_midstream_leaves_no_file_at_dest…` and
`folder_error_midstream_removes_the_created_tree`.

**Main-thread invariant**: the service never performs synchronous main-thread work from the queue thread (volume I/O +
a cheap downloads-watcher mutex, no `run_on_main_thread`), so `block_on`-ing it on the queue thread can't deadlock
against a busy main thread.

### Delegate-lifetime model (load-bearing)

`NSFilePromiseProvider.delegate` is WEAK: the provider doesn't retain its delegate. A delegate that's a drag-start local
would drop when `start_drag` returns, zeroing the provider's weak ref, and Finder would query a nil delegate and
silently produce no file. So each session's delegates + providers live in process-global storage in `promises.rs`,
freed only when BOTH the gesture has ended AND every in-flight fulfillment has completed.

Two stores, because `Retained<…>` AppKit objects aren't `Send` but the in-flight counter is touched from the queue
thread:

- **`COUNTERS`** (`Send`, any-thread `Mutex<HashMap>`): `{ in_flight, gesture_ended }`. Decides *when* cleanup fires.
- **The retained store** (`thread_local!`, main-thread-confined): the `Retained` delegates + providers. Registered on
  main at drag-start; freed via a main-thread dispatch (`run_on_main`) when the counters say "ended and drained." The
  shared queue rides in the delegate's ivar as a `SendQueue` (NSOperationQueue is documented thread-safe), so returning
  it from the queue thread needs no main-thread hop.

**Ordering defended**: AppKit ends the session at the DROP, but Finder pumps the fulfillment queue AFTER. Freeing on
session-end alone would yank a delegate mid-write. Gating on "ended AND in_flight == 0" keeps everything alive across
both. A fulfillment finishing after session-end frees the session itself (its `leave_fulfillment` sees the drained,
ended state). Pinned by `session_counters_wait_for_in_flight_to_drain`.

### Busy-volume seam

The fulfillment service marks the source volume busy for the eject guard via the `pub(crate)`
`write_operations::{register_external_volume_op, release_external_volume_op}` seam (an RAII `BusyGuard` releases on
every exit). This is the smallest honest seam: a drag-out download isn't a real write op (no `WRITE_OPERATION_STATE`,
no progress events), but it must guard the device the same way, so it touches only the `OPERATION_STATUS_CACHE` half
that `recompute_and_emit_busy_volumes` reads. Pinned by `source_volume_is_busy_during_fulfillment_and_released_after`.

### App-quit / device-disconnect abort

There's no user-initiated cancel of an in-flight fulfillment in v1 (Finder owns the gesture, no progress UI). In-flight
fulfillments end via stream `Drop` semantics: app quit drops the tokio runtime and the source `VolumeReadStream` (MTP's
`Drop` cancels the USB transfer, SMB's signals its producer); a device disconnect surfaces as a `next_chunk` read
error. Either way the cleanup contract removes the partial. No explicit teardown hook is needed beyond the existing
runtime shutdown.

### Completion toasts

Finder shows nothing while a promise downloads, so Cmdr is the only feedback surface. The session storage emits two
typed `tauri_specta::Event`s (`SessionStartedEvent` / `SessionCompleteEvent`, defined in the always-compiled
`crate::system_events` because this module is macOS-only), turned into ONE toast per drag SESSION by
`lib/file-explorer/drag/drag-out-event-bridge.ts` via the typed `onDragOutSession{Started,Complete}` wrappers:

- **`drag-out-session-started`**: emitted by `enter_fulfillment` the FIRST time a session's fulfillment begins (Finder
  asked). Carries `total_items`. This is the signs-of-life affordance: the FE raises a neutral `default`-level
  persistent in-progress toast ("Downloading 3 items…") within ~1 s, so a multi-GB / slow MTP drag doesn't feel hung.
  No Cancel button (v1 stays no-user-cancel; Finder owns the gesture). The trigger is fulfillment-start, not drag-start,
  because a drag the user drops back into Cmdr never fulfills and must show nothing.
- **`drag-out-session-complete`**: emitted when the session DRAINS (gesture ended AND `in_flight == 0`), carrying the
  folded `SessionSummary` (top-level `files_succeeded` / `folders_succeeded` / `failures` leaf names). The FE replaces
  the in-progress toast in place (same `drag-out:<sessionKey>` id) with the completion toast: success counts via the
  shared `composeTransferCompleteToast` ("Copied 2 files and 1 folder."), or a failure toast naming the file(s). A clean
  no-op session (dropped on a non-Finder target, nothing ever fulfilled) summarizes to empty and emits NO event, no
  toast.

**Counts are top-level dragged items**, consistent with the selection-split contract: one dragged folder counts as one
folder regardless of how many files land inside it. The delegate records each item's `ItemOutcome` (success + `is_dir`,
or failure + leaf) on the queue thread via `leave_fulfillment`; the drain point folds them with
`session_summary::summarize`.

**Failure complements Finder, not duplicates it.** Finder shows its own NSError alert per failed item (see NSError
mapping below). Our failure toast names the file and leans on Finder for the technical detail (the failure is classified
through the typed `ListingError` pipeline). Mirrors the transfer-failure pattern.

## NSError mapping

A `FulfillError` carries a typed `ListingError` (`file_system::volume::friendly_error`) plus a `cancelled` flag. The
delegate maps it to an `NSError` in domain `com.veszelovszki.cmdr.drag-out`, using `FulfillError::nserror_title()` (a
short, category-keyed one-liner) as `localizedDescription` (Finder shows its own alert; the detailed user-facing prose
lives on the frontend). A cancelled fulfillment uses the `NSUserCancelledError` code (3072) so Finder stays quiet; a
real failure uses code 1.

## Testing

- `fulfillment.rs`: headless against `InMemoryVolume` + tempdir. Happy path asserts the landed filename EQUALS the
  Finder-chosen leaf (the regression guard against the rejected copy-engine mismatch). Plus read-failure cleanup,
  unwritable dest, missing source, recursive folder, mid-folder cleanup, and the busy-volume seam (drive a blocking
  stream, assert busy during + released after).
- `uti.rs`: extension → UTI mapping units.
- `promises.rs`: delegate smoke (construct a provider, `fileNameForType` returns the leaf), NSError domain/title/code
  mapping, and the COUNTERS session-lifetime state machine (incl. outcome accumulation across in-flight fulfillments).
  The AppKit-touching tests guard on `MainThreadMarker::new()` and skip off-main (nextest runs tests on worker threads).
- `session_summary.rs`: the pure outcome fold (empty/no-toast, single file, mixed file+folder split, failures recording
  leaf names, all-failed still surfaces a toast).
- `type_plan.rs`: the pure pasteboard policy (local byte-identical, virtual empty across every item).

## Manual verification (the Finder leg)

The Finder leg can't be automated honestly (Finder owns the drop gesture), so it's a manual protocol. Run it with the
`CMDR_VIRTUAL_MTP=1` rig (`docs/tooling/virtual-mtp.md`), plus a real phone if available:

1. Virtual MTP pane → drag one photo to Desktop → file lands, bytes correct, completion toast.
2. Multi-select 3 files → all land, names right.
3. Name collision at dest → Finder uniquifies → our file lands under THAT leaf (`sunset 2.jpg`), not the source basename.
4. Folder drag → recursive download (the hand-rolled walk populates the Finder-created dir).
5. Eject attempt mid-fulfillment → blocked (source volume busy).
6. Failure injection (disconnect / read error mid-fulfillment) → Finder error alert, NO partial file at dest, friendly
   log line.
7. App quit mid-fulfillment → clean abort, no partial file.
8. Drag back into Cmdr (self-drag) → recorded-identity path, unchanged.
9. Drag from a LOCAL pane to Finder/terminal → byte-identical to today (URL drop + terminal text).
10. Drag from a non-local pane to a terminal → clean no-op, no textClipping junk.
11. SMB-native pane variant of (1).
