/**
 * E2E for the one quit-gate promise only a real webview can check: **the prompt
 * layers above a dialog that's already open.**
 *
 * Every other modal sits at `--z-modal` and they stack by DOM order, which is
 * not something a window-level prompt can rest on; the quit dialog opts into
 * `--z-modal-top`. The stand-in here is a gallery-opened `alert`, on exactly the
 * same rung as the `operation-conflict` dialog this has to beat — that one can
 * only be raised by a real deep-merge clash mid-background-copy, which no spec
 * can stage (its gallery row is `not-triggerable` for the same reason).
 *
 * **Why this spec never triggers a real quit.** The suite shares one app process
 * across specs, so a gate bug would take every later spec down with it and read
 * as a harness failure rather than a quit-gate failure. The decision, the
 * countdown, the frontend-never-answers case, and the teardown ordering are all
 * covered against the real gate in `src-tauri/src/quit/tests.rs`. The
 * `quit-requested` payload here is emitted directly, exactly as the backend
 * emits it.
 *
 * **Why there's no reload test here.** The defect the gate replaced (a
 * `beforeunload` handler cancelling the global registry) can't be red-tested
 * through a reload: the old handler's IPC raced page teardown, so a backgrounded
 * copy survived a reload anyway about half the time — a spec that goes green on
 * a broken build. It's pinned deterministically instead, in
 * `src/lib/quit/no-teardown-cancel.test.ts`.
 *
 * Requires `--features playwright-e2e`.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const QUIT_DIALOG = '[data-dialog-id="quit-confirmation"]'
const ALERT_DIALOG = '[data-dialog-id="alert"]'

/** Emits the event the quit gate emits when it holds an exit. */
async function emitQuitRequested(main: TauriPage, countdownMs: number): Promise<void> {
  await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
    event: 'quit-requested',
    payload: {
      operations: [{
        operationId: 'e2e-quit-op',
        operationType: 'copy',
        status: 'running',
        source: 'Holiday.mov',
        destination: 'Backup',
        supportsRollback: true,
        error: null
      }],
      countdownMs: ${String(countdownMs)}
    }
  })`)
}

test.beforeEach(async ({ tauriPage }) => {
  await ensureAppReady(tauriPage)
})

test.describe('Quit gate', () => {
  test('the quit prompt sits above a dialog that is already open', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage

    // Raise an ordinary modal first, from the gallery.
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'debug-open-gallery-dialog',
      payload: { dialogId: 'alert', stateId: 'short', fixtures: null }
    })`)
    await main.waitForSelector(ALERT_DIALOG, 5000)

    await emitQuitRequested(main, 15_000)
    await main.waitForSelector(QUIT_DIALOG, 5000)

    // Both are painted; the quit one wins, and wins by z-index rather than by
    // happening to come later in the DOM.
    const layering = await main.evaluate(`(function() {
      var quit = document.querySelector('${QUIT_DIALOG}');
      var alertOverlay = document.querySelector('${ALERT_DIALOG}');
      var panel = quit.querySelector('.modal-dialog');
      var box = panel.getBoundingClientRect();
      var hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return JSON.stringify({
        quitZ: Number(getComputedStyle(quit).zIndex),
        otherZ: Number(getComputedStyle(alertOverlay).zIndex),
        quitOwnsTheCentre: quit.contains(hit)
      });
    })()`)
    const { quitZ, otherZ, quitOwnsTheCentre } = JSON.parse(layering as string) as {
      quitZ: number
      otherZ: number
      quitOwnsTheCentre: boolean
    }
    expect(quitZ).toBeGreaterThan(otherZ)
    expect(quitOwnsTheCentre).toBe(true)

    // It counts down for display, and "Keep working" puts it away without
    // touching the dialog underneath.
    expect(await main.textContent(QUIT_DIALOG)).toContain('Quitting in 15 seconds')
    await main.evaluate(`(function() {
      document.querySelectorAll('${QUIT_DIALOG} .modal-footer button')[0].click();
    })()`)
    await expect.poll(async () => main.isVisible(QUIT_DIALOG), { timeout: 5000 }).toBeFalsy()
    expect(await main.isVisible(ALERT_DIALOG)).toBe(true)

    // Leave nothing open: the post-test leak guard fails whoever does.
    await main.evaluate(`(function() {
      var overlay = document.querySelector('${ALERT_DIALOG}');
      if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)
    await expect.poll(async () => main.isVisible(ALERT_DIALOG), { timeout: 5000 }).toBeFalsy()
  })
})
