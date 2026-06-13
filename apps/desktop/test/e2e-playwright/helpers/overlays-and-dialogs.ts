/**
 * Overlay, toast, and transfer-dialog helpers for the Cmdr Playwright E2E tests.
 *
 * `dismissOverlay` / `expectAndDismissToast` are the only sanctioned ways to
 * close overlays and toasts (never `keyboard.press('Escape')` — see the suite's
 * CLAUDE.md). `readDialogCounters` / `expectDialogCounters` / `countTree` cover
 * the transfer dialog's scan-tally assertions.
 */

import fs from 'fs'
import { expect } from '@playwright/test'
import { type PageLike, TRANSFER_DIALOG } from './core.js'

/**
 * Overlay selectors that `dismissOverlay` and the global afterEach safety net know
 * about. Listed in priority order (foreground-most first): a popover open ON a
 * dialog should close before the dialog itself. The afterEach probes all of them
 * for leaks; `dismissOverlay` closes the topmost open one per call.
 *
 * If you add a new overlay surface (a new dialog kind, a new dropdown), add its
 * selector here so the safety net catches leaks of it too.
 */
const OVERLAY_SELECTORS = [
  '.ui-popover',
  '.palette-overlay',
  '.search-overlay',
  '.modal-overlay',
  '.volume-dropdown',
] as const

// ── Overlay + toast dismissal ───────────────────────────────────────────────

/**
 * Dismiss the topmost open overlay (modal dialog, command palette, search
 * dialog, filter-chip popover, volume picker dropdown) via synthetic Escape on
 * the overlay element itself, then assert it actually closed.
 *
 * Why dispatch on the overlay and not at the document or window level:
 *
 * - `ModalDialog.svelte` binds its `onkeydown` on the `.modal-overlay` div, not
 *   on `<svelte:window>`. A `document.dispatchEvent` bubbles up to `window` and
 *   never reaches the overlay's listener (events don't descend into subtrees).
 * - `tauriPage.keyboard.press('Escape')` works on macOS because the OS routes
 *   the keystroke to the focused element (the overlay focuses itself on
 *   mount), but flakes on Linux Xvfb where X11 focus delivery isn't reliable.
 * - Dispatching on the overlay element with `bubbles: true` reaches BOTH
 *   element-bound (target phase) and window-bound (bubble phase) listeners,
 *   so it's the universal pattern across overlay kinds.
 *
 * Throws if no overlay is open (catches tests that call dismiss when nothing
 * is up — typically a leak from an earlier step that already closed). Fails
 * via `expect.poll` if the overlay doesn't close within 3s.
 *
 * For toasts, use `dismissAllToasts` instead — toasts dismiss via a Close
 * button click, not via Escape.
 */
