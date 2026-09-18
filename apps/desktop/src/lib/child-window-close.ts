/**
 * The one way a child webview (Settings, file viewer, Debug) closes itself.
 *
 * A webview must never destroy itself from inside the handler that asked it to. Two separate
 * problems say so:
 *
 * 1. **macOS WebKit teardown crash.** Destroying a content-heavy webview while a layer-tree
 *    commit from its web content process is still in flight makes WebKit's UI-side
 *    `RemoteLayerTreeDrawingAreaProxy::commitLayerTree` dereference freed state and take the
 *    whole app down with a `SIGSEGV`.
 * 2. **Linux / webkit2gtk IPC stall.** Destroying the webview on the same GTK main-loop tick
 *    that handled the key stalls IPC queued behind it from other webviews.
 *
 * ❗ The hide-and-wait lives in RUST (`commands/child_window_state.rs`), not here, and that is
 * the point of this module being a one-line wrapper. WebKit throttles timers in a hidden page to
 * roughly 1 Hz, so a `setTimeout` inside a webview that just hid itself can slip by a second or
 * more and leave an invisible window alive, still holding a web content process. A Rust timer
 * can't be throttled by the page it's about to close.
 *
 * ❌ Don't reintroduce a JS-side timer here, and ❌ don't call `getCurrentWindow().close()`
 * directly from a keydown handler. See `docs/notes/child-window-close-webkit-crash.md`.
 */
import { getCurrentWindow } from '@tauri-apps/api/window'
import { closeChildWindow } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('windowClose')

/**
 * Ask the backend to hide this window and then destroy it.
 *
 * Returns once the request is in; the window goes away a moment later. Safe to call twice (a
 * second call finds no window and does nothing), so a race between Escape and the red button
 * costs nothing.
 */
export async function closeSelfWindow(): Promise<void> {
  const { label } = getCurrentWindow()
  try {
    await closeChildWindow(label)
  } catch (e) {
    // The window stays up, which is visible to the user and better than a half-torn-down one.
    log.error('Closing window {label} returned an error: {error}', { label, error: String(e) })
  }
}
