/**
 * Unit tests for `clickEntryInPane`, the click `ensureAppReady` puts on the first
 * row of the left pane before every E2E test.
 *
 * The race they anchor: a pane replaces every row when a listing lands, and
 * `ensureAppReady`'s own navigation lands one right after a route remount. Measured
 * in the E2E lane (2026-09-12): every row of both panes was removed ~25 ms before
 * the click on an idle machine, and under a loaded `--include-slow` run the swap
 * landed between the helper's "is the row there" round trip and its "click it"
 * round trip, failing the test on a row that was back a moment later.
 *
 * Like `click-button-by-text.test.ts`, these execute the helper's real `evaluate`
 * payload against happy-dom, so the program under test is the one the suite ships.
 */

import { beforeEach, describe, expect, it } from 'vitest'
import type { PageLike } from './core.js'
import { clickEntryInPane } from './core.js'

/** `data-filename` of every row whose click handler ran, in order. */
let clicks: string[] = []

/** Replaces pane 0's rows with fresh elements, the way a landed listing re-renders them. */
function renderRows(): void {
  const list = document.querySelector('.file-pane .rows')
  if (!list) return
  list.innerHTML = ''
  for (const name of ['..', 'file-a.txt']) {
    const row = document.createElement('div')
    row.className = 'file-entry'
    row.setAttribute('data-filename', name)
    row.addEventListener('click', () => {
      clicks.push(name)
    })
    list.appendChild(row)
  }
}

/**
 * A `PageLike` running each payload against happy-dom. `afterFirstRoundTrip` runs
 * once, right after the first `evaluate` resolves its result and before the next
 * one can start: the gap a real round trip leaves for the app to re-render in.
 */
function pageWith(afterFirstRoundTrip: () => void): PageLike {
  let calls = 0
  return {
    evaluate: (js: string): Promise<unknown> => {
      // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the evaluate payload IS the code under test; the whole point is to run it verbatim.
      const run = new Function(`return ${js}`) as () => unknown
      const result = run()
      calls += 1
      if (calls === 1) afterFirstRoundTrip()
      return Promise.resolve(result)
    },
  } as unknown as PageLike
}

describe('clickEntryInPane', () => {
  beforeEach(() => {
    clicks = []
    document.body.innerHTML = '<div class="file-pane"><div class="rows"></div></div>'
  })

  it('still clicks when the pane re-renders its rows between two round trips', async () => {
    renderRows()
    // The listing lands: rows go, and fresh ones follow a beat later.
    const page = pageWith(() => {
      const list = document.querySelector('.file-pane .rows')
      if (list) list.innerHTML = ''
      setTimeout(renderRows, 60)
    })

    await clickEntryInPane(page, 0)

    expect(clicks).toEqual(['..'])
  })

  it('waits for a row that renders only once its listing lands', async () => {
    const page = pageWith(() => {})
    setTimeout(renderRows, 80)

    await clickEntryInPane(page, 0, 1)

    expect(clicks).toEqual(['file-a.txt'])
  })
})
