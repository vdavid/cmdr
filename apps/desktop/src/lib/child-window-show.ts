/**
 * How a child window (Settings, the file viewer, Debug, Keyboard shortcuts, the operation queue)
 * appears: hidden at creation, then shown by its own page once it looks right, sliding in on macOS
 * (unless Reduce motion is on). The close's counterpart is `#lib/child-window-close`; both halves
 * live in Rust, `src-tauri/src/child_window_lifecycle/`.
 *
 * - The opener creates the window `visible: false` (outside E2E) and calls `scheduleShowFallback`.
 * - The page calls `showSelfWindow` once its appearance is applied and its first content has
 *   rendered (settings, accent, glass, display accessibility, text size, then `await tick()`),
 *   never waiting on content I/O such as a file pull. Showing earlier slides in a window whose
 *   colors or content then change under the user's eyes.
 *
 * The backend ignores a window that's already up, so the fallback and an E2E window (created
 * visible, ordered to the back) are both no-ops.
 */
import { getCurrentWindow } from '@tauri-apps/api/window'
import { showChildWindow } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('windowShow')

/** How long an opener waits for the page to show its own window before showing it anyway. */
const SHOW_FALLBACK_MS = 2000

/** Called by a child window's page once it's ready: shows its own window. */
export function showSelfWindow(): void {
  const { label } = getCurrentWindow()
  void showChildWindow(label).catch((error: unknown) => {
    log.warn('Showing window {label} returned an error: {error}', { label, error })
  })
}

/**
 * Called by the opener right after creating a hidden window: shows it anyway if its page never
 * gets to (a load or init problem), so a problem is visible instead of an invisible window
 * holding a web content process. The timer runs in the opener's visible webview, so WebKit
 * doesn't throttle it.
 */
export function scheduleShowFallback(label: string): void {
  setTimeout(() => {
    void showChildWindow(label).catch((error: unknown) => {
      log.warn('Fallback show of window {label} returned an error: {error}', { label, error })
    })
  }, SHOW_FALLBACK_MS)
}
