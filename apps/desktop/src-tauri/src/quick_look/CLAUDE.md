# Quick Look

Native macOS Quick Look (`QLPreviewPanel`) integration. Shift+Space opens a real AppKit preview
panel over Cmdr; arrow keys keep navigating the file list while the panel tracks the cursor.

The full design rationale lives in [`docs/specs/quick-look-plan.md`](../../../../../docs/specs/quick-look-plan.md);
this file is the per-module quick reference.

## Files

| File            | Purpose                                                                 |
| --------------- | ----------------------------------------------------------------------- |
| `mod.rs`        | Module root. `QuickLookState = Mutex<QuickLookController>` (`Mutex<()>` on non-macOS), `init_state()`, and the `QuickLookKeyEvent` serde payload. |
| `controller.rs` | macOS-only. `QuickLookController` (bookkeeping), `QuickLookDelegate` (data source + delegate + close observer), `define_class!` glue, key-event translation, and state-machine unit tests. |

## Surface

Three Tauri commands live in `commands/ui.rs` (not here, to keep this module thin) and gate the
controller:

- `quick_look_open(path, volume_id)` — open or re-target.
- `quick_look_set_path(path, volume_id)` — re-target while open (no-op if closed).
- `quick_look_close()` — `orderOut:` the panel.

All three hop to the AppKit main thread via `app.run_on_main_thread()` + a one-shot `mpsc`
channel, wrapped in `blocking_with_timeout` (2 s) so a wedged AppKit pump can't freeze the IPC pool.

Two Tauri events flow out:

- `quick-look-key` — keyboard events the panel didn't want. Payload mirrors a DOM `KeyboardEvent`
  (`key`, `code`, `shiftKey`, `metaKey`, `altKey`, `ctrlKey`). The frontend re-routes them through
  the focused pane's existing navigation primitives.
- `quick-look-closed` — fires whenever the panel actually leaves the screen (our `orderOut:`, the
  ✕ button, or Esc). The frontend uses it to flip `isOpen = false`.

## Key decisions

**Decision**: Singleton controller behind `Mutex<QuickLookController>`, no "new each time."
**Why**: `+[QLPreviewPanel sharedPreviewPanel]` is process-wide. There is no per-instance panel
in AppKit; "open it" really means "install ourselves as data source + delegate, then
`makeKeyAndOrderFront:`." The struct only holds bookkeeping (`current_url`, `is_open`).

**Decision**: Set `dataSource` / `delegate` directly, skip the responder-chain
`QLPreviewPanelController` discovery.
**Why**: Tauri's window-delegate ownership makes inserting ourselves into the responder chain
awkward. Direct assignment is documented and is what Apple's sample code recommends when you own
the panel's lifecycle.

**Decision**: Forward keys via a Tauri event, not direct AppKit forwarding.
**Why**: WKWebView's keydown handling depends on the window being key, and ours isn't (the panel
is, by design). `[contentView keyDown:]` would silently lose events. Routing via Tauri event +
re-dispatch through `explorerRef.routePanelKey(payload)` gives a clean IPC boundary, works
regardless of which pane is focused, and avoids poking at Tauri internals.

**Decision**: MTP and other non-fs-accessible volumes no-op (debug log) on Quick Look.
**Why**: `QLPreviewPanel` wants an `NSURL` to a local file. MTP paths look like filesystem paths
(MTP virtual paths) but have no `NSURL` mapping. We gate on
`Volume::supports_local_fs_access()`, not `Path::exists()`, because `exists()` returns false
for live MTP files too — the volume kind is the correct signal. Finder doesn't preview MTP either.

**Decision**: `MainThreadOnly` `define_class!` for `QuickLookDelegate`.
**Why**: This is the first `define_class!` with `#[thread_kind = MainThreadOnly]` in the
codebase. Marking the class as main-thread-only at the type level lets objc2 enforce the
constraint at compile time rather than via runtime `MainThreadMarker::new().expect(...)` checks
in every method. The marker still appears in the `_on_main` Rust methods because the controller
itself isn't main-thread-bound (it's a `Mutex`-guarded shared state), but the delegate
construction (`Self::alloc(mtm)`) takes one, which is why `ensure_delegate` threads the marker
through.

**Decision**: Never `removeObserver:` the close-notification observer.
**Why**: The panel is process-wide and the delegate is retained by it through `setDelegate:`. The
observer must outlive any specific open/close cycle. When `AppHandle` drops at process shutdown
the delegate (and the observer) goes with it. This is the documented pattern for singleton
observers in AppKit.

## Gotchas

