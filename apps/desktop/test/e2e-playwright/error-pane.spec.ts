/**
 * E2E tests for the ErrorPane component.
 *
 * Uses error injection via the `inject_listing_error` Tauri command
 * (feature-gated behind `playwright-e2e`) to trigger specific OS errors
 * and verify the friendly error pane renders correctly.
 *
 * The injected error is cleared after one use, so retries succeed naturally.
 */

import os from 'os'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, pollUntil, sleep, getFixtureRoot, moveCursorToFile } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const IS_LINUX = os.platform() === 'linux'

// On macOS, friendly_error_from_errno maps specific errnos to specific titles.
// On Linux, the fallback maps all errnos to the generic "Couldn't read this folder".
const ETIMEDOUT_TITLE = IS_LINUX ? "Couldn't read this folder" : 'Connection timed out'
const EACCES_TITLE = IS_LINUX ? "Couldn't read this folder" : 'No permission'

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

/**
 * Injects a listing error and immediately navigates into sub-dir.
 *
 * The inject + navigate must be atomic (no sleep between) because on Linux,
 * background listings (watcher re-reads, focus-change reloads) can consume
 * the single-shot injected error before the intended navigation fires.
 */
async function injectAndNavigateIntoSubDir(tauriPage: PageLike, errorCode: number): Promise<void> {
  const fixtureRoot = getFixtureRoot()
  const subDirPath = fixtureRoot + '/left/sub-dir'

  // Inject the error, then navigate via IPC (not keyboard Enter).
  // Keyboard Enter goes through ensureAppReady's click handler chain which can
  // race with background listings on Linux. Direct IPC navigation is deterministic.
  await injectListingError(tauriPage, errorCode)
  await tauriPage.evaluate(`(function() {
        window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: 'left', path: ${JSON.stringify(subDirPath)} }
        });
    })()`)
  await sleep(2000)
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

    await injectAndNavigateIntoSubDir(tauriPage, 60)

    // Wait for the error pane to appear
    const errorPaneVisible = await pollUntil(
      tauriPage,
      async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`),
      15000,
    )
    expect(errorPaneVisible).toBe(true)

    // Verify the title says "Connection timed out"
    const title = await tauriPage.evaluate<string>(
      `(document.querySelector('.error-pane h2')?.textContent || '').trim()`,
    )
    expect(title).toBe(ETIMEDOUT_TITLE)

    // Verify explanation is rendered as HTML (contains a <p> or text node, not raw markdown)
    const explanationHtml = await tauriPage.evaluate<string>(
      `document.querySelector('.error-pane .explanation')?.innerHTML || ''`,
    )
    // Should contain rendered HTML, not raw markdown asterisks
    expect(explanationHtml).not.toContain('**')
    expect(explanationHtml.length).toBeGreaterThan(0)

    // On macOS, ETIMEDOUT maps to Transient category (retry button visible).
    // On Linux, the fallback maps all errnos to Serious category (no retry button,
    // because retry requires category === 'transient').
    const retryButtonVisible = await tauriPage.evaluate<boolean>(`(function() {
            var buttons = document.querySelectorAll('.error-pane button');
            return Array.from(buttons).some(function(b) {
                return b.textContent.trim() === 'Try again';
            });
        })()`)
    expect(retryButtonVisible).toBe(!IS_LINUX)

    // Verify collapsible "Technical details" section exists
    const technicalDetailsExists = await tauriPage.evaluate<boolean>(
      `!!document.querySelector('.error-pane .technical-details summary')`,
    )
    expect(technicalDetailsExists).toBe(true)

    // Clean up: navigate back to the fixture directory
    await navigateBackToLeft(tauriPage)
  })

  // On Linux, the error fallback maps to Serious category which doesn't show the retry button.
  // eslint-disable-next-line @typescript-eslint/unbound-method -- conditional skip
  const retryTest = IS_LINUX ? test.skip : test
  retryTest('retry loads the directory successfully after injected error clears', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject ETIMEDOUT and trigger the error
    await injectAndNavigateIntoSubDir(tauriPage, 60)

    // Wait for error pane
    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 15000)

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
    await injectAndNavigateIntoSubDir(tauriPage, 13)

    // Wait for the error pane to appear
    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 15000)

    // Verify the title says "No permission"
    const title = await tauriPage.evaluate<string>(
      `(document.querySelector('.error-pane h2')?.textContent || '').trim()`,
    )
    expect(title).toBe(EACCES_TITLE)

    // On macOS, EACCES maps to NeedsAction category (no retry button, permission-specific suggestion).
    // On Linux, the fallback maps all errnos to Serious category (with retry button, generic suggestion).
    const retryButtonVisible = await tauriPage.evaluate<boolean>(`(function() {
            var buttons = document.querySelectorAll('.error-pane button');
            return Array.from(buttons).some(function(b) {
                return b.textContent.trim() === 'Try again';
            });
        })()`)
    expect(retryButtonVisible).toBe(false) // No retry: macOS=NeedsAction, Linux=Serious (retry requires 'transient')

    const suggestionHtml = await tauriPage.evaluate<string>(
      `document.querySelector('.error-pane .suggestion')?.innerHTML || ''`,
    )
    if (!IS_LINUX) {
      expect(suggestionHtml).toContain('permission')
    }

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})

test.describe('Error pane: Accessibility', () => {
  test('has role="alert" and proper heading hierarchy', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject an error to show the error pane
    await injectAndNavigateIntoSubDir(tauriPage, 60)

    await pollUntil(tauriPage, async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), 15000)

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
