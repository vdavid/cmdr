# Drag out to other apps from MTP and SMB panes (file promises)

Plan for making drag-and-drop OUT of non-local panes (MTP devices, smb2-native shares) actually work: dropping a photo
from a phone's pane onto the Desktop downloads it there, the way Photos.app does it. Mechanism: macOS file promises
(`NSFilePromiseProvider`) — the drag carries a _promise_ to produce the file, and we stream the bytes from the device
only when an external destination asks.

This document captures the **intention** behind each decision so the implementing agent can adapt details when reality
pushes back, as long as the intentions stay intact.

## Why

- **The live incident**: dragging `sunset.jpg` from the virtual-MTP pane to Finder created a junk file
  `photos:sunset.jpg.textClipping` containing the text `/photos/sunset.jpg`. Mechanics: our drags publish a `file://`
  URL (bogus for virtual paths — the file doesn't exist locally), `public.utf8-plain-text` with the shell-escaped path
  (for terminal drops), and legacy `NSFilenamesPboardType` (stock wry's `collect_paths` reads it for our own in-app drop
  events). Finder ignores the dead URL, falls back to the text type, and materializes it as a text clipping. The current
  state is documented as a known limitation in `src/lib/file-explorer/drag/CLAUDE.md` — this plan replaces the
  limitation with the real feature.
- **Design-principles tie-ins**: platform-native, not generic (file promises are _the_ macOS-native answer; Photos,
  Mail, and browsers all do this); delightful UX (the gesture every Mac user already knows just works); rock solid
  (failure surfaces an honest error, not silence); protect the user's data (a failed download must not leave a
  plausible-looking partial file at the drop location).

## Current state (verified facts the design builds on)

