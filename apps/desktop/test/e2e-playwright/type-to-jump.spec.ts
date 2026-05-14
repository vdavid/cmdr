/**
 * E2E tests for the type-to-jump in-directory navigation feature.
 *
 * Covers the golden paths from the plan (§ Testing > Integration):
 * - Typing letters lands the cursor on the best fuzzy match.
 * - The indicator chip surfaces the live buffer ("Jump: fil").
 * - ESC clears the buffer + indicator.
 * - Cmd/Ctrl-modified shortcuts skip the buffer (Cmd+T opens a new tab).
 * - Switching pane clears the buffer on the previous pane.
 *
 * Timing-sensitive scenarios (stale-state transition at 1 s, indicator hide
 * at 5 s) are intentionally left out. They'd require real-time waits that
 * are flaky under parallel-shard load. The unit tests
 * (`type-to-jump-state.svelte.test.ts`) cover those transitions deterministically
 * with fake timers.
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, getFixtureRoot, pollUntil } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const INDICATOR = '.type-to-jump-indicator'

// Earlier suites (file-operations.spec.ts) mutate `left/` (create, rename, move,
// delete). Without this, type-to-jump's `ensureAppReady` fails because the
// expected `file-a.txt` / `sub-dir` entries are gone.
test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

/**
 * Types a sequence of characters into the focused pane. Dispatches DOM
 * `KeyboardEvent`s on `document.activeElement` (matching the production
 * keyboard handler's listening site). Each char goes through the same code
 * path the user's keypresses do, with no shortcuts.
 */
async function typeChars(tauriPage: PageLike, chars: string): Promise<void> {
  for (const char of chars) {
    const k = JSON.stringify(char)
    await tauriPage.evaluate(`(function(){
      var el = document.activeElement || document.body;
      var o = { key: ${k}, bubbles: true, cancelable: true };
      el.dispatchEvent(new KeyboardEvent('keydown', o));
      el.dispatchEvent(new KeyboardEvent('keypress', o));
      el.dispatchEvent(new KeyboardEvent('keyup', o));
    })()`)
  }
}

/** Reads the data-filename of the focused pane's cursor entry. */
async function cursorName(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function(){
    var pane = document.querySelector('.file-pane.is-focused');
    if (!pane) return '';
    var entry = pane.querySelector('.file-entry.is-under-cursor');
    if (!entry) return '';
    return entry.getAttribute('data-filename') || '';
  })()`)
}

/** Reads the indicator text content (returns empty string if not in DOM). */
async function indicatorText(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function(){
    var el = document.querySelector(${JSON.stringify(INDICATOR)});
    return el ? (el.textContent || '').trim() : '';
  })()`)
}

test.describe('Type-to-jump', () => {
  test('typing letters jumps the cursor to the best fuzzy match', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Type "file": the left pane has `file-a.txt`, `file-b.txt`, plus
    // directories `sub-dir/` and `bulk/`. The top-scoring fuzzy match for
    // "file" should be one of the `file-*.txt` entries.
    await typeChars(tauriPage, 'file')

    // The indicator must surface the live buffer.
    await pollUntil(
      tauriPage,
      async () => {
        const text = await indicatorText(tauriPage)
        return text.includes('Jump:') && text.includes('file')
      },
      3000,
    )

    // The cursor must have landed on a file-* entry.
    await pollUntil(
      tauriPage,
      async () => {
        const name = await cursorName(tauriPage)
        return name.startsWith('file-')
      },
      3000,
    )
  })

  test('ESC clears the buffer and hides the indicator', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await typeChars(tauriPage, 'fi')

    // Wait for the indicator to appear first.
    await tauriPage.waitForSelector(INDICATOR, 3000)

    // ESC should clear the buffer + indicator. The dispatcher routes ESC to
    // `clearJumpState()` before falling through to other handlers.
    await tauriPage.keyboard.press('Escape')
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible(INDICATOR)), 3000)
  })

  test('Cmd/Ctrl-modified keys do not feed the buffer', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Dispatch a DOM keydown carrying the modifier flag so the explorer's
    // `isTypeToJumpChar` returns false on it. We use the DOM path (not
    // TauriKeyboard) to stay in lockstep with `typeChars` and avoid races
    // around native menu accelerators triggering the new-tab handler before
    // the test can read the indicator state.
    const modKey = process.platform === 'darwin' ? 'metaKey' : 'ctrlKey'
    await tauriPage.evaluate(`(function(){
      var el = document.activeElement || document.body;
      var o = { key: 't', bubbles: true, cancelable: true, ${modKey}: true };
      el.dispatchEvent(new KeyboardEvent('keydown', o));
      el.dispatchEvent(new KeyboardEvent('keyup', o));
    })()`)

    // Give the dispatcher a tick. If the modifier skip worked, the indicator
    // must NOT be in the DOM. Quick poll catches the rare case where the
    // keystroke does fire the indicator before being cleared.
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible(INDICATOR)), 1000)
    expect(await tauriPage.isVisible(INDICATOR)).toBe(false)
  })

  test('switching pane clears the previous pane indicator', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await typeChars(tauriPage, 'f')
    await tauriPage.waitForSelector(INDICATOR, 3000)

    // Switch pane with Tab. The keydown intercept treats Tab as a reset key
    // and clears the active-pane buffer before the pane swap fires.
    await tauriPage.keyboard.press('Tab')

    // Once the swap has settled, no indicator should be visible anywhere.
    await pollUntil(
      tauriPage,
      async () => {
        const count = await tauriPage.evaluate<number>(`document.querySelectorAll(${JSON.stringify(INDICATOR)}).length`)
        return count === 0
      },
      3000,
    )
  })
})
