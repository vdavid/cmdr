/**
 * E2E tests for dialog focus trapping (`$lib/ui/focus-trap.ts`).
 *
 * Tier-2 flow coverage jsdom can't model: real focus movement across the
 * dialog/background boundary. Regression anchor: pressing Tab twice in the
 * command palette used to walk focus into the blurred background, where the
 * suppressed global dispatch left Esc, ⌘⇧P, and Tab all dead — a full keyboard
 * lockout, mouse-only recovery.
 *
 * Synthetic KeyboardEvents don't trigger the browser's default tabbing, so
 * these tests exercise the trap's own behavior: boundary wrapping (the trap
 * moves focus itself), the focusin leak guard, and the Escape fallback.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dismissOverlay, CTRL_OR_META, MKDIR_DIALOG, type PageLike } from './helpers.js'

const PALETTE = '.palette-overlay'
const PALETTE_INPUT = '.palette-overlay .search-input'
// Mirrors FOCUSABLE_SELECTOR in `src/lib/ui/focus-trap.ts`. Single-quote-free so it
// can sit inside single-quoted JS strings passed to `evaluate()`.
const TABBABLE = '[href], button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'

async function openCommandPalette(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  await tauriPage.waitForSelector(PALETTE, 3000)
}

test.describe('Dialog focus trapping', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test('command palette: Tab keeps focus in the search input', async ({ tauriPage }) => {
    await openCommandPalette(tauriPage)

    // The palette focuses its input on mount.
    await expect
      .poll(() => tauriPage.evaluate(`document.activeElement === document.querySelector('${PALETTE_INPUT}')`), {
        timeout: 3000,
      })
      .toBeTruthy()

    // Two Tab presses used to land focus in the background pane (the lockout).
    // The palette swallows Tab now: focus must not move at all.
    for (let i = 0; i < 2; i++) {
      await tauriPage.evaluate(
        `document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))`,
      )
    }
    const stillInInput = await tauriPage.evaluate(
      `document.activeElement === document.querySelector('${PALETTE_INPUT}')`,
    )
    expect(stillInInput, 'focus left the palette input after Tab').toBe(true)

    await dismissOverlay(tauriPage)
  })

  test('command palette: programmatically leaked focus is pulled back', async ({ tauriPage }) => {
    await openCommandPalette(tauriPage)
    await expect
      .poll(() => tauriPage.evaluate(`document.activeElement === document.querySelector('${PALETTE_INPUT}')`), {
        timeout: 3000,
      })
      .toBeTruthy()

    // Steal focus the way background code can: a direct .focus() call on the
    // explorer (tabindex="0"). The trap's focusin guard must pull it back.
    await tauriPage.evaluate(`document.querySelector('.dual-pane-explorer').focus()`)
    await expect
      .poll(
        () =>
          tauriPage.evaluate(
            `document.activeElement !== null && document.activeElement.closest('${PALETTE}') !== null`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()

    await dismissOverlay(tauriPage)
  })

  test('command palette: Escape still closes when focus has escaped', async ({ tauriPage }) => {
    await openCommandPalette(tauriPage)
    await expect
      .poll(() => tauriPage.evaluate(`document.activeElement === document.querySelector('${PALETTE_INPUT}')`), {
        timeout: 3000,
      })
      .toBeTruthy()

    // Steal focus and press Escape in the same synchronous task, BEFORE the
    // leak guard's microtask can pull focus back. This is the exact broken
    // state of the original lockout; the trap's Escape fallback must close
    // the palette even though the palette's own handler can't see the event.
    await tauriPage.evaluate(`(() => {
        const explorer = document.querySelector('.dual-pane-explorer');
        explorer.focus();
        explorer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    })()`)

    await expect
      .poll(() => tauriPage.evaluate(`document.querySelector('${PALETTE}') === null`), { timeout: 3000 })
      .toBeTruthy()

    // Closing must land keyboard focus back in the explorer (Escape-return-focus).
    await expect
      .poll(
        () =>
          tauriPage.evaluate(
            `document.activeElement !== null && document.activeElement.closest('.dual-pane-explorer') !== null`,
          ),
        { timeout: 3000 },
      )
      .toBeTruthy()
  })

  test('new-folder dialog: Tab wraps at both ends of the control cycle', async ({ tauriPage }) => {
    await tauriPage.keyboard.press('F7')
    await tauriPage.waitForSelector(MKDIR_DIALOG, 3000)

    // Tab on the last tabbable wraps to the first.
    await tauriPage.evaluate(`(() => {
        const dialog = document.querySelector('${MKDIR_DIALOG}');
        const tabbables = dialog.closest('.modal-overlay').querySelectorAll('${TABBABLE}');
        const last = tabbables[tabbables.length - 1];
        last.focus();
        last.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
    })()`)
    const wrappedToFirst = await tauriPage.evaluate(`(() => {
        const tabbables = document.querySelector('${MKDIR_DIALOG}').closest('.modal-overlay').querySelectorAll('${TABBABLE}');
        return document.activeElement === tabbables[0];
    })()`)
    expect(wrappedToFirst, 'Tab on the last control must wrap to the first').toBe(true)

    // Shift+Tab on the first tabbable wraps to the last.
    await tauriPage.evaluate(`(() => {
        const tabbables = document.querySelector('${MKDIR_DIALOG}').closest('.modal-overlay').querySelectorAll('${TABBABLE}');
        const first = tabbables[0];
        first.focus();
        first.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
    })()`)
    const wrappedToLast = await tauriPage.evaluate(`(() => {
        const tabbables = document.querySelector('${MKDIR_DIALOG}').closest('.modal-overlay').querySelectorAll('${TABBABLE}');
        return document.activeElement === tabbables[tabbables.length - 1];
    })()`)
    expect(wrappedToLast, 'Shift+Tab on the first control must wrap to the last').toBe(true)

    await dismissOverlay(tauriPage)
  })
})
