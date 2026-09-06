/**
 * E2E for the file viewer's selection gestures: the multi-click cycle, granularity drags,
 * and keyboard extension.
 *
 * ❌ Every gesture is dispatched INTO the viewer webview via `viewer.evaluate(...)`, never
 * through OS-level `keyboard.press` or pointer moves: pressing into a scoped viewer window
 * flakes under Xvfb, where the keystroke lands on the main window and the viewer webview
 * never sees it.
 *
 * Test file: a three-line temp file (a multi-word line, a short line, and one far wider
 * than any window). It lives outside the shared fixture tree on purpose — that tree is
 * manifest-guarded, and the global `afterEach` fails any spec leaving churn in `left/` or
 * `right/`.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow, CTRL_OR_META } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Opens a viewer for `filePath` and waits for it to be interactive. */
async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 8000)
  return viewer
}

test.describe('File viewer selection gestures', () => {
  // Several words on line 0, so a word selection and a line selection differ.
  const multiWordLine = 'alpha beta gamma'
  // Line 2 is far wider than any window, so bare Left / Right have something to scroll.
  const wideLine = 'x'.repeat(2000)
  const multiWordFile = path.join(os.tmpdir(), `cmdr-viewer-selection-${String(process.pid)}.txt`)

  let viewer: TauriPage
  let viewerLabel: string

  test.beforeAll(() => {
    fs.writeFileSync(multiWordFile, `${multiWordLine}\nsecond line\n${wideLine}\n`)
  })

  test.afterAll(() => {
    fs.rmSync(multiWordFile, { force: true })
  })

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, multiWordFile)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  /**
   * The middle of `[startOffset, endOffset)` on line 0, in client coordinates.
   *
   * Walks the line's text nodes to reach those offsets rather than indexing into the
   * first child: `.line-text` renders one segment per `{#each}` entry, so its children
   * are a mix of text nodes, `<span class="selected">` / `<mark>` wrappers, and Svelte's
   * own anchors. Read once, before anything is selected; the geometry doesn't move, so
   * one reading serves every press.
   */
  async function midpointOfLineZero(startOffset: number, endOffset: number): Promise<{ x: number; y: number }> {
    const start = String(startOffset)
    const end = String(endOffset)
    return await viewer.evaluate<{ x: number; y: number }>(`
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
                    if (!started && ${start} <= pos + len) { range.setStart(node, ${start} - pos); started = true }
                    if (started && ${end} <= pos + len) {
                        range.setEnd(node, ${end} - pos)
                        const rect = range.getBoundingClientRect()
                        return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
                    }
                    pos += len
                    node = walker.nextNode()
                }
                throw new Error('line 0 is shorter than the word it should hold: ' + pos + ' units')
            })()
        `)
  }

  /** The middle of the word `beta` (offsets 6-10 of line 0). */
  async function betaPoint(): Promise<{ x: number; y: number }> {
    return await midpointOfLineZero(6, 10)
  }

  /** The middle of the word `gamma` (offsets 11-16 of line 0). */
  async function gammaPoint(): Promise<{ x: number; y: number }> {
    return await midpointOfLineZero(11, 16)
  }

  /**
   * Presses and releases `times` times at `point`, WITHOUT setting `detail`: the viewer
   * counts the presses itself off the `pointerdown` stream, so this exercises our counter
   * rather than the engine's click count. A `detail` set by hand here would pass whatever
   * the app does.
   */
  async function pressAt(point: { x: number; y: number }, times: number): Promise<void> {
    await viewer.evaluate(`
            (function() {
                const target = document.querySelector('.file-content')
                if (!target) throw new Error('file-content not found')
                function fire(type) {
                    target.dispatchEvent(new PointerEvent(type, {
                        bubbles: true, cancelable: true,
                        clientX: ${String(point.x)}, clientY: ${String(point.y)},
                        button: 0, pointerId: 3, pointerType: 'mouse',
                    }))
                }
                for (let i = 0; i < ${String(times)}; i++) {
                    fire('pointerdown')
                    fire('pointerup')
                }
            })()
        `)
  }

  /**
   * Double-presses at `from`, drags to `to`, and releases there.
   *
   * All five events go out in one `evaluate` so the two presses stay well inside the
   * multi-click interval, and so the whole gesture reaches the viewer webview (see the
   * file header on why OS-level events don't).
   */
  async function doublePressAndDragTo(from: { x: number; y: number }, to: { x: number; y: number }): Promise<void> {
    await viewer.evaluate(`
            (function() {
                const target = document.querySelector('.file-content')
                if (!target) throw new Error('file-content not found')
                function fire(type, x, y) {
                    target.dispatchEvent(new PointerEvent(type, {
                        bubbles: true, cancelable: true,
                        clientX: x, clientY: y,
                        button: 0, pointerId: 4, pointerType: 'mouse',
                    }))
                }
                const fromX = ${String(from.x)}, fromY = ${String(from.y)}
                fire('pointerdown', fromX, fromY)
                fire('pointerup', fromX, fromY)
                fire('pointerdown', fromX, fromY)
                fire('pointermove', ${String(to.x)}, ${String(to.y)})
                fire('pointerup', ${String(to.x)}, ${String(to.y)})
            })()
        `)
  }

  /** The text painted as selected on line 0, read back out of the rendered segments. */
  async function selectedTextOnLineZero(): Promise<string> {
    return await viewer.evaluate<string>(
      `Array.from(document.querySelectorAll('[data-line="0"] .selected')).map(function (n) { return n.textContent }).join('')`,
    )
  }

  /**
   * Presses `key` `times` times with the given modifiers, dispatched into the viewer
   * webview's own `window` (the page binds its router with `<svelte:window on:keydown>`).
   */
  async function pressKeyTimes(
    key: string,
    modifiers: { shiftKey?: boolean; altKey?: boolean; metaKey?: boolean; ctrlKey?: boolean },
    times: number,
  ): Promise<void> {
    const init = JSON.stringify({ key, bubbles: true, cancelable: true, ...modifiers })
    await viewer.evaluate(`
            (function() {
                for (let i = 0; i < ${String(times)}; i++) {
                    window.dispatchEvent(new KeyboardEvent('keydown', ${init}))
                }
            })()
        `)
  }

  test('a double press selects the word and a triple press the whole line', async () => {
    const point = await betaPoint()

    await pressAt(point, 2)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta')

    // Wait out the multi-click interval so the next burst starts its own cycle.
    await new Promise((resolve) => setTimeout(resolve, 700))

    await pressAt(point, 3)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe(multiWordLine)

    // One more press is a plain click either way (a restarted cycle if it lands inside
    // the interval, a fresh one if it doesn't), so the line selection gives way to a
    // caret instead of sticking. The cycle's exact wrap-around is pinned in
    // `viewer-multi-click.test.ts`, where the clock is an argument.
    await pressAt(point, 1)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('')
  })

  test('a drag after a double press keeps selecting whole words', async () => {
    const from = await betaPoint()
    const to = await gammaPoint()

    await doublePressAndDragTo(from, to)

    // Both words in full: the drag runs at the double-press's granularity instead of
    // staying stuck on the first word.
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta gamma')
  })

  test('Shift+Right extends the selection one character at a time, and the copy chord takes it', async () => {
    // Seed a selection the user can see: a double press selects `beta` (offsets 6-10).
    await pressAt(await betaPoint(), 2)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta')

    // Five characters further right, keeping the anchor: [6, 15) of `alpha beta gamma`.
    await pressKeyTimes('ArrowRight', { shiftKey: true }, 5)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta gamm')

    // The copy modifier comes from the platform: these specs also run on Linux Docker.
    await pressKeyTimes('c', { metaKey: CTRL_OR_META === 'Meta', ctrlKey: CTRL_OR_META === 'Control' }, 1)
    await expect
      .poll(
        async () => {
          const text = (await viewer.textContent('.toast')) ?? ''
          return text.includes('on your clipboard')
        },
        { timeout: 5000 },
      )
      .toBeTruthy()

    const clip = await viewer.evaluate<string>(
      `(async () => { try { return await navigator.clipboard.readText() } catch { return '' } })()`,
    )
    expect(clip).toBe('beta gamm')
  })

  test('unmodified Left / Right scroll the view horizontally', async () => {
    // A previously failed run can leave the persisted word wrap on, and wrapped content
    // has no horizontal overflow to scroll.
    const wrapped = await viewer.evaluate<boolean>(
      `document.querySelector('.file-content')?.classList.contains('word-wrap') ?? false`,
    )
    if (wrapped) await pressKeyTimes('w', {}, 1)

    const scrollLeft = async () =>
      await viewer.evaluate<number>(`document.querySelector('.file-content')?.scrollLeft ?? -1`)
    expect(await scrollLeft()).toBe(0)

    await pressKeyTimes('ArrowRight', {}, 10)
    await expect.poll(scrollLeft, { timeout: 3000 }).toBeGreaterThan(0)

    // Back past the left edge: the step clamps at 0 rather than going negative.
    await pressKeyTimes('ArrowLeft', {}, 20)
    await expect.poll(scrollLeft, { timeout: 3000 }).toBe(0)
  })

  test('Option+Shift+Right extends by a whole word', async () => {
    await pressAt(await betaPoint(), 2)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta')

    // macOS semantics: the focus lands on the END of the next word, so `gamma` joins in
    // full rather than the selection stopping at its start.
    await pressKeyTimes('ArrowRight', { shiftKey: true, altKey: true }, 1)
    await expect.poll(selectedTextOnLineZero, { timeout: 3000 }).toBe('beta gamma')
  })
})
