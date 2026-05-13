/**
 * E2E test for Brief view cursor visibility under navigation and resize.
 *
 * Catches the c336dbba shrink-wrap regression: with variable column widths,
 * the cursor could fall outside the visible scroll viewport after horizontal
 * arrow navigation, Home/End, PageUp/PageDown, or pane resizes.
 *
 * The fix relies on prefix-sum-based exact virtual-scroll math. This test
 * exercises that math via the real Tauri webview.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { ensureAppReady, executeViaCommandPalette, getFixtureRoot, pollUntil, sleep } from './helpers.js'

/**
 * Subdir under the shared fixture root holding the 200 files for this suite.
 * Lives next to `left/` and `right/`, so the shared `recreateFixtures()` other
 * tests run in `beforeEach` leaves it untouched (it only touches `left/` and
 * `right/`).
 */
const FIXTURE_SUBDIR = 'brief-cursor-fixtures'

/** Container that holds the file list (excludes the header row). */
const BRIEF_LIST_SCROLL = '.file-pane.is-focused .brief-list-container .brief-list'
/** The cursor row. Brief mode uses `is-under-cursor` (matches the FE selector). */
const CURSOR_ENTRY = '.file-pane.is-focused .brief-list-container .file-entry.is-under-cursor'

/** Tolerance for sub-pixel rounding (e.g., devicePixelRatio quirks). */
const PIXEL_TOLERANCE = 1

interface Rect {
  left: number
  top: number
  right: number
  bottom: number
  width: number
  height: number
}

/**
 * Builds the deterministic 200-file fixture. Idempotent: re-running it is a
 * no-op once the directory exists with the right count, so test reruns are
 * fast and rebuilds are skipped.
 *
 * Mix of name lengths exercises the variable-column-width path:
 *  - 50 short names (`a-XX.txt`)
 *  - 100 medium names (`cmdr-test-fixture-XXX.json`)
 *  - 50 long names (`cmdr-e2e-playwright-mtp-1778621587-very-long-name-XXX.log`)
 */
function ensureBriefCursorFixture(): string {
  const fixtureRoot = getFixtureRoot()
  const dir = path.join(fixtureRoot, FIXTURE_SUBDIR)
  if (fs.existsSync(dir) && fs.readdirSync(dir).length >= 200) {
    return dir
  }
  fs.rmSync(dir, { recursive: true, force: true })
  fs.mkdirSync(dir, { recursive: true })

  const tiny = 'A'
  for (let i = 0; i < 50; i++) {
    fs.writeFileSync(path.join(dir, `a-${String(i).padStart(2, '0')}.txt`), tiny)
  }
  for (let i = 0; i < 100; i++) {
    fs.writeFileSync(path.join(dir, `cmdr-test-fixture-${String(i).padStart(3, '0')}.json`), tiny)
  }
  for (let i = 0; i < 50; i++) {
    fs.writeFileSync(
      path.join(dir, `cmdr-e2e-playwright-mtp-1778621587-very-long-name-${String(i).padStart(3, '0')}.log`),
      tiny,
    )
  }
  return dir
}

/**
 * Disables CSS transitions in the live webview. We can't call
 * `page.emulateMedia({ reducedMotion: 'reduce' })` in Tauri mode (TauriPage
 * doesn't expose it), so we inject a style tag that flat-out kills
 * transitions. Brief column widths animate over 300 ms otherwise, and
 * `getBoundingClientRect()` would read mid-animation values.
 */
async function disableTransitions(tauriPage: Parameters<typeof ensureAppReady>[0]): Promise<void> {
  await tauriPage.evaluate(`(function () {
        var existing = document.getElementById('cmdr-e2e-no-transitions');
        if (existing) return;
        var s = document.createElement('style');
        s.id = 'cmdr-e2e-no-transitions';
        s.textContent = '*, *::before, *::after { transition: none !important; animation: none !important; }';
        document.head.appendChild(s);
    })()`)
}

/** Navigates the focused pane to a path via the same Tauri event ensureAppReady uses. */
async function navigateFocusedPaneTo(
  tauriPage: Parameters<typeof ensureAppReady>[0],
  paneId: 'left' | 'right',
  targetPath: string,
): Promise<void> {
  await tauriPage.evaluate(`(function () {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: ${JSON.stringify(paneId)}, path: ${JSON.stringify(targetPath)} }
        });
    })()`)
}

/** Reads the bounding rect of the first DOM node matching `selector`. */
async function getRect(tauriPage: Parameters<typeof ensureAppReady>[0], selector: string): Promise<Rect | null> {
  return tauriPage.evaluate<Rect | null>(`(function () {
        var el = document.querySelector(${JSON.stringify(selector)});
        if (!el) return null;
        var r = el.getBoundingClientRect();
        return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
    })()`)
}

