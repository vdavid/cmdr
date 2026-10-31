/**
 * A wheel over the pane divider, against REAL layout: the handler in `PaneResizer.svelte` asks `elementsFromPoint` what's
 * under the pointer, and only the running webview answers that honestly. The test DOM has no layout, which is how a
 * version with a dead line shipped with a green unit test.
 *
 * The line has its own 1px column, so when the split lands on a whole pixel (any odd window width at 50%) no pane lies
 * under it. These pin that state on purpose: `installOddWidthLayout` forces it whatever the window's width is.
 *
 * The wheels are dispatched, so they run the handler but not the webview's native scrolling. That's what makes a moved
 * scroll offset here the handler's doing.
 */

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { ensureAppReady, executeViaCommandPalette, getFixtureRoot } from './helpers.js'

type TauriPage = Parameters<typeof ensureAppReady>[0]

const FIXTURE_SUBDIR = 'divider-wheel-fixtures'
const LEFT_LIST = '.pane-wrapper:first-of-type [data-file-list-surface]'
const RIGHT_LIST = '.pane-wrapper:last-of-type [data-file-list-surface]'

function ensureFixture(): string {
  const dir = path.join(getFixtureRoot(), FIXTURE_SUBDIR)
  if (fs.existsSync(dir) && fs.readdirSync(dir).length >= 300) return dir
  fs.rmSync(dir, { recursive: true, force: true })
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < 300; i++) {
    fs.writeFileSync(path.join(dir, `file-with-a-reasonably-long-name-${String(i).padStart(3, '0')}.txt`), 'A')
  }
  return dir
}

