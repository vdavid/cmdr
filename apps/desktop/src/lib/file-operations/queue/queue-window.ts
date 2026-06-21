/**
 * Transfer-queue window management.
 *
 * A real macOS vibrancy window (a sibling of Settings / Keyboard shortcuts, NOT
 * a modal): it lists every running and queued copy, move, delete, and trash
 * operation with per-row pause/resume/cancel, multi-select + "Cancel selected",
 * and global pause/resume. It's a hard window so the user can keep working in
 * the main window while transfers run in the background and still manage them.
 *
 * Cloned from `lib/settings/settings-window.ts`: open-or-focus a singleton
 * `queue` window, position via the shared `$lib/window-positioning` helpers,
 * apply `NSVisualEffectView` vibrancy (with the reduce-transparency opaque
 * fallback), and use an overlay title bar with the traffic lights inset into
 * our own header. The opener runs on the MAIN window, which already holds the
 * window-creation / monitor perms (`adding-a-window.md`); the queue window's own
 * capability file (`capabilities/queue.json`) grants only what its page calls.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import { emitTo } from '@tauri-apps/api/event'
import { Effect, EffectState } from '@tauri-apps/api/window'
import { commands } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'
import { getEffectiveScale } from '$lib/text-size.svelte'
import { decorateChildWindowTitle, getAppMode, orderChildWindowToBackInE2e } from '$lib/app-mode'
import { readMainRect, readMonitors, readSavedRect, resolveChildPosition } from '$lib/window-positioning'

const log = getAppLogger('queue')

/** Base (scale = 1) window size. Roomy enough for several operation rows with
 *  inline progress bars; resizable, with the list scrolling inside. */
const BASE_WIDTH = 560
const BASE_HEIGHT = 480
const MIN_WIDTH = 420
const MIN_HEIGHT = 280

/**
 * Opens the transfer-queue window, or focuses it if already open (singleton,
 * like Settings). Cross-window `setFocus()` doesn't reliably raise a window on
 * macOS, so an already-open window self-focuses via the `focus-self` event.
 *
 * Every Tauri call is awaited in try/catch with a `log.warn`: window perms fail
 * SILENTLY, so a missing grant must surface as a log line, not a dead window.
 */
export async function openQueueWindow(): Promise<void> {
  // E2E suites re-open windows many times; stealing OS focus each time makes the
  // host machine unusable while tests run. The plugin drives the webview over a
  // socket, so it doesn't need OS focus. Mirrors Settings / Shortcuts.
  const isE2e = getAppMode() === 'e2e'

  try {
    const existing = await WebviewWindow.getByLabel('queue')
    if (existing) {
      if (!isE2e) {
        await emitTo('queue', 'focus-self')
      }
      return
    }
  } catch (error) {
    log.warn('Failed to check for an existing queue window: {error}', { error: String(error) })
    return
  }

  log.debug('Creating new transfer-queue window')

  const scale = getEffectiveScale()
  const width = BASE_WIDTH * scale
  const height = BASE_HEIGHT * scale

  // Pick a position: saved-and-on-screen, else clamped, else centered on main.
  const [main, monitors, saved] = await Promise.all([readMainRect(), readMonitors(), readSavedRect('queue')])
  const rect = main ? resolveChildPosition({ size: { width, height }, main, monitors, saved }) : null

  // Honor macOS "Reduce transparency": open an opaque window and skip the
  // vibrancy material (the page tokens flip to opaque under the
  // `reduce-transparency` class). Read the value from the backend (NSWorkspace),
  // NOT a media query: WKWebView doesn't reflect `prefers-reduced-transparency`.
  // `prefers-color-scheme` IS reflected, so dark detection stays a media query.
  let reduceTransparency: boolean
  try {
    reduceTransparency = await commands.getShouldReduceTransparency()
  } catch (error) {
    log.warn('Failed to read reduce-transparency; opening opaque: {error}', { error: String(error) })
    reduceTransparency = true
  }
  const darkAppearance = window.matchMedia('(prefers-color-scheme: dark)').matches
  const backgroundColor: [number, number, number, number] = reduceTransparency
    ? darkAppearance
      ? [30, 30, 30, 255]
      : [255, 255, 255, 255]
    : [0, 0, 0, 0]

  const win = new WebviewWindow('queue', {
    url: '/queue',
    title: decorateChildWindowTitle('Transfer queue'),
    width: rect?.width ?? width,
    height: rect?.height ?? height,
    minWidth: MIN_WIDTH * scale,
    minHeight: MIN_HEIGHT * scale,
    ...(rect ? { x: rect.x, y: rect.y } : { center: true }),
    resizable: true,
    minimizable: true,
    closable: true,
    decorations: true,
    focus: !isE2e,
    // Translucent glass backdrop via the macOS `NSVisualEffectView` material,
    // UNLESS the user reduces transparency (then open opaque). Requires
    // `tauri/macos-private-api` (enabled in `Cargo.toml`).
    transparent: !reduceTransparency,
    backgroundColor,
    // Overlay title bar so the traffic lights tuck into our own header row.
    // `hiddenTitle` keeps the OS from painting the window title text.
    titleBarStyle: 'overlay',
    hiddenTitle: true,
    trafficLightPosition: new LogicalPosition(14, 18),
  })

  // Apply the `NSVisualEffectView` material AFTER creation (the `windowEffects`
  // creation option drops silently in this Tauri version; `setEffects` is the
  // reliable IPC path, gated by `core:window:allow-set-effects` in
  // `capabilities/queue.json`). UnderWindowBackground reads as a clean utility
  // "HUD-ish" panel — the macOS convention for a transfer/activity manager —
  // and follows the window's active state. Radius matches `--radius-xxl`.
  if (!reduceTransparency) {
    void win.once('tauri://created', () => {
      void win
        .setEffects({
          effects: [Effect.UnderWindowBackground],
          state: EffectState.FollowsWindowActiveState,
          radius: 12,
        })
        .catch((error: unknown) => {
          log.warn('Failed to apply queue window effects: {error}', { error: String(error) })
        })
    })
  }

  // E2E: push the window behind everything so a run's windows don't pop in front
  // of the developer's work. No-op outside E2E.
  void orderChildWindowToBackInE2e(win)
}