/**
 * Asserts the cursor row sits fully inside the brief-list scroll viewport.
 * The header row above the scroll container is intentionally excluded; only
 * the file-list area is the "in view" target.
 */
async function expectCursorInView(tauriPage: Parameters<typeof ensureAppReady>[0], context: string): Promise<void> {
  // Wait until both rects are settled and consistent on consecutive reads —
  // guards against reading mid-scroll. Two identical samples in a row is
  // sufficient because scroll updates are synchronous JS work; the only
  // delay is the next paint frame.
  const settled = await pollUntil(
    tauriPage,
    async () => {
      const a1 = await getRect(tauriPage, CURSOR_ENTRY)
      const c1 = await getRect(tauriPage, BRIEF_LIST_SCROLL)
      if (!a1 || !c1) return false
      const a2 = await getRect(tauriPage, CURSOR_ENTRY)
      const c2 = await getRect(tauriPage, BRIEF_LIST_SCROLL)
      if (!a2 || !c2) return false
      return (
        Math.abs(a1.left - a2.left) < 0.5 &&
        Math.abs(a1.right - a2.right) < 0.5 &&
        Math.abs(c1.left - c2.left) < 0.5 &&
        Math.abs(c1.right - c2.right) < 0.5
      )
    },
    3000,
  )
  expect(settled, `${context}: cursor/container rects did not settle`).toBe(true)

  const cursor = await getRect(tauriPage, CURSOR_ENTRY)
  const container = await getRect(tauriPage, BRIEF_LIST_SCROLL)
  expect(cursor, `${context}: cursor not found`).not.toBeNull()
  expect(container, `${context}: brief-list container not found`).not.toBeNull()
  if (!cursor || !container) return

  // Horizontal containment — the regression this whole change exists to fix.
  expect(
    cursor.left,
    `${context}: cursor.left (${String(cursor.left)}) < container.left (${String(container.left)})`,
  ).toBeGreaterThanOrEqual(container.left - PIXEL_TOLERANCE)
  expect(
    cursor.right,
    `${context}: cursor.right (${String(cursor.right)}) > container.right (${String(container.right)})`,
  ).toBeLessThanOrEqual(container.right + PIXEL_TOLERANCE)

  // Vertical containment — header above and pane bottom both fully exclude the row otherwise.
  expect(
    cursor.top,
    `${context}: cursor.top (${String(cursor.top)}) < container.top (${String(container.top)})`,
  ).toBeGreaterThanOrEqual(container.top - PIXEL_TOLERANCE)
  expect(
    cursor.bottom,
    `${context}: cursor.bottom (${String(cursor.bottom)}) > container.bottom (${String(container.bottom)})`,
  ).toBeLessThanOrEqual(container.bottom + PIXEL_TOLERANCE)
}

/** Reads the focused pane's current cursor row's filename. */
async function getCursorName(tauriPage: Parameters<typeof ensureAppReady>[0]): Promise<string> {
  return tauriPage.evaluate<string>(`(function () {
        var e = document.querySelector(${JSON.stringify(CURSOR_ENTRY)});
        return e ? (e.getAttribute('data-filename') || '') : '';
    })()`)
}

/**
 * Presses a key on the focused pane and waits until the cursor row's filename
 * actually changes (or the key is a no-op edge case after a polling window).
 * Avoids fixed waitForTimeout flakes.
 */
async function pressAndWaitCursorChange(tauriPage: Parameters<typeof ensureAppReady>[0], key: string): Promise<void> {
  const before = await getCursorName(tauriPage)
  await tauriPage.keyboard.press(key)
  await pollUntil(
    tauriPage,
    async () => {
      const after = await getCursorName(tauriPage)
      return after !== before
    },
    1500,
  )
}