- `src-tauri/src/native_drag.rs` (~290 lines): one `NSDraggingItem` per file; per-item `public.file-url`; EVERY item
  carries `public.utf8-plain-text` (first item: all paths shell-escaped + space-joined; later items: their own path) and
  the FIRST item carries `NSFilenamesPboardType` (`native_drag.rs:146-160`). The source-side drag image is set via
  `setDraggingFrame:contents:` per item — writer-agnostic, so promise-backed items keep the rich image; the count badge
  is system-rendered for >1 `NSDraggingItem`. (`drag_image_swap.rs` is a DIFFERENT mechanism: a drop-target-side swizzle
  that only fires for self-drags re-entering our window — irrelevant to drag-out; don't conflate them.)
- **wry is locked at 0.54.4 and wry#1723 has NOT shipped**: its `collect_paths` (`wkwebview/drag_drop.rs:18-33`) still
  `unwrap()`s `propertyListForType(NSFilenamesPboardType)` — BUT only inside `if pb.availableTypeFromArray(...)` says
  the type is available. Today that's always true for our drags because AppKit AUTO-DERIVES `NSFilenamesPboardType` from
  `public.file-url`. A promise-only item carries no file-url, so the expectation is: no auto-derivation →
  `availableTypeFromArray` returns none → `collect_paths` returns an EMPTY vec, no panic. This is the hypothesis M0 must
  confirm (not "filenames must always be present").
- `objc2-app-kit` 0.3.2 exposes `NSFilePromiseProvider` + `NSFilePromiseProviderDelegate`. The delegate methods:
  `filePromiseProvider:fileNameForType:` (runs on the MAIN thread — takes a `MainThreadMarker`) and
  `filePromiseProvider:writePromiseToURL:completionHandler:` (runs on the provider's `operationQueue`; gated behind the
  `block2` cargo feature — the completion handler is a `block2` block taking `*mut NSError`). So TWO cargo feature
  additions are expected: `NSFilePromiseProvider` and `block2`. (Note the selector: `writePromiseToURL:` — NOT the
  older, deprecated `writePromisedFileTo:` mechanism from `NSFilesPromisePboardType`.)
- Two drag-start commands (`commands/file_system/drag.rs`, both hop to the AppKit main thread): `start_selection_drag`
  (resolves paths from the listing cache — the listing knows its volume) and `start_drag_paths` (raw paths: single-file
  drags and the search-results pane, whose paths are absolute local).
- **A single drag can never mix local + virtual items**: selections are single-pane and panes are single-volume.
  Locality is a property of the drag SESSION, not of individual items.
- **In-app drops never read the pasteboard for identity anymore**: self-drags carry `{sourceVolumeId, sourcePaths}`
  through module drag state (`drag/drag-drop.ts`, the recorded-identity mechanism), consumed only for detected
  self-drags whose volume is backend-registered. The recorded-identity path doesn't read dropped paths — but it DOES
  require wry's drop EVENT to fire (an empty path vec is fine; a panic is not). Promises are fulfilled only when an
  _external_ consumer asks, so external fulfillment cannot affect in-app behavior.
- `Volume::open_read_stream` streams from every backend (MTP: owned download whose `Drop` cancels the USB transfer; SMB:
  channel-backed reader whose `Drop` signals its producer; see `file_system/volume/CLAUDE.md` § Streaming patterns).
  `Volume::write_from_stream(dest, size, stream, on_progress)` takes an EXACT destination file path and, on the local
  backend, makes the file durable before returning (`sync_data` + best-effort parent-dir fsync) — both are plain async
  fns returning `Result`, directly awaitable.
- The cross-volume copy engine (`copy_between_volumes` / `copy_volumes_with_progress`) takes a destination DIRECTORY and
  derives landed filenames from source basenames (`volume_strategy.rs:331,757`). It has no "land this one source at
  exactly THIS dest filename" mode.
- MTP is serial USB (`max_concurrent_ops() == 1`); concurrent reads against one device queue behind the device lock.
- The busy-volumes set (`write_operations/state.rs::register_operation_status`) disables Eject while an op touches a
  device — and the server-side `eject_volume` guard reads `busy_volume_ids()`, so registering is what actually blocks
  eject, not just picker UX. It is an ENGINE-layer mechanism: `write_from_stream` alone does not register, AND the
  register/unregister helpers are `pub(super)` (visible only inside `write_operations`) — the fulfillment service needs
  a small `pub(crate)` seam exposed (see M2).
- **`LocalPosixVolume::write_from_stream` self-cleans its partial ONLY on the cancel branch** (`ControlFlow::Break`,
  `local_posix.rs:569-575`); a mid-stream SOURCE-read error (`chunk_result?` at `:550`) propagates and LEAVES the
  partial at the destination. That read error is exactly the promise failure mode (device unplugged while streaming to a
  Finder-chosen local path). Don't pattern-match on `SmbVolume::write_from_stream`, which WAS hardened to delete
  partials on error — the local writer was not, because its engine callers own cleanup at a higher layer.
- `note_pending_write_for_cmdr` (the downloads-watcher Cmdr-own-write hook) is called by the ENGINE's wrappers, not by
  `write_from_stream` itself — a thin path must call it directly before writing, or dragging a phone photo into
  `~/Downloads` pops a spurious "Downloaded …" toast. It's a cheap prefix-scoped mutex (no main-thread hop) and a no-op
  for destinations outside Downloads.
- `quick_look/controller.rs` is the in-repo precedent for a `define_class!` NSObject delegate with ivars, main-thread
  handling, and `ProtocolObject` conformance.

## Scope / non-goals

- **macOS only.** The Volume trait keeps working everywhere; the promise machinery is `#[cfg(target_os = "macos")]` next
  to the existing native-drag code (which is already macOS-only).
- **Drag out of LOCAL panes is unchanged** — real file URLs work everywhere today; promises would only slow Finder down
  (it prefers promises when present). Promises are for non-local panes only.
- **In-app drops unchanged** (recorded identity; see facts above).
- **No drag-INTO-Cmdr changes.** No new Tauri commands are expected, so no capability-file changes (confirm at the end
  of M2 — AGENTS.md's silent-permission-failure rule is why this sentence exists).
- **The search-results pane** drags absolute local paths (`start_drag_paths`) — local, unchanged.
- **Drag out to terminals from non-local panes intentionally stops inserting path text** — the volume-relative string
  was meaningless outside Cmdr anyway (it's what the textClipping bug materialized). Local-pane drags keep the terminal
  text affordance. Mention in the changelog.
- **No user-initiated cancel of an in-flight fulfillment in v1.** There is no UI that could trigger it (Finder owns the
  gesture; we add no progress dialog in v1 — see M3). The v1 cancellation story is: clean abort on app quit and on
  device disconnect, never leaving a partial file. If M3's progress affordance later grows a Cancel button, the
  fulfillment service's `Result` shape already accommodates it.

## The load-bearing unknowns (resolve FIRST — milestone M0)

Two genuine unknowns remain (the wry version, the objc2 features, and the engine-API shape were resolved from the tree
and moved to "Current state" above):

1. **Pasteboard layout for virtual drags: does wry's drop event still fire, and does Finder behave?** Hypothesis (see
   Current state): promise-only items → no file-url → no `NSFilenamesPboardType` auto-derivation → `collect_paths`
   returns empty without panicking → wry's `DragDropEvent::Drop` still fires with an empty vec → `handleDrop` runs →
   recorded identity handles self-drags. The spike is a CONFIRMATION with a predicted winner, not an open four-way race:
   **(a) promise-only items is the target layout**; **(c) promise items + an EMPTY/sentinel `NSFilenamesPboardType` is
   the fallback** if (a)'s drop event fails to fire; **(b) promise items + a filenames item carrying the volume-relative
   paths, and (d) the current layout, are negative controls** — expected to reproduce the garbage-copy / textClipping
   failure modes, tested only to document WHY they lose. Never ship (b). For each layout observe BOTH sides in a
   throwaway dev build: does the in-app drop event fire (and with what paths)? Does Finder fulfill the promise, attempt
   to copy garbage paths, both, or neither?
2. **Directory promises**: verify Finder accepts a promise with UTI `public.folder` and calls `writePromiseToURL:` once
   with a directory URL we then populate recursively. Note the cost asymmetry for the decision: with the thin per-file
   fulfillment path (see Architecture), folder support means hand-rolling the recursive walk (list → mkdir → per-file
   `write_from_stream`), since the copy engine's recursive walker derives names from sources and can't be pointed at a
   Finder-renamed root. If folder promises are flaky OR the recursion cost crowds v1, ship file-only v1 (multi-select
   still works) and document folders as a fast-follow. Decide on evidence.

M0's output is a short findings note in `docs/notes/drag-out-promises-spike.md` (a temporary reference note per the docs
conventions, linked from the drag CLAUDE.md), recording: the chosen layout with observed behavior per candidate, and the
folder verdict. A throwaway branch/scratch build is fine; nothing from the spike merges except the note and the two
Cargo feature additions (`NSFilePromiseProvider`, `block2`).

## Confirmed behavior (the contract)

- Dragging files (and folders, pending M0 #2) from an MTP or smb2-native pane to Finder/Desktop **downloads them to the
  drop location, under the exact filename the destination chose** — Finder uniquifies collisions ("sunset 2.jpg") and
  our fulfillment must honor that leaf, not the source basename. Multi-select works. The drag image and count badge keep
  working (source-side `setDraggingFrame:contents:` is writer-agnostic; badge is system-rendered for >1 item — see
  Current state).
- Dropping the same drag back **into Cmdr** keeps today's recorded-identity behavior. The drop event must fire (empty
  pasteboard paths are fine and expected for promise-only layouts).
- Dragging from non-local panes to a **terminal** no longer inserts text (see non-goals). Dragging to apps that accept
  neither promises nor our remaining types is a clean no-op — **no more textClipping junk**, regardless of destination.
- **Failure is loud and safe**: a fulfillment that fails (device unplugged, read error, destination unwritable) reports
  an `NSError` through the promise completion handler (Finder surfaces its own alert) AND cleans up the partial
  destination file — never leave a half-written file that looks complete. Error text rides the existing `FriendlyError`
  copy where a mapping exists.
- **App quit during fulfillment aborts cleanly**: in-flight read streams drop (MTP's `Drop` cancels the USB transfer;
  SMB's signals its producer), the partial destination is removed, the completion handler gets a Cancelled-shaped
  `NSError`.
- **Eject is guarded during fulfillment**: the source volume registers in the busy-volumes set for the duration, so the
  user can't eject the phone mid-download (same contract as every other transfer).
- **Progress affordance** (the no-Finder-progress-UI reality): Finder shows nothing while a promise fulfills. v1:
  completion/failure surfaces via the standard transfer toasts ("Copied 3 files."), and for a large fulfillment (total >
  ~100 MB or > 30 s estimated) ALSO raise a signs-of-life affordance at start so a multi-GB video drag doesn't feel
  hung. Exact surface is an M3 decision; the intention — visible signs of life within ~1 s for big drags — is fixed.
  (Radical transparency principle.)

## Architecture decisions

**Decision: fulfillment is a thin per-file path — `open_read_stream` → `write_from_stream` to the EXACT Finder-supplied
URL — plus explicit busy-volume registration. The cross-volume copy engine is the REJECTED alternative.** The promise
callback hands us a per-file destination URL whose leaf Finder may have uniquified ("sunset 2.jpg"). The copy engine
plans into a destination _directory_ and derives landed names from source basenames — routing through it would silently
ignore Finder's chosen name and re-collide at the drop location. `write_from_stream` takes the exact dest path and
already keeps the durability promise on the local backend (`sync_data` + parent-dir fsync before returning). What the
thin path must add explicitly, because it lives in the engine layer and NOT in `write_from_stream`: busy-volume
registration for the source device, released on completion (eject guard; the current helpers are `pub(super)`, so M2
exposes a small `pub(crate)` seam), `note_pending_write_for_cmdr(dest)` before the write (the downloads-watcher
own-write hook the engine wrappers normally call), partial-file cleanup on failure, and typed error mapping through the
`FriendlyError` pipeline. The destination is local, so FSEvents covers the listing-cache mutation notification — no
manual `notify_mutation` needed (the documented local-backend exception). The fulfillment service is a plain async fn
returning `Result` — the delegate `block_on`s it from the operation-queue thread and gets the outcome as a VALUE; no
Tauri event listening anywhere in this path. _Adaptation latitude_: if M0 #2 brings folder promises into v1, the
recursive walk is hand-rolled at this layer (list → mkdir → per-file stream), reusing the same per-file primitive; do
not bend the copy engine into a Finder-renamed root.

**Decision: one `NSFilePromiseProvider` per dragged item; fulfillments execute on a single serial `NSOperationQueue` per
drag session.** Finder calls `writePromiseToURL:` per item on the provider's `operationQueue`. We supply one shared
serial queue for the session: MTP is serial USB anyway (parallel fulfillments would just contend on the device lock),
ordering is predictable, and a session-level "N of M done" progress notion stays computable. SMB could parallelize; v1
favors one code path — note the loosening point in the module docs.

**Decision: the delegate is a `define_class!` NSObject following the `quick_look/controller.rs` precedent, with a Rust
"fulfillment service" doing all real work — and a hard main-thread invariant.** The Objective-C surface stays
paper-thin: `fileNameForType:` (MAIN thread — return the leaf name we already know, zero I/O, zero locks shared with
fulfillment) and `writePromiseToURL:completionHandler:` (queue thread — forward to the service, call the completion
block with nil or the mapped NSError). ALL logic — volume resolution, streaming, cleanup, error→NSError mapping — lives
in a plain Rust module that unit tests drive with no AppKit. **Invariant: the fulfillment service never performs
main-thread work synchronously from the queue thread** (the main thread may be busy or itself waiting; a sync hop would
deadlock). Volume I/O + tokio runtime work satisfies this naturally; the note_pending downloads-watcher hook on the dest
path is a cheap mutex (and usually a no-op — Finder destinations are rarely inside Downloads), not a main-thread
dispatch. State the invariant in the module doc. Delegate ivars carry only `Send + Sync` handles, per the quick-look
precedent.

**Decision: pasteboard composition becomes an explicit, locality-aware type plan — a pure function with tests — keyed by
the DRAG SESSION's locality, not per item.** Mixed local+virtual drags are impossible (single-pane selections,
single-volume panes) — encode that as an invariant (assert-or-reject at the boundary), not a runtime branch. Local
sessions keep today's layout byte-for-byte (file-url per item; text on every item with the first-item join; filenames on
the first item). Virtual sessions: promise providers, and NO file-url / NO text / filenames-per-M0's-verdict — across
EVERY item, not just the first (the current `i == 0` branching is exactly where a partial strip would hide). This pure
function is where the wry constraint, the Finder interplay, and the terminal-affordance removal become visible, testable
policy.

