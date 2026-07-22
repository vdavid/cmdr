# Drag-out file promises — M0 spike findings

Records the two load-bearing unknowns the drag-out file-promises spike resolved before any real code landed: the chosen
pasteboard layout and the folder-promise verdict. The shipped architecture and its rationale live in
`apps/desktop/src-tauri/src/native_drag/CLAUDE.md` (+ its `DETAILS.md`); design history is in git. Linked from
`apps/desktop/src/lib/file-explorer/drag/CLAUDE.md`.

## Verdicts

- **Q1 — Pasteboard layout for virtual drags: ANSWERED. Ship layout (a) (promise-only).** A real drag of a virtual-MTP
  file to the Desktop downloaded it under Finder's chosen name; an in-app self-drag still fires wry's drop event with
  empty paths and no panic. (a) carries `NSFilePromiseProvider`s only — no file-url, no text, no `NSFilenamesPboardType`
  — across every item.
- **Q2 — Folder promises (`public.folder`): ANSWERED. Folders are IN for v1.** Finder accepts a `public.folder` promise
  and hands us a directory destination URL via `writePromiseToURL:`, which v1 populates with the plan's hand-rolled
  recursive walk (list → mkdir → per-file `write_from_stream`).

## What merges from M0

Only this note and two Cargo feature additions (already applied to `apps/desktop/src-tauri/Cargo.toml`). The throwaway
spike rig is deleted.

- `objc2-app-kit`: added `"NSFilePromiseProvider"` and `"block2"` features.
- `objc2-foundation`: added `"NSOperation"` (the delegate's `operationQueueForFilePromiseProvider:` returns
  `NSOperationQueue`, which M2 constructs one of per drag session; `NSFilePromiseProvider` also lists `NSOperation`
  among its feature deps).

`block2` (the crate) was already a direct dependency for other AppKit completion blocks; only the objc2-app-kit `block2`
**feature** needed enabling so `filePromiseProvider:writePromiseToURL:completionHandler:` is generated. These were
proven necessary in the spike: a delegate implementing `NSFilePromiseProviderDelegate` compiles only with all three.

The delegate selectors (verified working against a real Finder drop):

- `filePromiseProvider:fileNameForType:` takes a `MainThreadMarker` (main-thread call), returns `Retained<NSString>`,
  and needs `#[unsafe(method_id(...))]` (a retained return), per the `quick_look/controller.rs` precedent.
- `filePromiseProvider:writePromiseToURL:completionHandler:` is gated behind the objc2-app-kit `block2` feature; the
  completion handler is `&block2::DynBlock<dyn Fn(*mut NSError)>`. Calling it with `std::ptr::null_mut()` signals
  success; with `Retained::into_raw(nserror)` signals failure.
- `operationQueueForFilePromiseProvider:` returns `Retained<NSOperationQueue>` (optional; also needs `method_id`).
- UTI per item from a small pure mapping (`public.jpeg`/`public.png`/… → `public.data` fallback; `public.folder` for
  directories).

## ⚠️ Delegate-lifetime gotcha (M2 WILL hit this — load-bearing)

**`NSFilePromiseProvider.delegate` is a WEAK reference** (the objc2-app-kit binding documents it as
`/// This is a [weak property]`). The provider does NOT retain its delegate. So if the `Retained<...>` delegate is a
local that drops when drag-start returns, the delegate deallocates, the provider's weak `delegate` zeroes, and **Finder
queries a nil delegate and silently does nothing** — no `fileNameForType:`, no `writePromiseToURL:`, no error, no file.
The drag itself looks fine (the drag-start fires); the promise just never gets queried.

The spike hit exactly this: a real drag-to-Desktop fired the drag-start but produced ZERO delegate callbacks until the
delegate was kept alive. A self-test confirmed the mechanism directly: `provider.delegate()` reads `false` after the
local `Retained` drops, `true` once it's kept alive.

The same trap applies to the shared `NSOperationQueue` and the drag source (AppKit retains the source for the session,
so that one's fine — but the queue rides in the delegate's ivar, so keeping the delegate alive keeps the queue alive
too).

**For M2: store each session's delegates + providers + queue in session-scoped storage** (a `static OnceLock<Mutex<…>>`
keyed by drag session, like `quick_look`'s controller, or the drag source object's own retained set) and clear it on
drag-session end (`draggingSession:endedAtPoint:operation:`) AFTER any in-flight fulfillment completes. The objects must
outlive BOTH the gesture and the (possibly later, queue-thread) fulfillment.