**Gotcha**: `method_id` macro requires a single tail expression.
**Why**: `#[unsafe(method_id(...))]` wraps the return value in objc2's conversion machinery. The
function body can't `return early` or use `?` — both produce intermediate `Option`s that the
macro can't coerce. We compute the URL once and let the macro do the wrapping. See
`previewItemAtIndex` in `controller.rs`.

**Gotcha**: Setting `is_open = false` inside `close_on_main` races the close observer.
**Why**: `panel.orderOut(nil)` posts `NSWindowWillCloseNotification` asynchronously. Apple's
docs don't mention this (they only attribute the notification to `[NSWindow close]`), but
empirically `QLPreviewPanel` posts it on `orderOut:` too — verified via the MCP smoke procedure
in `apps/desktop/test/manual/quick-look-mcp.md`: the observer fires ~200 ms after the close IPC
returns. If we flip `is_open` synchronously here AND the observer flips it again on the
notification, a quick reopen could see `is_open == false` (good) but then the observer's late
flip resets to false mid-reopen, breaking the reopen. The observer path is the single source of
truth; don't introduce a parallel flip.

**Gotcha**: `Path::exists()` is not a valid Quick Look gate.
**Why**: MTP virtual paths return false from `exists()` even when the file is real on the device.
Use `Volume::supports_local_fs_access()` instead.

## Coexistence with NSOpenPanel

`QLPreviewPanel` and `NSOpenPanel` are both AppKit panels that take main-thread key focus. Cmdr
opens `NSOpenPanel` for "Open with… Other" (`commands/open_with.rs`) and for save dialogs via
Tauri's file plugin.

Empirically (M3 read-only verification, no native UI driving), AppKit serializes:

- While `NSOpenPanel` is the modal key window, our `makeKeyAndOrderFront:` call on
  `QLPreviewPanel` runs but the panel doesn't visibly take key — it queues behind the open panel.
  When the user dismisses `NSOpenPanel`, the Quick Look panel becomes key and visible.
- Opening `NSOpenPanel` while Quick Look is up demotes Quick Look to a regular floating window.

No crash either way; just the visual ordering you'd expect from AppKit modality. We don't
pre-empt with explicit detection — the cost of a heuristic that misclassifies a save dialog
mid-flight outweighs the cost of the user closing the open panel first. Document the behavior,
don't gate it.

## Testing gap

The state-machine unit tests in `controller.rs` cover the bookkeeping half of every transition
(`apply_open`, `apply_set_path`, `mark_closed`, accessors): open → set_path → close → reopen,
double-open is idempotent, set_path before open is a no-op, mark_closed clears both fields. They
do NOT cover the AppKit half (`makeKeyAndOrderFront:`, `reloadData`, `orderOut:`, the
`NSNotificationCenter` close observer) — those need a real main-thread runloop and a live
`QLPreviewPanel`, neither of which is reachable from `cargo nextest`.

We didn't add a trait abstraction over the AppKit calls because the gap is small (three
method calls, all single-line) and the cost of mocking would exceed the value: the AppKit calls
are documented Apple APIs that don't change between runs, and the only failure mode the trait
could catch (wrong selector, wrong argument type) is also caught by `cargo check`. The
bookkeeping is what every other layer reads (the `volume_supports_local_fs` IPC gate, the close
event emitter, the frontend `isOpen` flag), so pinning the transitions there is enough.

Manual / MCP-driven verification covers the AppKit side: see
`apps/desktop/test/manual/quick-look-mcp.md` for the smoke procedure.

## Extending to multi-selection

Finder shows a "carousel" of preview items when multiple files are selected. v1 deliberately
shows only the cursor item, ignoring the selection set. To add carousel mode:

1. Change `DelegateIvars::url` from `Mutex<Option<Retained<NSURL>>>` to a `Mutex<Vec<Retained<NSURL>>>`.
2. Return the vector's length from `numberOfPreviewItemsInPreviewPanel:` and look up by index in
   `previewItemAtIndex`.
3. Add a `quick_look_set_paths(paths[], volume_id)` IPC parallel to `quick_look_set_path` —
   fired by the frontend's cursor-follow `$effect` whenever the selection size > 1.
4. Wire the frontend listener so cursor changes within a multi-selection re-target the panel's
   highlighted item rather than firing a new `set_paths`.

The state-machine refactor in step 1 is the load-bearing one; the IPC and frontend changes
follow from it.

## Dependencies

External: `objc2`, `objc2-app-kit`, `objc2-foundation`, `objc2-quick-look-ui` (all macOS-only).
Internal: `tauri::AppHandle`, `crate::file_system::get_volume_manager` (volume gate, via the IPC
layer in `commands/ui.rs`).
