/**
 * Keyboard-shortcuts help window management.
 *
 * A read-only, narrow-and-tall window (Help > Keyboard shortcuts) listing every
 * command's shortcuts, live-synced with the user's customizations. It's a
 * sibling of the Settings and File-viewer windows: a separate `WebviewWindow`
 * on the `/shortcuts` route, opened (or focused if already open) by
 * `openShortcutsWindow`. The editing lives in Settings; this window only links
 * there.
 *
 * **Sizing.** The window opens narrow and tall (~1:3). Both dimensions scale
 * with the effective text size so the layout keeps its proportions, and the
 * height is capped to the target monitor's usable height so a tall window never
 * spawns partly off-screen. It's resizable; the in-window content scrolls.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import { emitTo } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { getEffectiveScale } from '$lib/text-size.svelte'
import { decorateChildWindowTitle, getAppMode, orderChildWindowToBackInE2e } from '$lib/app-mode'
import {
  centerOnMain,
  nearestMonitor,
  readMainRect,
  readMonitors,
  readSavedRect,
  resolveChildPosition,
} from '$lib/window-positioning'

const log = getAppLogger('shortcuts')

/** Base (scale = 1) window size. Narrow and tall, roughly a 1:3 ratio. */
const BASE_WIDTH = 360
const BASE_HEIGHT = 1000
const MIN_WIDTH = 300
const MIN_HEIGHT = 420
/** Margin kept below the monitor height so the window never reaches the screen edge. */
const MONITOR_HEIGHT_MARGIN = 80

/**
 * Opens the Keyboard shortcuts window, or focuses it if already open (singleton,
 * like Settings). Cross-window `setFocus()` doesn't reliably raise a window on
 * macOS, so an already-open window self-focuses via the `focus-self` event.
 */
export async function openShortcutsWindow(): Promise<void> {
  // E2E suites stealing OS focus on each open make the host unusable; the plugin
  // drives the webview over a socket, so it doesn't need focus. Mirrors Settings.
  const isE2e = getAppMode() === 'e2e'

  const existing = await WebviewWindow.getByLabel('shortcuts')
  if (existing) {
    if (!isE2e) {
      await emitTo('shortcuts', 'focus-self')
    }
    return
  }

  log.debug('Creating new keyboard shortcuts window')

  const scale = getEffectiveScale()
  const width = BASE_WIDTH * scale

  const [main, monitors, saved] = await Promise.all([readMainRect(), readMonitors(), readSavedRect('shortcuts')])

  // Cap the height to the target monitor's usable height so a tall window can't
  // spawn off the bottom of the screen.
  const desiredHeight = BASE_HEIGHT * scale
  const probe = main ? centerOnMain(main, { width, height: desiredHeight }) : null
  const monitor = probe ? nearestMonitor(probe, monitors) : (monitors[0] ?? null)
  const heightCap = monitor ? monitor.height - MONITOR_HEIGHT_MARGIN : desiredHeight
  const height = Math.max(MIN_HEIGHT, Math.min(desiredHeight, heightCap))

  const rect = main ? resolveChildPosition({ size: { width, height }, main, monitors, saved }) : null

  const win = new WebviewWindow('shortcuts', {
    url: '/shortcuts',
    title: decorateChildWindowTitle('Keyboard shortcuts'),
    width: rect?.width ?? width,
    height: rect?.height ?? height,
    minWidth: MIN_WIDTH * scale,
    minHeight: MIN_HEIGHT,
    ...(rect ? { x: rect.x, y: rect.y } : { center: true }),
    resizable: true,
    minimizable: true,
    closable: true,
    focus: !isE2e,
    // Overlay title bar with hidden title and the traffic lights tucked into the
    // top-left, matching the Settings/Viewer windows. Our own header row paints
    // the title and the "Edit shortcuts" link beneath the lights.
    titleBarStyle: 'overlay',
    hiddenTitle: true,
    trafficLightPosition: new LogicalPosition(14, 18),
  })
  // E2E: push the window behind everything so a run's windows don't pop in front
  // of the developer's work. No-op outside E2E. See `orderChildWindowToBackInE2e`.
  void orderChildWindowToBackInE2e(win)
}
