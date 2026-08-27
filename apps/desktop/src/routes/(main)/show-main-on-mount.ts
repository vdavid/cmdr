import { getAppLogger } from '$lib/logging/logger'
import { showMainWindow } from '$lib/tauri-commands'
import { waitForNextPaint } from '$lib/utils/timing'

const log = getAppLogger('startup')

/**
 * How long to wait for a frame AFTER the window is shown before treating the
 * paint as stuck. Nothing waits on this to display the window, so it only
 * controls how quickly the repair below kicks in.
 */
const PAINT_AFTER_SHOW_TIMEOUT_MS = 1000

/**
 * Shows the main window as soon as the app has mounted.
 *
 * The window is created `visible: false` (`tauri.conf.json`) and the backend
 * places it during setup without showing it, so this is the only thing that
 * makes Cmdr appear. It runs after `onMount`, by which point the frontend has
 * hydrated and built the DOM.
 *
 * **Why there's no paint gate before the show.** There used to be one, waiting
 * on `waitForNextPaint` first. It could never succeed: WebKit throttles
 * `requestAnimationFrame` in a hidden window, so every launch burned the full
 * timeout and then showed anyway, costing a fixed second of startup for no
 * signal. (It went unnoticed because the old window-state plugin showed the
 * window before the frontend ran, which made the gate a no-op re-show.)
 *
 * The check now happens *after* the show, where rAF is no longer throttled and
 * the answer means something: it tells us whether a frame actually landed. If
 * one doesn't, we re-show as a repair — `makeKeyAndOrderFront:` re-invalidates
 * the view, and a blank window that never repaints is otherwise stuck until
 * the user resizes it (observed once on a cold prod launch during a heavy
 * full-root reindex). The repair passes `'repaint-repair'` so it repaints
 * without taking the front back from an app the user switched to meanwhile.
 *
 * Call fire-and-forget from `onMount` so it never holds up listener setup.
 */
export async function showMainOnMount(): Promise<void> {
  await showMainWindow('launch')

  const paint = await waitForNextPaint(PAINT_AFTER_SHOW_TIMEOUT_MS)
  if (paint === 'painted') {
    log.debug('Main window shown and a frame landed')
    return
  }

  log.warn('No frame within {ms}ms of showing the main window; re-showing to force a repaint', {
    ms: PAINT_AFTER_SHOW_TIMEOUT_MS,
  })
  await showMainWindow('repaint-repair')
}
