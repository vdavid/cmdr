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
import { ensureAppReady, getFixtureRoot } from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const IS_LINUX = os.platform() === 'linux'

// `listing_error_from_errno` maps specific macOS errnos to specific reasons.
// The non-macOS fallback maps every errno to `CouldntReadUnknown`, hence one title.
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
  // Wait for the error pane to render (the injected error fires during the
  // background listing kicked off by the navigation above). 3 s: the error
  // pane renders in <100 ms on the happy path; longer budgets just hid
  // failures behind the 8 s outer test timeout.
  await expect
    .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
    .toBeTruthy()
}

/**
 * Reads the labels of the error screen's action-row buttons.
 *
 * Labels carry a trailing `ShortcutChip` (`Go to home folder ⌘⇧H`), so callers
 * match with `startsWith` unless the button has no chip (`Try again`).
 */
async function ctaLabels(tauriPage: PageLike): Promise<string[]> {
  return tauriPage.evaluate<string[]>(`(function() {
        var buttons = document.querySelectorAll('.error-pane .cta button');
        return Array.from(buttons).map(function(b) { return (b.textContent || '').trim(); });
    })()`)
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
  // Wait for the error pane (if any) to be gone and file entries to appear,
  // which together prove the navigation landed.
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(
          `!document.querySelector('.error-pane') && document.querySelectorAll('.file-pane.is-focused .file-entry').length > 0`,
        ),
      { timeout: 5000 },
    )
    .toBeTruthy()
}

// ── Tests ───────────────────────────────────────────────────────────────────

test.describe('Error pane: Transient errors (ETIMEDOUT)', () => {
  test('shows friendly error pane with correct title and retry button', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await injectAndNavigateIntoSubDir(tauriPage, 60)

    // Wait for the error pane to appear
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
      .toBeTruthy()

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

    // `Try again` keys on `retryHint` alone, and both classifications set it:
    // macOS reads ETIMEDOUT as Transient, the Linux fallback as Serious. So the
    // button is there on both platforms, and the action row always carries the
    // always-available way out next to it.
    const labels = await ctaLabels(tauriPage)
    expect(labels).toContain('Try again')
    expect(labels.some((label) => label.startsWith('Go to home folder'))).toBe(true)

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
    await injectAndNavigateIntoSubDir(tauriPage, 60)

    // Wait for error pane
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
      .toBeTruthy()

    // Click "Try again": the injected error was cleared after first use,
    // so this retry should succeed and show the directory contents. Retry the
    // click via expect.poll so a NodeList that's transiently empty (button not
    // rendered yet) doesn't silently no-op.
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`(function() {
          var buttons = document.querySelectorAll('.error-pane button');
          for (var i = 0; i < buttons.length; i++) {
            if ((buttons[i].textContent || '').trim() === 'Try again') {
              buttons[i].click();
              return true;
            }
          }
          return false;
        })()`),
        { timeout: 2000 },
      )
      .toBeTruthy()

    // The error pane should disappear and file entries should appear
    await expect
      .poll(
        async () => {
          const hasErrorPane = await tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`)
          const hasEntries = await tauriPage.evaluate<boolean>(
            `document.querySelectorAll('.file-pane.is-focused .file-entry').length > 0`,
          )
          return !hasErrorPane && hasEntries
        },
        { timeout: 5000 },
      )
      .toBeTruthy()

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})

test.describe('Error pane: NeedsAction errors (EACCES)', () => {
  test('shows a permission error whose way out matches what the platform knows', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject EACCES (errno 13) and trigger the error
    await injectAndNavigateIntoSubDir(tauriPage, 13)

    // Wait for the error pane to appear
    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
      .toBeTruthy()

    // Verify the title says "No permission"
    const title = await tauriPage.evaluate<string>(
      `(document.querySelector('.error-pane h2')?.textContent || '').trim()`,
    )
    expect(title).toBe(EACCES_TITLE)

    // The two platforms classify EACCES differently, so they earn different ways out,
    // and each one has to be the RIGHT way out:
    //   macOS recognizes it as a permission problem (`NoPermissionErrno`, NeedsAction,
    //     `retryHint: false`, `actionKind: open_privacy_settings`), so retrying can't
    //     help and the screen sends the user to System Settings instead.
    //   Linux has no errno mapping yet, so it falls back to `CouldntReadUnknown`
    //     (Serious, `retryHint: true`): the app doesn't know retrying is futile, and
    //     offering it beats stranding the user.
    const labels = await ctaLabels(tauriPage)
    expect(labels.includes('Try again')).toBe(IS_LINUX)

    const suggestionHtml = await tauriPage.evaluate<string>(
      `document.querySelector('.error-pane .suggestion')?.innerHTML || ''`,
    )
    if (IS_LINUX) {
      expect(labels.some((label) => label.startsWith('Open '))).toBe(false)
    } else {
      expect(labels.some((label) => label.startsWith('Open '))).toBe(true)
      expect(suggestionHtml).toContain('permission')
    }

    // Whichever branch ran, the screen is never a dead end.
    expect(labels.some((label) => label.startsWith('Go to home folder'))).toBe(true)

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})

test.describe('Error pane: Accessibility', () => {
  test('has role="alert" and proper heading hierarchy', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Inject an error to show the error pane
    await injectAndNavigateIntoSubDir(tauriPage, 60)

    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
      .toBeTruthy()

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

test.describe('Error pane: UI affordances', () => {
  test('shows the offending folder path and a collapsed technical details disclosure', async ({ tauriPage }) => {
    // Covers two behaviors the friendly error pane promises but no existing
    // test asserts: (1) the folder path the user tried to enter is displayed
    // so the message is actually contextualized, and (2) the technical details
    // <details> element starts collapsed and expands on click.
    await ensureAppReady(tauriPage)

    await injectAndNavigateIntoSubDir(tauriPage, 60)

    await expect
      .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('.error-pane')`), { timeout: 3000 })
      .toBeTruthy()

    // The displayed folder path must end with the path we navigated into.
    const folderPath = await tauriPage.evaluate<string>(
      `(document.querySelector('.error-pane .folder-path')?.textContent || '').trim()`,
    )
    expect(folderPath.endsWith('/left/sub-dir')).toBe(true)

    // Disclosure starts collapsed.
    const startsCollapsed = await tauriPage.evaluate<boolean>(
      `!document.querySelector('.error-pane .technical-details')?.hasAttribute('open')`,
    )
    expect(startsCollapsed).toBe(true)

    // Clicking the <summary> expands it.
    await tauriPage.evaluate(`(function() {
            var summary = document.querySelector('.error-pane .technical-details summary');
            if (summary) summary.click();
        })()`)
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `document.querySelector('.error-pane .technical-details')?.hasAttribute('open') || false`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()

    // The raw-detail block under the expanded disclosure should hold a non-empty string.
    const rawDetail = await tauriPage.evaluate<string>(
      `(document.querySelector('.error-pane .raw-detail')?.textContent || '').trim()`,
    )
    expect(rawDetail.length).toBeGreaterThan(0)

    // Clean up
    await navigateBackToLeft(tauriPage)
  })
})