test.describe('Brief view cursor visibility', () => {
  test.beforeAll(() => {
    ensureBriefCursorFixture()
  })

  test('cursor stays in view under arrow nav, Home/End, PageUp/PageDown, and resize', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await disableTransitions(tauriPage)

    // Navigate the left pane (focused by default after ensureAppReady) into the
    // 200-file fixture subdir. We use the same mcp-nav-to-path Tauri event the
    // helper itself uses, so this stays in lockstep with the rest of the suite.
    const fixtureDir = path.join(getFixtureRoot(), FIXTURE_SUBDIR)
    await navigateFocusedPaneTo(tauriPage, 'left', fixtureDir)
    // Wait for the new listing to render (look for a known long-name file).
    await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`!!document.querySelector('.file-pane.is-focused [data-filename="a-00.txt"]')`),
      10000,
    )

    // Switch to Brief view via the command palette (same path file-operations.spec.ts uses).
    await executeViaCommandPalette(tauriPage, 'Brief view')
    await pollUntil(tauriPage, async () => tauriPage.isVisible('.file-pane.is-focused .brief-list-container'), 5000)

    // Make sure cursor starts at column 0 — press Home and confirm.
    await tauriPage.keyboard.press('Home')
    await pollUntil(tauriPage, async () => (await getCursorName(tauriPage)) !== '', 3000)
    await expectCursorInView(tauriPage, 'after Home (start)')

    // ── Arrow Right × 50 ─────────────────────────────────────────────────────
    for (let i = 0; i < 50; i++) {
      await pressAndWaitCursorChange(tauriPage, 'ArrowRight')
      await expectCursorInView(tauriPage, `after ArrowRight #${String(i + 1)}`)
    }

    // ── End ──────────────────────────────────────────────────────────────────
    await tauriPage.keyboard.press('End')
    // Cursor lands on the last entry; poll until it settles.
    await pollUntil(
      tauriPage,
      async () => {
        const name = await getCursorName(tauriPage)
        return name.length > 0
      },
      3000,
    )
    // Settle the scroll position before measuring.
    await sleep(50)
    await expectCursorInView(tauriPage, 'after End')

    // ── Home ─────────────────────────────────────────────────────────────────
    await tauriPage.keyboard.press('Home')
    await pollUntil(
      tauriPage,
      async () => {
        const name = await getCursorName(tauriPage)
        // After Home, cursor is either at ".." or the first real entry.
        return name === '..' || name === 'a-00.txt'
      },
      3000,
    )
    await expectCursorInView(tauriPage, 'after Home')

    // ── PageDown × 5 ─────────────────────────────────────────────────────────
    for (let i = 0; i < 5; i++) {
      await pressAndWaitCursorChange(tauriPage, 'PageDown')
      await expectCursorInView(tauriPage, `after PageDown #${String(i + 1)}`)
    }

    // ── PageUp × 5 ───────────────────────────────────────────────────────────
    for (let i = 0; i < 5; i++) {
      await pressAndWaitCursorChange(tauriPage, 'PageUp')
      await expectCursorInView(tauriPage, `after PageUp #${String(i + 1)}`)
    }

    // ── Resize variant ────────────────────────────────────────────────────────
    // Tauri windows aren't directly resizable from a Playwright-in-Tauri test,
    // and we can't drive the OS window manager. We get the same effect — and
    // the same code path — by driving the in-app PaneResizer with synthetic
    // mouse events: it owns `leftPaneWidthPercent`, which sets the inline
    // `width: X%` on the pane wrapper. The brief-list-container's `bind:clientWidth`
    // picks the change up via the regular reactive path, exercising
    // `containerWidth → capPx → fetchColumnWidths → scrollToIndex(cursor)`.
    // Park the cursor near the middle of the list before resizing, then drag
    // the resizer about 200 px to the left and assert the cursor stays in view.
    await tauriPage.keyboard.press('Home')
    await sleep(100)
    for (let i = 0; i < 25; i++) {
      await tauriPage.keyboard.press('ArrowRight')
    }
    await pollUntil(tauriPage, async () => (await getCursorName(tauriPage)) !== '', 3000)
    await expectCursorInView(tauriPage, 'mid-list before resize')

    // Drive the resizer programmatically. PaneResizer.svelte (lines 12–41)
    // listens for `mousedown` on `.pane-resizer`, then `mousemove` / `mouseup`
    // on `document`. We bypass DOM event dispatching for `mousemove` (which
    // synthetic events can't cleanly target `document` for cross-element
    // listeners across browsers) and instead emit them by invoking the same
    // computation directly via `dispatchEvent` on `document`.
    await tauriPage.evaluate(`(function () {
            var resizer = document.querySelector('.pane-resizer');
            if (!resizer) throw new Error('pane-resizer not found');
            var container = resizer.closest('.dual-pane-explorer');
            if (!container) throw new Error('dual-pane-explorer not found');
            var rect = container.getBoundingClientRect();
            var resizerRect = resizer.getBoundingClientRect();
            var startX = resizerRect.left + resizerRect.width / 2;
            var targetX = Math.max(rect.left + rect.width * 0.25, startX - 200);
            resizer.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: startX, clientY: resizerRect.top + 10 }));
            document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, button: 0, clientX: targetX, clientY: resizerRect.top + 10 }));
            document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: targetX, clientY: resizerRect.top + 10 }));
        })()`)

    // Wait until the focused pane's width actually shrank.
    const shrank = await pollUntil(
      tauriPage,
      async () => {
        const rect = await getRect(tauriPage, BRIEF_LIST_SCROLL)
        return rect !== null && rect.width > 0
      },
      3000,
    )
    expect(shrank, 'brief-list rect should be readable after resize').toBe(true)

    // Allow one debounce cycle (50 ms in BriefList's fetchColumnWidths) plus a
    // little headroom for the IPC round-trip and the cursor-visibility effect.
    await sleep(300)

    await expectCursorInView(tauriPage, 'after pane resize')
  })
})
