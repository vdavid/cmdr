/**
 * Unit tests for `escapeOverlayUntilGone`, the E2E suite's close path for an overlay that
 * answers its first Escape with something other than a close.
 *
 * They run against happy-dom rather than the app because the thing under test is pure DOM
 * choreography: a query dialog spends press one stopping a live run (or handing it to an
 * open popover) and closes on press two, so a helper that presses once and then waits is
 * waiting on a close nobody asked for. That shape reproduces in happy-dom exactly, which
 * makes a browserless anchor for it possible.
 *
 * The payload under test is the `evaluate` STRING the helper ships into the webview, so
 * these tests execute that exact string; paraphrasing it in TypeScript would test a
 * different program than the one the suite runs.
 */

import { beforeEach, describe, expect, it } from 'vitest'
import type { PageLike } from './core.js'
import { escapeOverlayUntilGone } from './overlays-and-dialogs.js'

/** Every Escape whose handler actually ran, in order. */
let escapes: string[] = []

/**
 * A `PageLike` whose `evaluate` runs the helper's real payload against the happy-dom
 * document and whose `count` answers from the same document. The helper calls nothing else.
 */
const page = {
  evaluate: (js: string): Promise<unknown> => {
    // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the evaluate payload IS the code under test; the whole point is to run it verbatim.
    const run = new Function(`return ${js}`) as () => unknown
    return Promise.resolve(run())
  },
  count: (selector: string): Promise<number> => Promise.resolve(document.querySelectorAll(selector).length),
} as unknown as PageLike

/**
 * Stands up an overlay that unmounts on its `swallow`-th-plus-one Escape, modelling
 * `QueryDialog.resolveEscape`: presses before that are answered (a run stopped, a popover
 * closed) and are NOT a close.
 */
function addOverlay(swallow: number): void {
  const overlay = document.createElement('div')
  overlay.className = 'search-overlay'
  let seen = 0
  overlay.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return
    escapes.push('Escape')
    seen += 1
    if (seen > swallow) overlay.remove()
  })
  document.body.appendChild(overlay)
}

describe('escapeOverlayUntilGone', () => {
  beforeEach(() => {
    escapes = []
    document.body.innerHTML = ''
  })

  it('closes a dialog that spends its first Escape on something other than closing', async () => {
    // The shape that leaves a search dialog standing: the run under way owns press one,
    // so a helper that presses once reports a close that never happened, and the leak
    // guard fails whichever spec happens to be next on the shard.
    addOverlay(1)

    await escapeOverlayUntilGone(page, '.search-overlay', 2000)

    expect(document.querySelectorAll('.search-overlay')).toHaveLength(0)
    expect(escapes).toEqual(['Escape', 'Escape'])
  })

  it('presses once when once is all it takes, and never again after the overlay is gone', async () => {
    addOverlay(0)

    await escapeOverlayUntilGone(page, '.search-overlay', 2000)

    expect(escapes).toEqual(['Escape'])
  })

  it('still fails an overlay that never unmounts, however many presses it swallows', async () => {
    // Re-pressing must not turn a wedged dialog into a green test: the budget is what
    // ends it, and the count of swallowed presses is not an excuse to keep waiting.
    addOverlay(Number.POSITIVE_INFINITY)

    await expect(escapeOverlayUntilGone(page, '.search-overlay', 300)).rejects.toThrow()
    expect(document.querySelectorAll('.search-overlay')).toHaveLength(1)
  })
})
