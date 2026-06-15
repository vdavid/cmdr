/**
 * Regression E2E for horizontal scroll-to-match in the file viewer (no-wrap mode).
 *
 * `scrollToMatch` used to set only `scrollTop`, so jumping to a match far along a
 * long line highlighted it at the correct logical spot but left it scrolled out
 * of view horizontally. The pure centring math lives in
 * `viewer-search-scroll.test.ts`; what only an E2E can verify is the
 * cross-component flow: search → active `mark` rendered → the viewer scrolls it
 * into the horizontal viewport. Asserting the `mark.active` rect sits inside the
 * `.file-content` bounds (not merely that it exists in the DOM) is the check that
 * would fail before the fix.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const FIXTURE_DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-viewer-hscroll-'))
const FIXTURE_PATH = path.join(FIXTURE_DIR, 'long-line.txt')

// Two tokens sharing a prefix: `ZQXFIND_LONG` sits near the far right of a very
// long first line (column ~3000, and far down it once wrapped), `ZQXFIND_TAIL`
// sits on a short line far below it (past a block of filler). Searching the
// unique token yields one match; searching the shared prefix yields two on
// different lines, which drives the cross-line navigation case.
const PREFIX = 'ZQXFIND'
const TOKEN_LONG = `${PREFIX}_LONG`
const TOKEN_TAIL = `${PREFIX}_TAIL`
const LONG_LINE = `${'abcdefghij '.repeat(300)}${TOKEN_LONG}`
const FILLER = Array.from({ length: 40 }, (_, i) => `filler line number ${String(i)}`).join('\n')
const FIXTURE_CONTENT = [LONG_LINE, FILLER, `the tail line has ${TOKEN_TAIL} on it`].join('\n')

async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 8000)
  return viewer
}

/** True when the status bar shows the "wrap" badge. */
function wrapBadgeVisible(viewer: TauriPage): Promise<boolean> {
  return viewer.evaluate<boolean>(`
    (function () {
      const badges = document.querySelectorAll('.status-bar .backend-badge')
      for (const badge of badges) {
        if ((badge.textContent || '').trim() === 'wrap') return true
      }
      return false
    })()
  `)
}

/** Forces word wrap to a known state regardless of any persisted setting (the
 *  `w` key toggles it; a previous run can leave it either way). */
async function ensureWrap(viewer: TauriPage, on: boolean): Promise<void> {
  if ((await wrapBadgeVisible(viewer)) === on) return
  await viewer.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', bubbles: true }))`)
  await expect.poll(() => wrapBadgeVisible(viewer), { timeout: 3000 }).toBe(on)
}

