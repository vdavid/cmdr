# Child window lifecycle: details

Must-knows: `CLAUDE.md`. This file holds the flows, the races, and the why.

## Open

1. The opener (`lib/settings/settings-window.ts`, `lib/file-viewer/open-viewer.ts`, `lib/debug/debug-window.ts`,
   `lib/shortcuts/shortcuts-window.ts`, `lib/file-operations/queue/queue-window.ts`) creates the `WebviewWindow` with
   `visible: false` (E2E keeps `visible: true`, since `orderChildWindowToBackInE2e` owns its ordering) and calls
   `scheduleShowFallback(label)`.
2. The page calls `showSelfWindow()` once it looks right: after its appearance init (settings, accent, glass, display
   accessibility, text size), its `initialized` render, and an `await tick()`, ❌ never waiting on content I/O (a viewer
   file pull can run for minutes). Showing earlier slides in a window whose colors or content then change under the
   user.
3. `show` hops to the main thread. A window that's visible, minimized, or closing is left alone, so the 2 s fallback
   can't take the front back or resurrect a window mid-close.
4. `appkit::show` sets `animationBehavior = None` (ours would otherwise stack with AppKit's), then either orders it
   front plainly (Reduce motion) or sets alpha 0 and the frame 24 pt higher, orders front, and animates alpha 1 and the
   real frame through the `animator` proxy. The ledger records `SlidIn { settles_at }`.

Decision: the slide is ours, not an `NSWindowAnimationBehavior`. David compared `DocumentWindow`, `UtilityWindow`, and
`AlertPanel` side by side against this slide (2026-11-11) and kept the slide.

Decision: Settings and the Debug window get their vibrancy via `setEffects` on `tauri://created`. With the window
hidden at creation, that lands before the first show, so the slide plays over the finished material.

## Close

1. `begin_close` hands out a ticket, or `None` when a close is already under way (Escape plus the red button is
   ordinary).
2. On the main thread: if the window slid in, is on screen, and Reduce motion is off, `appkit::start_slide_out`
   animates alpha 0 and the frame 24 pt up and the ledger keeps the frame it left from (`rest`). Otherwise the window
   hides right away.
3. `finish_close` (a tokio task, so WebKit's hidden-page timer throttling can't stretch it): after a slide, wait
   `SLIDE_DURATION` + `SETTLE_MARGIN` (40 ms, since the display link can land the last frame a little late), then hide.
   Then wait `HIDE_TO_DESTROY`, then `take_close` and `destroy()`.
4. Every step after the first checks its ticket is still current. Only `cancel_close` changes it.

Linux: no slide and no hide (an untested change to how webkit2gtk tears a window down); the 100 ms defer alone, which
there prevents a GTK main-loop IPC stall.

Decision: route the red button and ⌘W through `CloseRequested` + `prevent_close()` rather than a JS
`onCloseRequested` listener. The handler runs in Rust on the event that every close path emits (tao's
`windowShouldClose:` for the button and `performClose:`, Tauri's `Window::close()` for programmatic closes), so no page
has to register anything, and a page that never finished loading closes safely too.

## Reopen while closing

`WebviewWindow.getByLabel` still finds a closing singleton, so the opener calls `raise_child_window`, which calls
`cancel_close` first:

- `SlideBack { rest }`: animates alpha 1 and `rest`, taking over from the running slide-out (a newer animation of the
  same properties replaces it). The ledger goes back to `SlidIn`, so the next close slides again.
- `Plain`: the window was hidden without a slide; `raise_ns_window`'s `makeKeyAndOrderFront:` shows it.

Known gap: a reopen landing between `take_close` and the `destroy()` (microseconds) raises a window that's then
destroyed, so that one press does nothing. A second press opens a fresh window.

Known gap: closing a window during its own slide-in reads the in-flight frame as `rest`, so a reopen in that same
window of time slides back to a spot up to 24 pt off.

## Saved rects

`trackOwnRect` (`lib/window-positioning.ts`) saves on every move and resize through `set_child_window_rect`, which asks
`accepts_rect_write` first. `SlidIn` before `settles_at` and every `Closing` refuse. Regression anchor: the slide-out
moved the window 24 pt up and the page saved it, so each reopen opened 24 pt higher.