async function navigatePane(tauriPage: TauriPage, pane: 'left' | 'right', target: string): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
    event: 'mcp-nav-to-path',
    payload: { pane: ${JSON.stringify(pane)}, path: ${JSON.stringify(target)} },
  })`)
}

/** Gives the explorer an odd pixel width at a 50% split, so the line sits on whole pixels. Returns the line's left edge. */
async function installOddWidthLayout(tauriPage: TauriPage): Promise<number> {
  await tauriPage.evaluate(`(function () {
    var explorer = document.querySelector('.dual-pane-explorer');
    var resizer = explorer.querySelector('.pane-resizer');
    resizer.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    var width = Math.floor(explorer.getBoundingClientRect().width);
    explorer.style.flex = 'none';
    explorer.style.width = String(width % 2 === 0 ? width - 1 : width) + 'px';
  })()`)
  await expect
    .poll(
      () =>
        tauriPage.evaluate<boolean>(
          `Number.isInteger(document.querySelector('.pane-resizer').getBoundingClientRect().left)`,
        ),
      { timeout: waitBudget(5000) },
    )
    .toBe(true)
  return tauriPage.evaluate<number>(`document.querySelector('.pane-resizer').getBoundingClientRect().left`)
}

async function removeOddWidthLayout(tauriPage: TauriPage): Promise<void> {
  await tauriPage.evaluate(`(function () {
    var explorer = document.querySelector('.dual-pane-explorer');
    explorer.style.flex = '';
    explorer.style.width = '';
  })()`)
}

interface WheelResult {
  /** Whether the point lies on the divider at all (else this reading proves nothing about its handler). */
  hitsResizer: boolean
  /** Whether any pane's list lies under the point, as the webview's own layout answers it. */
  hasListUnder: boolean
  left: { x: number; y: number }
  right: { x: number; y: number }
}

/** Dispatches a wheel at client coordinates to whatever is on top there, then reports how far each list moved. */
async function wheelAt(
  tauriPage: TauriPage,
  point: { x: number; deltaX?: number; deltaY?: number },
): Promise<WheelResult> {
  return tauriPage.evaluate<WheelResult>(`(function () {
    var left = document.querySelector(${JSON.stringify(LEFT_LIST)});
    var right = document.querySelector(${JSON.stringify(RIGHT_LIST)});
    var box = right.getBoundingClientRect();
    var x = ${String(point.x)};
    var y = Math.round(box.top + box.height / 3);
    left.scrollTop = 0; left.scrollLeft = 0; right.scrollTop = 0; right.scrollLeft = 0;
    var stack = document.elementsFromPoint(x, y);
    stack[0].dispatchEvent(new WheelEvent('wheel', {
      bubbles: true, cancelable: true, clientX: x, clientY: y,
      deltaX: ${String(point.deltaX ?? 0)}, deltaY: ${String(point.deltaY ?? 0)},
    }));
    return {
      hitsResizer: !!stack[0].closest('.pane-resizer'),
      hasListUnder: stack.some(function (e) { return !!e.closest('[data-file-list-surface]'); }),
      left: { x: left.scrollLeft, y: left.scrollTop },
      right: { x: right.scrollLeft, y: right.scrollTop },
    };
  })()`)
}

/** Waits until a list has loaded enough rows to overflow on an axis, or there'd be nothing for a wheel to move. */
async function waitForOverflow(tauriPage: TauriPage, selector: string, axis: 'x' | 'y'): Promise<void> {
  const [scroll, client] = axis === 'x' ? ['scrollWidth', 'clientWidth'] : ['scrollHeight', 'clientHeight']
  await expect
    .poll(
      () =>
        tauriPage.evaluate<boolean>(`(function () {
          var list = document.querySelector(${JSON.stringify(selector)});
          return !!list && list.${scroll} > list.${client};
        })()`),
      { timeout: waitBudget(10000) },
    )
    .toBeTruthy()
}

test.describe('A wheel over the pane divider', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const dir = ensureFixture()
    await navigatePane(tauriPage, 'left', dir)
    await navigatePane(tauriPage, 'right', dir)
  })

  test.afterEach(async ({ tauriPage }) => {
    await removeOddWidthLayout(tauriPage)
  })

  test('in Full view, the line and the strip scroll the right pane', async ({ tauriPage }) => {
    await waitForOverflow(tauriPage, LEFT_LIST, 'y')
    await waitForOverflow(tauriPage, RIGHT_LIST, 'y')
    const lineLeft = await installOddWidthLayout(tauriPage)

    const onLine = await wheelAt(tauriPage, { x: lineLeft, deltaY: 120 })
    expect(onLine.hitsResizer).toBe(true)
    // The state this spec exists for: without this, it isn't testing the dead line.
    expect(onLine.hasListUnder).toBe(false)
    expect(onLine.right.y).toBe(120)
    expect(onLine.left.y).toBe(0)

    const onStrip = await wheelAt(tauriPage, { x: lineLeft + 4, deltaY: 120 })
    expect(onStrip.hitsResizer).toBe(true)
    expect(onStrip.right.y).toBe(120)
    expect(onStrip.left.y).toBe(0)
  })

  test('in Brief view, a sideways wheel scrolls on the line and the strip; a vertical one does what it does on the pane', async ({
    tauriPage,
  }) => {
    await tauriPage.locator('.pane-wrapper:last-of-type .file-pane').click()
    await executeViaCommandPalette(tauriPage, 'Brief view')
    try {
      await waitForOverflow(tauriPage, RIGHT_LIST, 'x')
      const lineLeft = await installOddWidthLayout(tauriPage)

      const onLine = await wheelAt(tauriPage, { x: lineLeft, deltaX: 120 })
      expect(onLine.hitsResizer).toBe(true)
      expect(onLine.hasListUnder).toBe(false)
      expect(onLine.right.x).toBe(120)

      const onStrip = await wheelAt(tauriPage, { x: lineLeft + 4, deltaX: 120 })
      expect(onStrip.right.x).toBe(120)

      // A Brief list has no vertical scroll: nothing moves on the pane, so nothing moves here.
      const vertical = await wheelAt(tauriPage, { x: lineLeft + 4, deltaY: 120 })
      expect(vertical.right).toEqual({ x: 0, y: 0 })
    } finally {
      await executeViaCommandPalette(tauriPage, 'Full view')
    }
  })
})