test.describe('File viewer horizontal scroll-to-match', () => {
  let viewer: TauriPage
  let viewerLabel: string

  test.beforeAll(() => {
    fs.writeFileSync(FIXTURE_PATH, FIXTURE_CONTENT, 'utf-8')
  })

  test.afterAll(() => {
    try {
      fs.rmSync(FIXTURE_DIR, { recursive: true, force: true })
    } catch {
      // best-effort cleanup
    }
  })

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, FIXTURE_PATH)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  test('scrolls an off-screen match into the horizontal viewport', async () => {
    await ensureWrap(viewer, false)

    await viewer.keyboard.press('Control+f')
    await viewer.waitForSelector('.search-bar', 5000)

    await viewer.fill('.search-input', TOKEN_LONG)
    await expect
      .poll(async () => ((await viewer.textContent('.match-count')) ?? '').includes('1 of 1'), { timeout: 5000 })
      .toBeTruthy()

    // Jump to the (only) match. This calls scrollToMatch.
    await viewer.evaluate(`
      (function () {
        const btn = document.querySelector('button[aria-label="Next match"]')
        if (!btn) throw new Error('next-match button not found')
        btn.click()
      })()
    `)

    // The active match must end up inside the content viewport's horizontal
    // bounds, and the view must actually have scrolled right to get there.
    await expect
      .poll(
        () =>
          viewer.evaluate<{ inView: boolean; scrolledRight: boolean }>(`
            (function () {
              const content = document.querySelector('.file-content')
              const mark = document.querySelector('mark.active')
              if (!content || !mark) return { inView: false, scrolledRight: false }
              const c = content.getBoundingClientRect()
              const m = mark.getBoundingClientRect()
              return {
                inView: m.left >= c.left - 1 && m.right <= c.right + 1,
                scrolledRight: content.scrollLeft > 0,
              }
            })()
          `),
        { timeout: 5000 },
      )
      .toEqual({ inView: true, scrolledRight: true })
  })

  test('scrolls a match down a wrapped line into the vertical viewport', async () => {
    // Word wrap on: the long line wraps into many visual rows, so the match near
    // its end sits far below the line top. The vertical jump must land on the
    // match's wrapped row, not the top of the line.
    await ensureWrap(viewer, true)

    await viewer.keyboard.press('Control+f')
    await viewer.waitForSelector('.search-bar', 5000)

    await viewer.fill('.search-input', TOKEN_LONG)
    await expect
      .poll(async () => ((await viewer.textContent('.match-count')) ?? '').includes('1 of 1'), { timeout: 5000 })
      .toBeTruthy()

    await viewer.evaluate(`
      (function () {
        const btn = document.querySelector('button[aria-label="Next match"]')
        if (!btn) throw new Error('next-match button not found')
        btn.click()
      })()
    `)

    // The active match must end up inside the content viewport's VERTICAL bounds.
    await expect
      .poll(
        () =>
          viewer.evaluate<boolean>(`
            (function () {
              const content = document.querySelector('.file-content')
              const mark = document.querySelector('mark.active')
              if (!content || !mark) return false
              const c = content.getBoundingClientRect()
              const m = mark.getBoundingClientRect()
              return m.top >= c.top - 1 && m.bottom <= c.bottom + 1
            })()
          `),
        { timeout: 5000 },
      )
      .toBeTruthy()
  })

  test('does not jump to the line start when the wrapped match is already visible', async () => {
    // The bug: pressing Enter on a match that's already on-screen rough-scrolled
    // to the top of the (wrapped) line, flinging the view to the line start.
    await ensureWrap(viewer, true)

    await viewer.keyboard.press('Control+f')
    await viewer.waitForSelector('.search-bar', 5000)
    await viewer.fill('.search-input', TOKEN_LONG)
    await expect
      .poll(async () => ((await viewer.textContent('.match-count')) ?? '').includes('1 of 1'), { timeout: 5000 })
      .toBeTruthy()

    const clickNext = () => viewer.evaluate(`document.querySelector('button[aria-label="Next match"]').click()`)
    const scrollTop = () => viewer.evaluate<number>(`document.querySelector('.file-content').scrollTop`)

    // First jump brings the match (far down the wrapped line) into view.
    await clickNext()
    await expect.poll(scrollTop, { timeout: 5000 }).toBeGreaterThan(50)
    const settled = await scrollTop()

    // Press Next again on the same, now-visible match and capture scrollTop
    // SYNCHRONOUSLY in the same evaluate as the click, before the async recenter
    // runs. The bug was a synchronous rough-scroll to the line top here, which
    // this reading catches deterministically (a frame-paced poll from the test
    // side would miss the transient once the recenter restores it).
    const scrollTopRightAfterClick = await viewer.evaluate<number>(`
      (function () {
        const content = document.querySelector('.file-content')
        document.querySelector('button[aria-label="Next match"]').click()
        return content.scrollTop
      })()
    `)
    expect(scrollTopRightAfterClick).toBeGreaterThan(settled - 20)

    // And the match stays in view after everything settles.
    await expect
      .poll(
        () =>
          viewer.evaluate<boolean>(`
            (function () {
              const content = document.querySelector('.file-content')
              const mark = document.querySelector('mark.active')
              if (!content || !mark) return false
              const c = content.getBoundingClientRect()
              const m = mark.getBoundingClientRect()
              return m.top >= c.top - 1 && m.bottom <= c.bottom + 1
            })()
          `),
        { timeout: 5000 },
      )
      .toBeTruthy()
  })

  test('jumps back up to a match on an off-screen wrapped line', async () => {
    // The reported bug: with two matches (one far down the wrapped first line, one
    // on a tail line below the filler), navigating to the tail and then BACK to the
    // long-line match flung the view to the top with the match off-screen, because
    // the long line was no longer rendered when we jumped to it.
    await ensureWrap(viewer, true)

    await viewer.keyboard.press('Control+f')
    await viewer.waitForSelector('.search-bar', 5000)
    await viewer.fill('.search-input', PREFIX)
    await expect
      .poll(async () => ((await viewer.textContent('.match-count')) ?? '').includes('1 of 2'), { timeout: 5000 })
      .toBeTruthy()

    const clickNext = () => viewer.evaluate(`document.querySelector('button[aria-label="Next match"]').click()`)

    // Match 0 is the long-line hit. Next → match 1 (tail line, far below), which
    // scrolls the long first line off the top.
    await clickNext()
    await expect
      .poll(() => viewer.evaluate<number>(`document.querySelector('.file-content').scrollTop`), {
        timeout: 5000,
      })
      .toBeGreaterThan(50)

    // Next again wraps back to match 0 on the now-off-screen long line. It must
    // scroll that match into the vertical viewport, not leave the view stranded.
    await clickNext()
    await expect
      .poll(
        () =>
          viewer.evaluate<boolean>(`
            (function () {
              const content = document.querySelector('.file-content')
              const mark = document.querySelector('mark.active')
              if (!content || !mark) return false
              const c = content.getBoundingClientRect()
              const m = mark.getBoundingClientRect()
              return m.top >= c.top - 1 && m.bottom <= c.bottom + 1
            })()
          `),
        { timeout: 5000 },
      )
      .toBeTruthy()
  })
})