## Question 1 — Pasteboard layout for virtual drags

| Layout                        | In-app drop event (wry)                                        | Finder behavior                                                                       | Verdict                           |
| ----------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------- |
| **a** PromiseOnly             | Fires. `enter`/`over`/`drop`, `paths: []`, no panic (observed) | **Downloads the file under Finder's chosen name** (observed — see log below)          | **SHIP — confirmed both legs**    |
| b PromiseAndRealFilenames     | Would fire WITH the volume-relative path in `paths`            | Would copy the dead path / make junk                                                  | Never ship — untested by decision |
| c PromiseAndSentinelFilenames | Would fire with `paths: []`                                    | Would behave like (a)                                                                 | Fallback, unneeded — not tested   |
| d CurrentLayout               | Fires today (status quo)                                       | **Produces the `…textClipping` junk file** — the production incident IS this evidence | Negative control — not re-tested  |

Notes on the controls:

- **(d)** is the layout shipping today; the live incident (`sunset.jpg` → `photos:sunset.jpg.textClipping` containing
  `/photos/sunset.jpg`) is its recorded failure. No re-test needed — that bug is exactly why this feature exists.
- **(b)** can only lose (a real `NSFilenamesPboardType` carrying a volume-relative path is the garbage Finder would try
  to copy) and would never ship, so it was skipped by decision.
- **(c)** existed only as a fallback if (a)'s drop event failed to fire. It didn't fail, so (c) is moot and untested.

**(a) success — Finder leg, observed in the instance log:**

```
SPIKE DRAG paths=["/DCIM/photo-001.jpg"]
fileNameForType -> photo-001.jpg
writePromiseToURL dest=Some("/Users/veszelovszki/Desktop/photo-001.jpg")
fulfillment OK
```

The real file landed on the Desktop (placeholder content in the spike; real streamed bytes in M2). No textClipping.

**(a) in-app self-drag — observed drop-log tail (the wry-panic question, answered no-panic):**

```
{type:"enter", paths:[], pos:{x:273,y:134}}
... (over events) ...
{type:"drop",  paths:[], pos:{x:811,y:170}}
```

Mechanism: promise-only items carry no file-url, so AppKit doesn't auto-derive `NSFilenamesPboardType`, so wry's
`collect_paths` returns an empty vec instead of `unwrap()`-panicking, and `DragDropEvent::Drop` still reaches the FE —
the recorded-identity self-drag path keeps working. This was the only real M1 risk; it's cleared.

## Question 2 — Folder promises (`public.folder`)

**Finder accepts `public.folder` and supplies a directory destination URL.** Observed in the instance log:

```
SPIKE DRAG paths=["/DCIM"]
fileNameForType -> DCIM
writePromiseToURL dest=Some("/Users/veszelovszki/Desktop/DCIM")
fulfillment OK
```

(The spike's placeholder writer writes a _file_ at that URL, so the test produced a file named `DCIM` — expected. The
point is that Finder fired the callback with a directory-intended destination URL, which is all v1 needs.) So folders
are **in for v1**: M2 populates the supplied directory URL with a hand-rolled recursive walk (list → mkdir → per-file
`write_from_stream` to the exact child paths), since the cross-volume copy engine derives names from sources and can't
be pointed at a Finder-renamed root.

## v1 decisions (the plan's open decisions, now decided)

1. **Pasteboard layout**: ship **(a) promise-only** for virtual sessions. Local sessions keep today's layout
   byte-for-byte.
2. **Folders in v1**: **yes**. Finder hands us a directory URL; the recursion is the plan's small hand-rolled walk.

## Recommendation for M1 / M2

- **M1 (session-locality pasteboard composition)** is unblocked and safe. The (a) result proves the virtual session can
  strip file-url/text/filenames across every item without breaking the in-app drop event. Build the pure type-plan with
  (a)'s "promise-only, no legacy types" as the virtual-session output.
- **M2 (provider + delegate + fulfillment service)** has a proven native skeleton:
  `NSFilePromiseProvider::initWithFileType_delegate`, a `MainThreadOnly` `define_class!` delegate with `method_id`
  returns for `fileNameForType:`/`operationQueueForFilePromiseProvider:`, the `block2` completion in
  `writePromiseToURL:completionHandler:` (null = success, `Retained::into_raw(NSError)` = failure), one shared serial
  `NSOperationQueue` per session, and the small UTI mapping. **Mind the delegate-lifetime gotcha above** — store
  delegates/providers/queue in session-scoped storage, do not leak.
