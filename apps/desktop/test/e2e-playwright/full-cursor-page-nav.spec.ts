/**
 * E2E regression test for Full view sticky-header cursor visibility.
 *
 * Repro: in Full view of a folder with enough entries to scroll past two pages,
 * press PageDown twice then PageUp twice. The cursor lands back on ".." (index
 * 0). Before the fix, scrollTop ended at exactly `headerHeight`, which placed
 * row 0 fully under the sticky column header — invisible, cursor included.
 *
 * The fix removes a spurious `+ headerHeight` translation in `FullList.svelte`:
 * the spacer's scroll offset is `scrollTop` directly, not `scrollTop -
 * headerHeight` clamped at 0. This test pins both the visual behavior and the
 * underlying scrollTop so the regression can't sneak back in.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { ensureAppReady, ensureExplorerFocused, executeViaCommandPalette, getFixtureRoot } from './helpers.js'

/** Subdir under the shared fixture root holding the files for this suite. */
const FIXTURE_SUBDIR = 'full-page-nav-fixtures'

const FULL_LIST_SCROLL = '.file-pane.is-focused .full-list-container .full-list'
const FULL_LIST_HEADER = '.file-pane.is-focused .full-list-container .header-row'
const CURSOR_ENTRY = '.file-pane.is-focused .full-list-container .file-entry.is-under-cursor'

/** Sub-pixel rounding tolerance for getBoundingClientRect comparisons. */
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
 * Builds a 120-file fixture. 120 is enough to scroll past two PageDown jumps
 * at any sensible window height, and small enough that `recreateFixtures()`
 * elsewhere doesn't recreate it.
 */
function ensureFullPageNavFixture(): string {
  const fixtureRoot = getFixtureRoot()
  const dir = path.join(fixtureRoot, FIXTURE_SUBDIR)
  if (fs.existsSync(dir) && fs.readdirSync(dir).length >= 120) {
    return dir
  }
  fs.rmSync(dir, { recursive: true, force: true })
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < 120; i++) {
    fs.writeFileSync(path.join(dir, `file-${String(i).padStart(3, '0')}.txt`), 'x')
  }
  return dir
}

/** Disable CSS transitions so width/scroll animations don't race the reads. */
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

/** Navigate the focused pane to a path via the mcp-nav-to-path Tauri event. */
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

/** Read three rects + the scroll container's scrollTop in one round-trip. */
async function readState(tauriPage: Parameters<typeof ensureAppReady>[0]): Promise<{
  cursor: Rect | null
  header: Rect | null
  scrollContainer: Rect | null
  scrollTop: number
  cursorName: string
}> {
  return tauriPage.evaluate<{
    cursor: Rect | null
    header: Rect | null
    scrollContainer: Rect | null
    scrollTop: number
    cursorName: string
  }>(`(function () {
        function toRect(el) {
            if (!el) return null;
            var r = el.getBoundingClientRect();
            return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
        }
        var cursor = document.querySelector(${JSON.stringify(CURSOR_ENTRY)});
        var header = document.querySelector(${JSON.stringify(FULL_LIST_HEADER)});
        var scroll = document.querySelector(${JSON.stringify(FULL_LIST_SCROLL)});
        return {
            cursor: toRect(cursor),
            header: toRect(header),
            scrollContainer: toRect(scroll),
            scrollTop: scroll ? scroll.scrollTop : -1,
            cursorName: cursor ? (cursor.getAttribute('data-filename') || '') : '',
        };
    })()`)
}

/** Read the current cursor row's filename. */
async function getCursorName(tauriPage: Parameters<typeof ensureAppReady>[0]): Promise<string> {
  return tauriPage.evaluate<string>(
    `(function () { var e = document.querySelector(${JSON.stringify(CURSOR_ENTRY)}); return e ? (e.getAttribute('data-filename') || '') : ''; })()`,
  )
}

/**
 * Press a key on the focused pane and wait until the cursor row's filename
 * actually changes — proves the keystroke was processed.
 */