export async function dismissOverlay(tauriPage: PageLike): Promise<void> {
  const selectorsJson = JSON.stringify(OVERLAY_SELECTORS)
  const selector = await tauriPage.evaluate<string | null>(`(function(){
        var sels = ${selectorsJson};
        for (var i = 0; i < sels.length; i++) {
            if (document.querySelector(sels[i]) !== null) return sels[i];
        }
        return null;
    })()`)
  if (selector === null) {
    throw new Error(
      `dismissOverlay: no overlay is open (checked ${OVERLAY_SELECTORS.join(', ')}). ` +
        `If you expected one, something dismissed it earlier; ` +
        `if not, drop the dismissOverlay() call.`,
    )
  }
  const sel = JSON.stringify(selector)
  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${sel});
        if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`)
  await expect.poll(async () => (await tauriPage.count(selector)) === 0, { timeout: 3000 }).toBeTruthy()
}

/**
 * Assert that exactly ONE toast whose text contains `substring` appears within
 * `timeout`, then dismiss that single toast.
 *
 * This is the ONLY public toast helper on purpose. Tests that trigger a
 * user-visible toast should care that it appeared (the toast IS the user-
 * facing confirmation; ship-the-wording is the contract) — not just clean
 * up after it. Bundling the assert + dismiss into one call removes the
 * "I'll just clean up the leak" shortcut: if you want to dismiss a toast,
 * you have to assert it first.
 *
 * Per-toast scoping (not blanket cleanup): the helper closes ONLY the first
 * `.toast` whose `.toast-message` text contains `substring`. Other toasts
 * stay open and will fail the test via the global afterEach safety net. If
 * your test fires multiple distinct toasts (rare), call this once per toast
 * with a substring that uniquely identifies each.
 *
 * Substring match is case-sensitive. Pass a stable prefix or unique fragment;
 * toast message format can change in non-load-bearing ways (whitespace,
 * pluralization) and assertion shouldn't break on those.
 *
 * The global afterEach safety net catches any toast you forgot to dismiss
 * and fails the test, so leaked toasts are loud, not silent.
 */
export async function expectAndDismissToast(
  tauriPage: PageLike,
  substring: string,
  options: { timeout?: number } = {},
): Promise<void> {
  const timeout = options.timeout ?? 3000
  const sub = JSON.stringify(substring)
  // Match against the WHOLE toast's textContent, not just `.toast-message`:
  // string-content toasts render their text in a `.toast-message` span, but
  // component-content toasts (`QuickLookHintToastContent`, the AI download
  // toast, error-report toasts) render the body straight into `.toast-content`
  // without that wrapper. Reading the toast element's textContent covers both.
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
            var toasts = document.querySelectorAll('.toast');
            for (var i = 0; i < toasts.length; i++) {
                if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) return true;
            }
            return false;
        })()`),
      { timeout },
    )
    .toBeTruthy()
  // Click the close button on the SAME toast we just asserted, leaving any
  // other toasts open (they'll fail their own tests' afterEach checks).
  await tauriPage.evaluate(`(function(){
        var toasts = document.querySelectorAll('.toast');
        for (var i = 0; i < toasts.length; i++) {
            if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) {
                var close = toasts[i].querySelector('.toast-close');
                if (close) close.click();
                return;
            }
        }
    })()`)
  // Wait for the specific toast to be gone. We poll the same substring match
  // to avoid races with neighboring toasts (which are out of scope here).
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
            var toasts = document.querySelectorAll('.toast');
            for (var i = 0; i < toasts.length; i++) {
                if ((toasts[i].textContent || '').indexOf(${sub}) !== -1) return false;
            }
            return true;
        })()`),
      { timeout: 2000 },
    )
    .toBeTruthy()
}

// ── Transfer-dialog counters ─────────────────────────────────────────────────

/**
 * Expected counter values for {@link expectDialogCounters}.
 *
 * `files` / `dirs` are exact integers — the dialog renders them through
 * `formatNumber` (`toLocaleString('en-US')`), so they read as plain digits for
 * the small counts E2E fixtures use (no thousands separators here).
 *
 * `bytes` is optional and format-aware. The dialog renders the byte total via
 * the `<Size>` component in dynamic mode, so it's the user-facing string like
 * `"3.19 KB"` — NOT a raw byte count. Pass that exact string, or a RegExp when
 * the fixture's size is allowed to drift within a band (e.g. /^\d+(\.\d+)? KB$/).
 * Omit it when only the file/dir split matters. There's no substring shrug:
 * a string is matched whole, a RegExp is `.test()`-ed against the whole cell.
 */
export interface ExpectedDialogCounters {
  /** Exact byte string the FE renders (e.g. "3.19 KB"), or a RegExp to match it. Omit to skip the byte assertion. */
  bytes?: string | RegExp
  /** Exact top-level file count. */
  files: number
  /** Exact top-level directory count. */
  dirs: number
  /**
   * Accept the `skipped` scan state (tallies legitimately stay at 0 because no
   * deep scan runs — e.g. a same-non-default-volume move's server-side rename).
   * When set, the helper waits for `done` OR `skipped`; otherwise only `done`.
   */
  allowSkipped?: boolean
}

/**
 * Reads the three live counter cells out of the transfer dialog's tallies
 * element and returns them as `{ scanState, bytes, files, dirs }` (or `null`
 * when the dialog isn't open). `bytes` is the rendered cell text (`"3.19 KB"`);
 * `files` / `dirs` are the parsed integer counts. Used internally by
 * {@link expectDialogCounters}; exposed for the rare test that wants the raw
 * snapshot.
 */
export async function readDialogCounters(
  tauriPage: PageLike,
): Promise<{ scanState: string; bytes: string; files: number; dirs: number } | null> {
  return tauriPage.evaluate<{ scanState: string; bytes: string; files: number; dirs: number } | null>(
    `(function(){
        var stats = document.querySelector('${TRANSFER_DIALOG} .scan-stats');
        if (!stats) return null;
        var cells = stats.querySelectorAll('.scan-stat');
        if (cells.length < 3) return null;
        function valueOf(cell){
            var v = cell.querySelector('.scan-value');
            return v ? (v.textContent || '').trim() : '';
        }
        var filesText = valueOf(cells[1]).replace(/,/g, '');
        var dirsText = valueOf(cells[2]).replace(/,/g, '');
        return {
            scanState: stats.getAttribute('data-scan-state') || '',
            bytes: valueOf(cells[0]),
            files: parseInt(filesText, 10),
            dirs: parseInt(dirsText, 10),
        };
    })()`,
  )
}

/**
 * Asserts the transfer dialog's counter line ("3.19 KB / 1 file / 0 dirs"),
 * race-free.
 *
 * First polls `data-scan-state` on the tallies element until it reads `done`
 * (or `done`/`skipped` when `allowSkipped` is set), so the assertion never
 * fires mid-scan against partial totals. Then asserts the exact file and dir
 * counts and, if `bytes` was provided, the rendered byte string.
 *
 * The dialog must already be open and the destination/operation settled — call
 * this right after `waitForSelector(TRANSFER_DIALOG, …)` and after any
 * Copy/Move toggle the test performs (the toggle restarts the scan, so the
 * `data-scan-state` poll re-synchronises automatically).
 *
 * @example
 * await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
 * await expectDialogCounters(tauriPage, { bytes: '3.19 KB', files: 1, dirs: 0 })
 */
export async function expectDialogCounters(
  tauriPage: PageLike,
  expected: ExpectedDialogCounters,
  options: { timeout?: number } = {},
): Promise<void> {
  const timeout = options.timeout ?? 10000
  const terminalStates = expected.allowSkipped ? ['done', 'skipped'] : ['done']

  // Wait for the scan to settle to a terminal state before reading counts, so
  // we never assert against a partial in-flight tally.
  await expect
    .poll(
      async () => {
        const snapshot = await readDialogCounters(tauriPage)
        return snapshot !== null && terminalStates.includes(snapshot.scanState)
      },
      { timeout },
    )
    .toBeTruthy()

  const snapshot = await readDialogCounters(tauriPage)
  expect(snapshot, 'transfer dialog tallies element present').not.toBeNull()
  if (snapshot === null) return // unreachable after the poll, narrows the type

  expect(snapshot.files, `top-level file count (state=${snapshot.scanState})`).toBe(expected.files)
  expect(snapshot.dirs, `top-level dir count (state=${snapshot.scanState})`).toBe(expected.dirs)

  if (expected.bytes !== undefined) {
    if (typeof expected.bytes === 'string') {
      expect(snapshot.bytes, 'rendered byte total').toBe(expected.bytes)
    } else {
      expect(
        expected.bytes.test(snapshot.bytes),
        `rendered byte total "${snapshot.bytes}" matches ${String(expected.bytes)}`,
      ).toBe(true)
    }
  }
}

/**
 * Walks a set of source paths (files or directories) and returns the RECURSIVE
 * file / dir counts the transfer scan preview would report for them — the same
 * totals the dialog's tallies show. A directory contributes itself to `dirs`
 * and recurses into its children; a file contributes itself to `files`.
 *
 * Used by the sweep specs that select a folder (or `selectAll` over `bulk/`):
 * computing the counts from the actual fixture tree keeps the assertion exact
 * AND self-maintaining if a fixture's contents change, without hardcoding the
 * ~23-file / ~170 MB `bulk/` shape. Node-side only (reads real disk), so call
 * it on the absolute fixture paths the spec already has.
 */
export function countTree(absPaths: string[]): { files: number; dirs: number } {
  let files = 0
  let dirs = 0
  const walk = (p: string): void => {
    const stat = fs.lstatSync(p)
    if (stat.isDirectory()) {
      dirs++
      for (const child of fs.readdirSync(p)) walk(`${p}/${child}`)
    } else {
      files++
    }
  }
  for (const p of absPaths) walk(p)
  return { files, dirs }
}
