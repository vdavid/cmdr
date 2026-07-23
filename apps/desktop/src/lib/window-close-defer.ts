/**
 * Deferred close for self-closing child webviews (Settings, file viewer).
 *
 * Two separate problems force a deferred `close()`, and they want different delays:
 *
 * 1. **Linux / webkit2gtk IPC stall.** Calling `close()` synchronously from a
 *    keydown handler destroys the webview on the same GTK main-loop tick that
 *    handled the key, which stalls IPC queued behind it from other webviews.
 *    Deferring to the next tick (`0`) is enough for this one.
 * 2. **macOS WebKit teardown crash.** Destroying a content-heavy webview while
 *    a layer-tree commit from its web content process is still in flight makes
 *    WebKit's UI-side `RemoteLayerTreeDrawingAreaProxy::commitLayerTree`
 *    dereference freed state and take the whole app down with a `SIGSEGV`. A
 *    next-tick defer does NOT cover this: the commit arrives on WebKit's own
 *    IPC run loop, not ours. A real delay lets in-flight commits drain first.
 *
 * So the delay is set by (2), the stricter of the two. Measured on macOS 26.5.2
 * (25F84) with a repro harness driving real Escape keypresses to close the
 * settings window from a live-updating section: at `0` ms the app crashed after
 * 36 close cycles; at `100` ms it survived 80 consecutive cycles clean.
 * See `docs/notes/child-window-close-webkit-crash.md`.
 *
 * This mitigates a race rather than removing it, so treat the delay as a floor:
 * don't lower it, and don't switch call sites to `requestAnimationFrame` (macOS
 * WKWebView throttles rAF in unfocused windows, which starves E2E closes; see
 * `docs/testing.md` § "rAF in unfocused windows").
 */
export const WINDOW_CLOSE_DEFER_MS = 100

/**
 * Runs `close` after `WINDOW_CLOSE_DEFER_MS`, so a webview never destroys
 * itself from inside the event handler that asked it to.
 *
 * Use this from every self-closing webview instead of hand-rolling a
 * `setTimeout`, so the two windows can't drift apart on the delay.
 *
 * @param close what to run once it's safe to tear the window down
 * @param delayMs override only for tests; production callers take the default
 * @returns the timer handle, so a caller can cancel a queued close
 */
export function deferWindowClose(
  close: () => void,
  delayMs: number = WINDOW_CLOSE_DEFER_MS,
): ReturnType<typeof setTimeout> {
  return setTimeout(close, delayMs)
}