async function pressAndWaitCursorChange(
  tauriPage: Parameters<typeof ensureAppReady>[0],
  key: string,
  expectedName?: string,
): Promise<void> {
  const before = await getCursorName(tauriPage)
  await tauriPage.keyboard.press(key)
  await expect
    .poll(
      async () => {
        const name = await getCursorName(tauriPage)
        if (expectedName !== undefined) return name === expectedName
        return name !== before
      },
      { timeout: 5000 },
    )
    .toBeTruthy()
}

test.describe('Full view sticky-header cursor visibility', () => {
  test.beforeAll(() => {
    ensureFullPageNavFixture()
  })

  test('cursor on ".." stays visible after PageDown ×2, PageUp ×2', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await disableTransitions(tauriPage)

    const fixtureDir = path.join(getFixtureRoot(), FIXTURE_SUBDIR)
    await navigateFocusedPaneTo(tauriPage, 'left', fixtureDir)
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `!!document.querySelector('.file-pane.is-focused [data-filename="file-000.txt"]')`,
          ),
        { timeout: 5000 },
      )
      .toBeTruthy()

    // Switch to Full view via command palette.
    await executeViaCommandPalette(tauriPage, 'Full view')
    await expect.poll(async () => tauriPage.isVisible(FULL_LIST_HEADER), { timeout: 5000 }).toBeTruthy()

    // Cursor starts on "..". Confirm before running the repro.
    await expect.poll(async () => (await getCursorName(tauriPage)) === '..', { timeout: 3000 }).toBeTruthy()

    // Reclaim keyboard focus before the OS key presses. The preceding
    // `mcp-nav-to-path` and the command-palette open/close drift
    // `document.activeElement` to `<body>`, where PageDown never reaches the
    // explorer's container-level keydown handler (the cursor would silently
    // never move). See the `ensureAppReady` focus contract in DETAILS.md.
    await ensureExplorerFocused(tauriPage)

    // PageDown × 2 — cursor walks two pages down into the listing.
    await pressAndWaitCursorChange(tauriPage, 'PageDown')
    await pressAndWaitCursorChange(tauriPage, 'PageDown')

    // PageUp × 2 — cursor comes back to "..".
    await pressAndWaitCursorChange(tauriPage, 'PageUp')
    await pressAndWaitCursorChange(tauriPage, 'PageUp', '..')

    // Sample state. The interesting reads:
    //  - cursor row exists and is on "..",
    //  - cursor row's top is at or below the header's bottom (not hidden under it),
    //  - cursor row's bottom is at or above the scroll container's bottom (still in viewport),
    //  - scrollTop is 0 (canonical "top of list" state).
    const state = await readState(tauriPage)

    expect(state.cursor, 'cursor row should be rendered').not.toBeNull()
    expect(state.header, 'header row should be rendered').not.toBeNull()
    expect(state.scrollContainer, 'scroll container should be rendered').not.toBeNull()
    if (!state.cursor || !state.header || !state.scrollContainer) return

    expect(state.cursorName, 'cursor should be back on ".."').toBe('..')

    // Primary assertion: the cursor row top sits at or below the header's bottom.
    // Before the fix, cursor.top was ~0 px above the viewport (under the sticky header).
    expect(
      state.cursor.top,
      `cursor.top (${String(state.cursor.top)}) must be >= header.bottom (${String(state.header.bottom)}) — row is hidden under the sticky header`,
    ).toBeGreaterThanOrEqual(state.header.bottom - PIXEL_TOLERANCE)

    // The cursor row must also be inside the scroll container's viewport.
    expect(
      state.cursor.bottom,
      `cursor.bottom (${String(state.cursor.bottom)}) must be <= scrollContainer.bottom (${String(state.scrollContainer.bottom)})`,
    ).toBeLessThanOrEqual(state.scrollContainer.bottom + PIXEL_TOLERANCE)

    // Underlying invariant: at the top of the list, scrollTop is exactly 0.
    // Before the fix this was `headerHeight` (~22 px at scale 1), which is what
    // covered row 0. Pinning it at 0 documents the canonical state and gives
    // a clean failure message if the math regresses.
    expect(state.scrollTop, `scrollTop must be 0 at top of list, got ${String(state.scrollTop)}`).toBe(0)
  })
})
