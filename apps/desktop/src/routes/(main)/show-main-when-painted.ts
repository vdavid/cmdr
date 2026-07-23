import { getAppLogger } from '$lib/logging/logger'
import { showMainWindow } from '$lib/tauri-commands'
import { waitForNextPaint } from '$lib/utils/timing'

const log = getAppLogger('startup')

/**
 * How long to wait for the webview's first paint before showing the main
 * window anyway. Generous enough not to false-trip on a slow first paint under
 * heavy startup load, short enough that a genuinely stuck paint doesn't keep
 * the window hidden noticeably.
 */
const FIRST_PAINT_TIMEOUT_MS = 1000

/**
 * Shows the main window once the webview has actually painted a frame.
 *
 * The window launches `visible: false` and the frontend calls `show()` when
 * ready, to avoid a white flash. But showing the instant `onMount` reaches the
 * call can still race the compositor: if `show()` (makeKeyAndOrderFront) lands
 * before the first frame is presented and nothing invalidates the view
 * afterward, the window can sit blank until the next repaint (only a resize or
 * a full relaunch clears it). Gating `show()` on a confirmed paint closes that
 * race.
 *
 * Call fire-and-forget from `onMount` so it never holds up listener setup. The
 * timeout fallback in `waitForNextPaint` guarantees the window still shows even
 * if rAF is throttled while the window is hidden; a `warn` then flags that the
 * paint was never confirmed (rare, and worth seeing in telemetry).
 */
export async function showMainWhenPainted(): Promise<void> {
  const startedAt = performance.now()
  const paint = await waitForNextPaint(FIRST_PAINT_TIMEOUT_MS)
  const elapsedMs = Math.round(performance.now() - startedAt)

  if (paint === 'timeout') {
    log.warn('First paint not confirmed within {ms}ms; showing the main window anyway (it may briefly appear blank)', {
      ms: FIRST_PAINT_TIMEOUT_MS,
    })
  } else {
    log.debug('First paint confirmed after {ms}ms; showing the main window', { ms: elapsedMs })
  }

  await showMainWindow()
}
