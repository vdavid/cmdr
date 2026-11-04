import { getAppLogger } from '#lib/logging/logger.js'
import { showMainWindow } from '#lib/tauri-commands/index.js'
import { waitForNextPaint } from '#lib/utils/timing.js'

const log = getAppLogger('startup')

/**
 * How long to wait for a frame AFTER the window is shown before treating the
 * paint as stuck. Nothing waits on this to display the window, so it only
 * controls how quickly the repair below kicks in.
 */
const PAINT_AFTER_SHOW_TIMEOUT_MS = 1000

/**
 * How long after the repair to keep waiting before calling the window stuck.
 * A frame that lands inside this is late, not stuck: the 2015 MacBook Air on
 * Catalina misses the first deadline on every launch while both panes mount,
 * then paints (Cmdr 0.59.0, 10.15.8, four of four launches, 2026-11-04).
 */
const PAINT_AFTER_REPAIR_TIMEOUT_MS = 9000

/**
 * Shows the main window as soon as the app has mounted.
 *
 * The window is created `visible: false` (`tauri.conf.json`) and the backend
 * places it during setup without showing it, so this is what makes Cmdr
 * appear. (If it never runs, the backend shows the window after a delay:
 * `src-tauri/src/main_window_show.rs`.) It runs after `onMount`, by which
 * point the frontend has hydrated and built the DOM.
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
 * After the repair it keeps waiting, so the log says what happened: a late
 * frame (a slow Mac with the main thread busy mounting the panes) logs the
 * real time to first frame at info, and only a window that still hasn't
 * painted after the full wait warns.
 *
 * Call fire-and-forget from `onMount` so it never holds up listener setup.
 */
export async function showMainOnMount(): Promise<void> {
  await showMainWindow('launch')
  const shownAt = performance.now()

  if ((await waitForNextPaint(PAINT_AFTER_SHOW_TIMEOUT_MS)) === 'painted') {
    log.debug('Main window shown and a frame landed after {ms}ms', { ms: elapsedSince(shownAt) })
    return
  }

  await showMainWindow('repaint-repair')
  if ((await waitForNextPaint(PAINT_AFTER_REPAIR_TIMEOUT_MS)) === 'painted') {
    log.info('First frame landed {ms}ms after showing the main window (re-showed at {repairMs}ms to be safe)', {
      ms: elapsedSince(shownAt),
      repairMs: PAINT_AFTER_SHOW_TIMEOUT_MS,
    })
    return
  }

  log.warn('No frame within {ms}ms of showing the main window, even after re-showing it to force a repaint', {
    ms: elapsedSince(shownAt),
  })
}

function elapsedSince(start: number): number {
  return Math.round(performance.now() - start)
}
