/**
 * E2E for the file viewer's optional text cursor (`viewer.showTextCursor`, off by
 * default).
 *
 * Only a real webview proves this one: the cursor's box comes from `Range` geometry
 * measured off a laid-out row, which jsdom has none of, and the setting reaches the
 * viewer through the restricted-window path under the real capability ACL (the viewer
 * has no `tauri-plugin-store` grant, so a unit mock can't reproduce it).
 *
 * The toggle is flipped through the MCP `set_setting` tool, the same store pipeline the
 * Settings row uses. The viewer has no control of its own for this setting, so it reaches
 * an open viewer over the cross-window `settings:changed` event and a fresh one through
 * the `get_restricted_window_settings` snapshot; the two tests below take one path each.
 *
 * ❗ The snapshot is read off `settings.json`, which the store writes on a 500 ms
 * debounce, so a viewer opened right after `set_setting` still sees the OLD value and no
 * live event (it didn't exist when the event fired). Wait for the value to land on disk
 * before opening, exactly as `viewer-wordwrap-persistence.spec.ts` does.
 *
 * ❌ Gestures go into the viewer webview via `viewer.evaluate(...)`, never through
 * OS-level `keyboard.press` or pointer moves: under Xvfb those land on the main window
 * and the viewer never sees them.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, ensureAppReady, openViewerWindow } from './helpers.js'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const SETTING_ID = 'viewer.showTextCursor'

// Reading the instance's own settings.json is how "the snapshot a new viewer will read is
// current" becomes deterministic. Requires an isolated instance; refusing to fall back to
// the developer's real config is deliberate.
const settingsFilePath = (() => {
  const dataDir = process.env.CMDR_DATA_DIR
  if (!dataDir) throw new Error('CMDR_DATA_DIR env var is not set; this spec needs an isolated app instance')
  return path.join(dataDir, 'settings.json')
})()

/** `viewer.showTextCursor` as persisted, or `undefined` while the file or key is absent. */
function textCursorOnDisk(): boolean | undefined {
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsFilePath, 'utf-8'))
    if (typeof parsed !== 'object' || parsed === null) return undefined
    const value: unknown = (parsed as Record<string, unknown>)[SETTING_ID]
    return typeof value === 'boolean' ? value : undefined
  } catch {
    // Absent or mid-write: treat as "not persisted yet" and keep polling.
    return undefined
  }
}

/** A box read off the rendered cursor element, or `null` when it isn't painted. */
interface CursorBox {
  left: number
  top: number
  height: number
  /** Painted as a sibling of `.lines-container`, inside `.scroll-spacer`. */
  inSpacer: boolean
  /** ❌ Must stay false: an extra child there corrupts `avgWrappedLineHeight`. */
  inLinesContainer: boolean
}

