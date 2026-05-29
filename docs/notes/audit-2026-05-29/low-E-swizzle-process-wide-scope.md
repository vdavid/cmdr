# Drag-image swizzle mutates an objc class shared with the rest of the process

**Severity:** low
**Lens:** E — macOS pitfalls
**Confidence:** medium

## Location

- `apps/desktop/src-tauri/src/drag_image_detection.rs:119–190` (`install_swizzles`)
- `apps/desktop/src-tauri/src/drag_image_swap.rs` (swap path documented as relying on the same swizzle)

## What

`install_swizzles` walks up from the *first* webview window's native pointer, reads its objc class, and calls `method.set_implementation(...)` on three selectors (`draggingEntered:`, `draggingUpdated:`, `draggingExited:`). Objective-C method swizzling is **class-scoped, process-wide**: it patches the dispatch table of `WryWebView` (or whatever wry's class is named today). Every instance of that class in the process is affected, including:

- All Cmdr webview windows (intended).
- Any other webview Tauri creates during the session, including Settings, viewer, and Quick Look pop-overs once they exist (probably fine, since the swizzle's degradation path is documented).
- Hypothetically, any in-process plugin or extension that loads another wry/WKWebView via the same class identifier (unlikely today, but the assumption is implicit).

The CLAUDE.md doesn't list this constraint explicitly, and the install is guarded only by the "first webview window exists" check. There's no idempotency gate: if `install` is somehow called twice (e.g. webview re-creation on settings-window open in some future refactor), the second call would re-set the implementation to the same swizzled fn AND re-cache the *current implementation* (which is already the swizzle) as `ORIGINAL_*_IMP`. That would create an infinite loop next time a drag fires (swizzle calls "original" which is itself).

The `ORIGINAL_*_IMP.set()` returns `Err` on second call (it's a `OnceLock`), so the original IS preserved — but `method.set_implementation(...)` runs unconditionally, which is harmless on second call because it sets the same pointer.

## Why it matters

- Process-wide class mutation is an external-impact action. Hard to predict downstream effects without tooling, easy to miss when reviewing.
- The "if Apple deprecates" graceful-degradation path is well documented (lines 17–24), but the "if wry adds another `WryWebView` subclass" path isn't.
- Low severity because:
  - Today, only one webview class is in use.
  - `OnceLock` prevents the infinite-loop case in practice.
  - The swizzle gracefully no-ops if class lookups fail.

## Evidence

`drag_image_detection.rs:138–187`: three `method.set_implementation` calls on the discovered class. No re-entry guard at the function level — relies on `install` being called exactly once from `RunEvent::Ready`. `lib.rs` is structured so this is true today.

## Suggested fix

Make `install_swizzles` explicitly idempotent at the function level:

```rust
static SWIZZLES_INSTALLED: OnceLock<()> = OnceLock::new();
if SWIZZLES_INSTALLED.set(()).is_err() {
    return;  // already installed; ignore re-entry
}
```

The `ORIGINAL_*_IMP` `OnceLock` already provides this for the originals, but explicit early-return at the function gate makes the intent obvious and protects against a future where another webview class joins the process.

## Notes

Confidence is medium because I haven't checked whether `with_webview` could be called against a *settings* window in the future, which would discover a different class on some wry version. Today's `tauri.conf.json` has a single main window; Settings is a separate window opened later. Worth checking which class Settings' webview reports if/when this gets revisited.
