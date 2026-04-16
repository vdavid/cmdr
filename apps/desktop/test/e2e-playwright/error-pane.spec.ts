/**
 * E2E tests for the ErrorPane component.
 *
 * Uses error injection via the `inject_listing_error` Tauri command
 * (feature-gated behind `playwright-e2e`) to trigger specific OS errors
 * and verify the friendly error pane renders correctly.
 *
 * The injected error is cleared after one use, so retries succeed naturally.
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, pollUntil, sleep, getFixtureRoot, moveCursorToFile } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

// Recreate fixtures before each test so previous test suites (e.g. conflict tests)
// don't leave the fixture directory in a non-standard layout.
test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Injects a listing error into the root volume via the Tauri command. */
async function injectListingError(tauriPage: PageLike, errorCode: number): Promise<void> {
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('inject_listing_error', { volumeId: 'root', errorCode: ${String(errorCode)} })`,
  )
}

/** Navigates the focused pane into sub-dir to trigger a new listing (and thus the injected error). */
async function navigateIntoSubDir(tauriPage: PageLike): Promise<void> {
  const moved = await moveCursorToFile(tauriPage, 'sub-dir')
  expect(moved).toBe(true)
  await tauriPage.keyboard.press('Enter')
  await sleep(500)
}

/** Navigates the focused pane back to the fixture root's left/ directory. */
async function navigateBackToLeft(tauriPage: PageLike): Promise<void> {
  const fixtureRoot = getFixtureRoot()
  await tauriPage.evaluate(`(function() {
        window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'left', path: ${JSON.stringify(fixtureRoot + '/left')} }
        });
    })()`)
  await sleep(500)
  await tauriPage.waitForSelector('.file-entry', 5000)
}

// ── Tests ───────────────────────────────────────────────────────────────────

test.describe('Error pane: Transient errors (ETIMEDOUT)', () => {
  test('shows friendly error pane with correct title and retry button', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject ETIMEDOUT (errno 60) and navigate into sub-dir to trigger it
    await injectListingError(tauriPage, 60)
    await navigateIntoSubDir(tauriPage)

    // Wait for the error pane to appear
    const errorPaneVisible = await pollUntil(
      tauriPage,
      async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`),
      5000,
    )
    expect(errorPaneVisible).toBe(true)

    // Verify the title says "Connection timed out"
    const title = await tauriPage.evaluate<string>(`document.querySelector('.error-pane h2')?.textContent || ''`)
    expect(title).toBe('Connection timed out')

    // Verify explanation is rendered as HTML (contains a <p> or text node, not raw markdown)
    const explanationHtml = await tauriPage.evaluate<string>(
      `document.querySelector('.error-pane .explanation')?.innerHTML || ''`,
    )
    // Should contain rendered HTML, not raw markdown asterisks
    expect(explanationHtml).not.toContain('**')
    expect(explanationHtml.length).toBeGreaterThan(0)

    // Verify "Try again" button is visible (Transient category)
    const retryButtonVisible = await tauriPage.evaluate<boolean>(`(function() {
            var buttons = document.querySelectorAll('.error-pane button');
            return Array.from(buttons).some(function(b) {
                return b.textContent.trim() === 'Try again';
            });
        })()`)
    expect(retryButtonVisible).toBe(true)

    // Verify collapsible "Technical details" section exists
    const technicalDetailsExists = await tauriPage.evaluate<boolean>(
      `!!document.querySelector('.error-pane .technical-details summary')`,
    )
    expect(technicalDetailsExists).toBe(true)

    // Clean up: navigate back to the fixture directory
    await navigateBackToLeft(tauriPage)
  })

  test('retry loads the directory successfully after injected error clears', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject ETIMEDOUT and trigger the error
    await injectListingError(tauriPage, 60)
    await navigateIntoSubDir(tauriPage)

    // Wait for error pane
    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 5000)

    // Click "Try again" — the injected error was cleared after first use,
    // so this retry should succeed and show the directory contents
    await tauriPage.evaluate(`(function() {
            var buttons = document.querySelectorAll('.error-pane button');
            for (var i = 0; i < buttons.length; i++) {
                if (buttons[i].textContent.trim() === 'Try again') {
                    buttons[i].click();
                    return;
                }
            }
        })()`)
    await sleep(500)

    // The error pane should disappear and file entries should appear
    const recovered = await pollUntil(
      tauriPage,
      async () => {
        const hasErrorPane = await tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`)
        const hasEntries = await tauriPage.evaluate<boolean>(
          `document.querySelectorAll('.file-pane.is-focused .file-entry').length > 0`,
        )
        return !hasErrorPane && hasEntries
      },
      5000,
    )
    expect(recovered).toBe(true)

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})

test.describe('Error pane: NeedsAction errors (EACCES)', () => {
  test('shows permission error without retry button', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject EACCES (errno 13) and trigger the error
    await injectListingError(tauriPage, 13)
    await navigateIntoSubDir(tauriPage)

    // Wait for the error pane to appear
    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 5000)

    // Verify the title says "No permission"
    const title = await tauriPage.evaluate<string>(`document.querySelector('.error-pane h2')?.textContent || ''`)
    expect(title).toBe('No permission')

    // Verify NO "Try again" button for NeedsAction category
    const retryButtonVisible = await tauriPage.evaluate<boolean>(`(function() {
            var buttons = document.querySelectorAll('.error-pane button');
            return Array.from(buttons).some(function(b) {
                return b.textContent.trim() === 'Try again';
            });
        })()`)
    expect(retryButtonVisible).toBe(false)

    // Verify permission-specific suggestion text is present
    const suggestionHtml = await tauriPage.evaluate<string>(
      `document.querySelector('.error-pane .suggestion')?.innerHTML || ''`,
    )
    expect(suggestionHtml).toContain('permission')

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})

test.describe('Error pane: Accessibility', () => {
  test('has role="alert" and proper heading hierarchy', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject an error to show the error pane
    await injectListingError(tauriPage, 60)
    await navigateIntoSubDir(tauriPage)

    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 5000)

    // Verify role="alert" on the error pane
    const hasAlertRole = await tauriPage.evaluate<boolean>(
      `document.querySelector('.error-pane')?.getAttribute('role') === 'alert'`,
    )
    expect(hasAlertRole).toBe(true)

    // Verify the title is an <h2> element
    const titleTagName = await tauriPage.evaluate<string>(`document.querySelector('.error-pane h2')?.tagName || ''`)
    expect(titleTagName).toBe('H2')

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})