test.describe('File viewer text cursor', () => {
  const multiWordLine = 'alpha beta gamma delta'
  const cursorFile = path.join(os.tmpdir(), `cmdr-viewer-text-cursor-${String(process.pid)}.txt`)

  let viewer: TauriPage | null = null
  let viewerLabel = ''
  let mainPage: TauriPage

  test.beforeAll(() => {
    fs.writeFileSync(cursorFile, `${multiWordLine}\nsecond line\nthird line\n`)
  })

  test.afterAll(() => {
    fs.rmSync(cursorFile, { force: true })
  })

  test.beforeEach(async ({ tauriPage }) => {
    mainPage = tauriPage as TauriPage
    await ensureAppReady(mainPage)
    await ensureMcpClient(mainPage)
    // Wait for the default-off baseline to reach DISK, not just the store: a `true` left
    // by an earlier attempt would otherwise still be what the first viewer's snapshot
    // reads, and the "off paints nothing" assertions would fail for the wrong reason.
    await setTextCursor(false)
    await expect.poll(textCursorOnDisk, { timeout: 5000 }).not.toBe(true)
  })

  // Always close the viewer AND restore the default-off baseline, even on failure: the
  // app instance is shared, so a setting left on would change what the next spec renders.
  test.afterEach(async () => {
    if (viewer !== null) {
      await closeScopedWindow(mainPage, viewer, viewerLabel)
      viewer = null
    }
    await setTextCursor(false)
  })

  /** Flips the toggle through the same store pipeline the Settings row uses.
   *  `set_setting` round-trips, so the value is live when this resolves. */
  async function setTextCursor(value: boolean): Promise<void> {
    await mcpCall('set_setting', { id: SETTING_ID, value })
  }

  async function openViewer(): Promise<TauriPage> {
    const page = await openViewerWindow(mainPage, cursorFile)
    await page.waitForSelector('.viewer-container[data-window-ready="loaded"]', 8000)
    viewer = page
    const label = page.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = label
    return page
  }

  /** The middle of `[startOffset, endOffset)` on line 0, in client coordinates. Walks the
   *  text nodes because `.line-text` renders one segment per `{#each}` entry, so its
   *  children are a mix of text nodes, wrappers, and Svelte anchors. */
  async function midpointOfLineZero(startOffset: number, endOffset: number): Promise<{ x: number; y: number }> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    return await page.evaluate<{ x: number; y: number }>(`
            (function() {
                const lineText = document.querySelector('[data-line="0"] .line-text')
                if (!lineText) throw new Error('line 0 not found')
                const walker = document.createTreeWalker(lineText, NodeFilter.SHOW_TEXT)
                const range = document.createRange()
                let pos = 0
                let started = false
                let node = walker.nextNode()
                while (node) {
                    const len = (node.nodeValue || '').length
                    if (!started && ${String(startOffset)} <= pos + len) {
                        range.setStart(node, ${String(startOffset)} - pos)
                        started = true
                    }
                    if (started && ${String(endOffset)} <= pos + len) {
                        range.setEnd(node, ${String(endOffset)} - pos)
                        const rect = range.getBoundingClientRect()
                        return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
                    }
                    pos += len
                    node = walker.nextNode()
                }
                throw new Error('line 0 is shorter than the span it should hold: ' + pos + ' units')
            })()
        `)
  }

  /** One press and release at `point`, dispatched into the viewer webview. */
  async function clickAt(point: { x: number; y: number }): Promise<void> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    await page.evaluate(`
            (function() {
                const target = document.querySelector('.file-content')
                if (!target) throw new Error('file-content not found')
                function fire(type) {
                    target.dispatchEvent(new PointerEvent(type, {
                        bubbles: true, cancelable: true,
                        clientX: ${String(point.x)}, clientY: ${String(point.y)},
                        button: 0, pointerId: 5, pointerType: 'mouse',
                    }))
                }
                fire('pointerdown')
                fire('pointerup')
            })()
        `)
  }

  async function pressKeyTimes(
    key: string,
    modifiers: { shiftKey?: boolean; altKey?: boolean },
    times: number,
  ): Promise<void> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    const init = JSON.stringify({ key, bubbles: true, cancelable: true, ...modifiers })
    await page.evaluate(`
            (function() {
                for (let i = 0; i < ${String(times)}; i++) {
                    window.dispatchEvent(new KeyboardEvent('keydown', ${init}))
                }
            })()
        `)
  }

  async function readCursor(): Promise<CursorBox | null> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    return await page.evaluate<CursorBox | null>(`
            (function() {
                const el = document.querySelector('.text-cursor')
                if (!el) return null
                const rect = el.getBoundingClientRect()
                return {
                    left: rect.left,
                    top: rect.top,
                    height: rect.height,
                    inSpacer: el.parentElement !== null && el.parentElement.classList.contains('scroll-spacer'),
                    inLinesContainer: el.closest('.lines-container') !== null,
                }
            })()
        `)
  }

  /** The text painted as selected on line 0, read back out of the rendered segments. */
  async function selectedTextOnLineZero(): Promise<string> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    return await page.evaluate<string>(
      `Array.from(document.querySelectorAll('[data-line="0"] .selected')).map(function (n) { return n.textContent }).join('')`,
    )
  }

  /** The row-0 `.line` box, for comparing the cursor's vertical placement against. */
  async function lineZeroBox(): Promise<{ top: number; height: number }> {
    const page = viewer
    if (page === null) throw new Error('No viewer open')
    return await page.evaluate<{ top: number; height: number }>(`
            (function() {
                const el = document.querySelector('[data-line="0"]')
                if (!el) throw new Error('line 0 not found')
                const rect = el.getBoundingClientRect()
                return { top: rect.top, height: rect.height }
            })()
        `)
  }

  test('appears live when the setting goes on, paints at the click point, and follows Shift+Right', async () => {
    // Open FIRST, then flip: this is the cross-window live-update path, the one the
    // viewer depends on because it has no control of its own for this setting.
    await openViewer()
    const betaPoint = await midpointOfLineZero(6, 10)
    await clickAt(betaPoint)
    expect(await readCursor()).toBeNull()

    await setTextCursor(true)

    await expect.poll(readCursor, { timeout: 3000 }).not.toBeNull()
    const painted = await readCursor()
    if (painted === null) throw new Error('the text cursor vanished between two reads')

    // Mounted in `.scroll-spacer`, never in `.lines-container`, whose child count divides
    // its height into `avgWrappedLineHeight`.
    expect(painted.inSpacer).toBe(true)
    expect(painted.inLinesContainer).toBe(false)

    // Placed from the measured rect, so it lands on the clicked row rather than the
    // 10⁵-10⁷ px away that double-counting `linesOffset` would produce.
    const row = await lineZeroBox()
    expect(Math.abs(painted.top - row.top)).toBeLessThan(4)
    expect(painted.height).toBeGreaterThan(0)
    expect(painted.height).toBeLessThanOrEqual(row.height + 1)
    expect(Math.abs(painted.left - betaPoint.x)).toBeLessThan(20)

    // Shift+Right walks the focus, and the cursor follows it.
    await pressKeyTimes('ArrowRight', { shiftKey: true }, 4)
    await expect.poll(async () => (await readCursor())?.left ?? -1, { timeout: 3000 }).toBeGreaterThan(painted.left)

    // And back off again: the reactive read follows the setting in both directions.
    await setTextCursor(false)
    await expect.poll(readCursor, { timeout: 3000 }).toBeNull()
  })

  test('is already on in a viewer opened after the setting was saved', async () => {
    // The other half of the plumbing: a viewer that missed the change event entirely and
    // seeds from the `get_restricted_window_settings` snapshot instead.
    await setTextCursor(true)
    await expect.poll(textCursorOnDisk, { timeout: 5000 }).toBe(true)

    await openViewer()
    await clickAt(await midpointOfLineZero(6, 10))
    await expect.poll(readCursor, { timeout: 3000 }).not.toBeNull()
  })

  test('paints nothing while the setting is off', async () => {
    await openViewer()

    const gammaPoint = await midpointOfLineZero(11, 16)
    await clickAt(gammaPoint)
    // Extend from the click so the gesture leaves a visible mark: that's what proves the
    // viewer saw both events, and therefore that the absent cursor is the setting rather
    // than a press that never landed.
    await pressKeyTimes('ArrowRight', { shiftKey: true }, 3)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).not.toBe('')

    expect(await readCursor()).toBeNull()
  })
})
