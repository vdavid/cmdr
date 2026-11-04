/**
 * The pane-split leak: a test that moves the divider and leaves it there.
 *
 * The split persists across tests (it's app layout, and `ensureAppReady` doesn't
 * touch it), so every later spec on the shard meets the off-center panes. A pane
 * width changes what renders, most visibly Full view's narrow-pane fit, which
 * drops the date column below a width. That's how one drag in
 * `brief-cursor-visibility` failed `pane-controls` ~100 specs later, and only in
 * the full suite. The global leak guard (`fixtures.ts`) reads the split after
 * every test, names the test that moved it, and re-centers it.
 */

import type { EvaluatablePage } from './app-death.js'

/** The left pane wrapper's inline width when the split is centered (`CENTER_SPLIT_PERCENT`). */
const CENTERED = '50%'

/**
 * The leaked split, or `null` when there's nothing to report: centered, or nothing
 * readable (no pane wrapper, or no page: the production-binding Escape tests close
 * the window they ran in, and `evaluate` then resolves to `undefined`). A test that
 * left the explorer or killed the app has a louder failure of its own, and the
 * guard mustn't mask it.
 */
export function paneSplitLeak(leftWrapperWidth: unknown): string | null {
  if (typeof leftWrapperWidth !== 'string' || leftWrapperWidth === '') return null
  return leftWrapperWidth === CENTERED ? null : leftWrapperWidth
}

const READ_LEFT_WIDTH = `(function(){
  var wrapper = document.querySelector('.dual-pane-explorer .pane-wrapper');
  return wrapper ? wrapper.style.width : null;
})()`

/** Reads the split, and when a test left it off center, re-centers it and returns the report. */
export async function reportAndResetPaneSplit(tauriPage: EvaluatablePage): Promise<string | null> {
  let leaked: string | null
  try {
    leaked = paneSplitLeak(await tauriPage.evaluate<unknown>(READ_LEFT_WIDTH))
  } catch {
    return null
  }
  if (leaked === null) return null
  try {
    // The production reset (`pane.resetDivider`, the divider's double-click), so it persists too.
    await tauriPage.evaluate(`(function(){
      var resizer = document.querySelector('.dual-pane-explorer .pane-resizer');
      if (resizer) resizer.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    })()`)
  } catch {
    // Best-effort repair; the report below is the load-bearing signal.
  }
  return (
    `Test left the pane divider off center (left pane at ${leaked}). Every later spec on the shard ` +
    `would meet the narrower pane (Full view drops its date column, for one). Re-center it before the ` +
    `test ends: double-click \`.pane-resizer\`, or \`dispatchMenuCommand(tauriPage, 'pane.resetDivider')\`. ` +
    `The guard has re-centered it for the next test.`
  )
}