**Decision: UTI per item derives from the filename extension via a small pure mapping (fallback `public.data`; folders
`public.folder` pending M0 #2).** No new dependency: `NSFilePromiseProvider.fileType` takes a UTI string. Pure,
unit-tested; unknown extensions degrade to `public.data`, which Finder accepts for any file.

**Decision: a fulfillment that fails deletes its partial destination on ANY `Err`, then reports.** The destination is
Finder-chosen; a half-downloaded `IMG_2031.mov` at the drop location is indistinguishable from a complete one — worse
than no file. Cleanup is the fulfillment service's own responsibility on this path, and the "ANY `Err`" wording is
load-bearing: `LocalPosixVolume::write_from_stream` self-cleans its partial ONLY on the cancel branch
(`ControlFlow::Break`), NOT on a propagated source-read error — which is exactly the disconnect-mid-stream failure mode
this feature hits. Do not pattern-match on the SMB writer (which was hardened to clean on error); the local one
deliberately leaves cleanup to its callers. The `NSError` surfaces after cleanup. Pin with a test (read-failure
mid-stream → NO file at dest, error returned).

## Milestones

Sequential. M0 gates everything; M1 is independently shippable (it kills the junk-file bug on its own); M2 is the
feature; M3 polish; M4 verification.

### M0 — Spike: resolve the two remaining unknowns

Intent: replace the two guesses with recorded facts before any real code. Timebox ~half a day; produce
`docs/notes/drag-out-promises-spike.md` with the chosen pasteboard layout (observed wry-event + Finder behavior per
candidate, incl. whether the drop event fires with empty paths) and the folder-promise verdict. Merge only the note and
the `NSFilePromiseProvider` + `block2` cargo feature additions.

### M1 — Session-locality pasteboard composition (ships the textClipping fix standalone)

Intent: virtual paths stop publishing representations external apps can materialize as garbage, before promises even
exist.

- Extract the pure type-plan policy + unit tests (local session byte-identical to today; virtual session strips
  file-url/text across EVERY item; filenames per M0; mixed sessions rejected as impossible-by-construction).
- The drag commands learn session locality: `start_selection_drag` already has the listing (volume-scoped);
  `start_drag_paths` callers pass what they know (the FE drag-start path has `sourceVolumeId` since the
  recorded-identity work — thread it to the BE command as an optional param rather than re-deriving).
- Behavior after M1 alone: dragging from MTP to Finder is a clean no-op; in-app virtual drags still work (drop event
  plus recorded identity, per M0's verified layout); local drags byte-identical.
- Tests: pure-policy units; existing drag suites stay green; manual spot-check of the M4 protocol's rows 6-8.

### M2 — File promises: provider, delegate, fulfillment service

Intent: the feature. Dropping on Finder downloads, under Finder's chosen names.

- `native_drag/promises.rs` (or sibling): the `define_class!` delegate (quick-look precedent; `fileNameForType:` on
  main, `writePromiseToURL:completionHandler:` on the queue — get the selectors right, see Current state), the shared
  serial `NSOperationQueue`, and the Rust fulfillment service.
- Expose a `pub(crate)` busy-volume seam from `write_operations` (the existing `register_operation_status` /
  `unregister_operation_status` are `pub(super)` and unreachable from the promises module) — a thin wrapper like
  `register_external_volume_op(op_id, volume_ids)` that keeps `recompute_and_emit_busy_volumes` firing.
- Fulfillment service: `fulfill(source_volume_id, source_path, dest_path) -> Result<(), FulfillError>` — resolve the
  volume from the registry, `note_pending_write_for_cmdr(dest_path)`, `open_read_stream` →
  `write_from_stream(dest_path, …)` (exact Finder leaf), busy-volume register/release around it, partial cleanup on ANY
  `Err` (see the cleanup decision — the local writer does NOT self-clean on read errors), `FulfillError` carrying the
  `FriendlyError` text. The delegate `block_on`s the service from the queue thread (designed behavior — the queue is
  ours and serial; document why, plus the main-thread invariant).
- NSError mapping: domain `com.veszelovszki.cmdr.drag-out`, localized description from `FriendlyError` (raw text
  fallback). Finder displays it; we also log under a scoped target.
- App-quit abort: in-flight fulfillments end via stream `Drop` semantics + partial cleanup; wire whatever shutdown hook
  the service needs (mirror how other background work handles teardown).
- Drag-start wiring: virtual sessions construct providers per the M1 type plan; verify image + badge unchanged.
- Confirm no new capability entries are needed (no new window-invoked Tauri APIs expected).
- Tests: fulfillment service headless against the virtual MTP volume + tempdir dest — happy path **asserting the landed
  filename equals the Finder-chosen leaf exactly** (the regression guard for the rejected-engine mismatch), read-failure
  → cleanup + error, unwritable destination, disconnect-shaped error mapping; UTI mapping units; delegate smoke
  (construct provider, verify the fileName callback string).

### M3 — UX affordances + docs

Intent: nobody stares at a frozen Finder; the docs describe the feature instead of the limitation.

- Completion/failure toasts via the existing transfer-toast composer (counts are top-level items — consistent with the
  selection-split contract). Large-fulfillment signs-of-life affordance per the contract; decide the exact surface with
  the code in hand (toast with spinner vs. status indicator; if the chosen surface includes a Cancel button, wire it to
  the service's cancel seam — otherwise v1 stays no-user-cancel per Scope).
- Remove the "known limitation" block from `drag/CLAUDE.md`; document the promise architecture (current behavior only);
  update `native_drag.rs` module docs, the pane/transfer CLAUDE.mds where they mention drag-out, and
  `docs/tooling/virtual-mtp.md` (the virtual device is the test rig for this feature).
- CHANGELOG entry (user-facing: "Drag files from your phone straight to Finder or the Desktop").

### M4 — Verification

Intent: honest coverage given that the Finder leg cannot be automated.

- Full `pnpm check` + `--include-slow` (the in-app drag E2E suites must stay green).
- `cargo mutants` on the new pure logic (type plan, UTI mapping, error mapping, fulfillment service's pure parts).
- **Manual protocol** (executed with the `CMDR_VIRTUAL_MTP=1` rig + a real phone if available; record results in the
  spike note):
  1. Virtual MTP pane → drag one photo to Desktop → file lands, bytes correct, completion toast.
  2. Multi-select (3 files) → all land; names right.
  3. Name collision at dest → Finder uniquifies → our file lands under THAT name ("sunset 2.jpg").
  4. Folder drag (per M0 verdict) → recursive download or documented-unsupported.
  5. Eject attempt mid-fulfillment → blocked (busy volume).
  6. Failure injection (disconnect / read error mid-fulfillment) → Finder error alert, no partial file at dest, friendly
     log line.
  7. App quit mid-fulfillment → clean abort, no partial file.
  8. Drag back into Cmdr (self-drag) → recorded-identity path, unchanged.
  9. Drag from a LOCAL pane to Finder/terminal → byte-identical to today (URL drop + terminal text).
  10. Drag from non-local pane to a terminal → clean no-op, no textClipping.
  11. SMB-native pane variant of (1).

## Testing strategy summary

- **Unit (Rust)**: pasteboard type plan, UTI mapping, NSError/FriendlyError mapping, fulfillment service against virtual
  MTP + InMemory volumes (happy + exact-leaf, failure-cleanup, disconnect, unwritable dest).
- **Component/E2E**: existing in-app drag suites stay green (regression guard that promises didn't disturb the
  recorded-identity path or wry drop events). No new Playwright for the Finder leg — impossible to automate honestly;
  that's what the manual protocol is for.
- **Mutation**: the new pure modules.

## Open decisions (flagged, with proposed defaults)

1. **Folder promises in v1** — RESOLVED (M0): yes. Finder accepts a `public.folder` promise and hands us a directory
   destination URL; the fulfillment service populates it with the plan's hand-rolled recursive walk. Multi-select and
   folders both work.
2. **Large-fulfillment progress surface** — RESOLVED (M3): a single per-SESSION toast keyed by the drag sequence number,
   replaced in place from signs-of-life to completion. When the FIRST fulfillment begins (Finder asked), a neutral
   `default`-level persistent in-progress toast ("Copying N items from your device…") appears within ~1 s — the
   signs-of-life affordance, honest because it reflects real in-flight work rather than a guessed size threshold (MTP is
   serial-USB slow regardless of size). When the session drains, the same toast is replaced by a `success` completion
   toast ("Copied 2 files and 1 folder.", via the shared transfer-toast composer) or a `warn`/`error` failure toast
   naming the file(s). No Cancel button, so v1 stays no-user-cancel (Finder owns the gesture — Scope). Rejected the
   size-thresholded "big drags only" surface (a threshold is a guess; the in-progress toast is cheap and always honest)
   and a bespoke progress strip (no real progress signal short of per-byte plumbing we don't need in v1). Lives in
   `native_drag/session_summary.rs` (BE fold) + `lib/file-explorer/drag/drag-out-event-bridge.ts` (FE toasts).

## Execution notes

- Sequential milestones; M1 may merge alone (it's a user-visible bugfix in its own right). Nothing here parallelizes
  safely — the milestones share `native_drag.rs`.
- Full check suite before each commit; `--include-slow` before declaring M2 and M4 done.
- No pushes without explicit approval, per repo policy.
