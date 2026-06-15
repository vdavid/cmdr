# Quick Look details

Architecture and decisions for the native macOS Quick Look integration. `CLAUDE.md` holds the must-knows.

## Decisions

- **Singleton controller behind `Mutex<QuickLookController>`, no "new each time."**
  `+[QLPreviewPanel sharedPreviewPanel]` is process-wide; there is no per-instance panel in AppKit. "Open it" really
  means "install ourselves as data source + delegate, then `makeKeyAndOrderFront:`." The struct only holds bookkeeping
  (`current_url`, `is_open`).
- **Set `dataSource` / `delegate` directly, skip the responder-chain `QLPreviewPanelController` discovery.**
  Tauri's window-delegate ownership makes inserting ourselves into the responder chain awkward. Direct assignment is
  documented and is what Apple's sample code recommends when you own the panel's lifecycle.
- **Forward keys via a Tauri event, not direct AppKit forwarding.** WKWebView's keydown handling depends on the window
  being key, and ours isn't (the panel is, by design), so `[contentView keyDown:]` would silently lose events. Routing
  via Tauri event + re-dispatch through `explorerRef.routePanelKey(payload)` gives a clean IPC boundary, works
  regardless of which pane is focused, and avoids poking at Tauri internals.
- **`MainThreadOnly` `define_class!` for `QuickLookDelegate`.** Marking the class main-thread-only at the type level
  lets objc2 enforce the constraint at compile time rather than via runtime `MainThreadMarker::new().expect(...)` checks
  in every method. The marker still appears in the `_on_main` Rust methods because the controller itself isn't
  main-thread-bound (it's `Mutex`-guarded shared state), but the delegate construction (`Self::alloc(mtm)`) takes one,
  which is why `ensure_delegate` threads the marker through.
- **Never `removeObserver:` the close-notification observer.** The panel is process-wide and the delegate is retained by
  it through `setDelegate:`. The observer must outlive any specific open/close cycle. When `AppHandle` drops at process
  shutdown, the delegate (and observer) go with it. This is the documented pattern for singleton observers in AppKit.

## Coexistence with NSOpenPanel

`QLPreviewPanel` and `NSOpenPanel` are both AppKit panels that take main-thread key focus. Cmdr opens `NSOpenPanel` for
"Open with… Other" (`commands/open_with.rs`) and for save dialogs via Tauri's file plugin. Empirically, with no native
UI driving in test, AppKit serializes:

- While `NSOpenPanel` is the modal key window, our `makeKeyAndOrderFront:` on `QLPreviewPanel` runs but the panel
  doesn't visibly take key; it queues behind the open panel and becomes key/visible when the user dismisses the open
  panel.
- Opening `NSOpenPanel` while Quick Look is up demotes Quick Look to a regular floating window.

No crash either way; just the visual ordering you'd expect from AppKit modality. We don't pre-empt with explicit
detection: the cost of a heuristic that misclassifies a save dialog mid-flight outweighs the cost of the user closing
the open panel first. Document the behavior, don't gate it.

## Testing gap

The state-machine unit tests in `controller.rs` cover the bookkeeping half of every transition (`apply_open`,
`apply_set_path`, `mark_closed`, accessors): open → set_path → close → reopen, double-open is idempotent, set_path
before open is a no-op, `mark_closed` clears both fields. They do NOT cover the AppKit half
(`makeKeyAndOrderFront:`, `reloadData`, `orderOut:`, the `NSNotificationCenter` close observer): those need a real
main-thread runloop and a live `QLPreviewPanel`, neither reachable from `cargo nextest`.

We didn't add a trait abstraction over the AppKit calls because the gap is small (three single-line method calls) and
the cost of mocking would exceed the value: the AppKit calls are documented Apple APIs that don't change between runs,
and the only failure mode the trait could catch (wrong selector, wrong argument type) is also caught by `cargo check`.
The bookkeeping is what every other layer reads (the `volume_supports_local_fs` IPC gate, the close event emitter, the
frontend `isOpen` flag), so pinning the transitions there is enough. Manual / MCP-driven verification covers the AppKit
side: see `apps/desktop/test/manual/quick-look-mcp.md` for the smoke procedure.

## Extending to multi-selection

Finder shows a "carousel" of preview items when multiple files are selected. v1 deliberately shows only the cursor
item. To add carousel mode:

1. Change `DelegateIvars::url` from `Mutex<Option<Retained<NSURL>>>` to `Mutex<Vec<Retained<NSURL>>>`.
2. Return the vector's length from `numberOfPreviewItemsInPreviewPanel:` and look up by index in `previewItemAtIndex`.
3. Add a `quick_look_set_paths(paths[], volume_id)` IPC parallel to `quick_look_set_path`, fired by the frontend's
   cursor-follow `$effect` whenever the selection size > 1.
4. Wire the frontend listener so cursor changes within a multi-selection re-target the panel's highlighted item rather
   than firing a new `set_paths`.

The state-machine refactor in step 1 is the load-bearing one; the IPC and frontend changes follow from it.

## Dependencies

External: `objc2`, `objc2-app-kit`, `objc2-foundation`, `objc2-quick-look-ui` (all macOS-only). Internal:
`tauri::AppHandle`, `crate::file_system::get_volume_manager` (volume gate, via the IPC layer in `commands/ui.rs`).
