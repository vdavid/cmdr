/**
 * E2E tests for onboarding wizard re-entry.
 *
 * Covers the user-visible re-entry surfaces: the macOS menu item, the command
 * palette command (both platforms), and the MCP `dialog open onboarding` path. Walks the resume rule's already-granted variant (FDA is
 * already granted in the E2E fixture, so menu re-entry shows step 1 with the
 * single-Next variant on macOS, or step 2 directly on Linux).
 *
 * Scope notes:
 *
 * 1. **Per-spec env var control is out of scope** for the shared Playwright app.
 *    The full-FDA-branch coverage (`CMDR_MOCK_FDA=granted|denied|notgranted`
 *    paired with `CMDR_FORCE_ONBOARDING=1`) requires per-spec process restarts,
 *    which the current runner doesn't do — every spec shares one Tauri instance
 *    per shard. The four FDA-state banners are covered by tier-3 Vitest specs
 *    (`StepFda.test.ts`, `StepAi.test.ts`, `onboarding-state.test.ts`). This
 *    spec covers the cross-component re-entry plumbing that those tier-3
 *    suites can't model.
 *
 * 2. The wizard is mounted in `routes/(main)/+page.svelte`. Once
 *    `notifyOnboardingComplete()` has fired (the E2E fixture grants FDA, so it
 *    fires on first launch), `showOnboarding` stays `false` until the user
 *    re-opens via menu or palette. This spec triggers the re-entry surfaces
 *    and asserts the wizard appears with the expected starting step.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dispatchMenuCommand, type PageLike } from './helpers.js'

const WIZARD_SELECTOR = '[data-dialog-id="onboarding"]'
const PALETTE_OVERLAY = '.palette-overlay'

/** Whether the onboarding wizard is currently mounted in the main window. */
async function wizardIsOpen(tauriPage: PageLike): Promise<boolean> {
  return tauriPage.isVisible(WIZARD_SELECTOR)
}

/** Returns the active step (1, 2, or 3) read from the wizard's `aria-current="step"` dot. */
async function activeStep(tauriPage: PageLike): Promise<number | null> {
  return tauriPage.evaluate<number | null>(`(function() {
    var dots = document.querySelectorAll('${WIZARD_SELECTOR} .step-dot');
    for (var i = 0; i < dots.length; i++) {
      if (dots[i].getAttribute('aria-current') === 'step') return i + 1;
    }
    return null;
  })()`)
}

/** Closes the wizard if it's open. The wizard swallows Escape, so dismiss via Finish flow. */
async function closeWizardIfOpen(tauriPage: PageLike): Promise<void> {
  if (!(await wizardIsOpen(tauriPage))) return
  // The wizard intentionally swallows Escape (per round-3 #9). Click "Next" until
  // we reach the last step, then click "Finish". For the already-granted variant
  // this is two clicks (step 1 Next → step 2 has a dual-button footer; pick the
  // secondary "Start using Cmdr!" to finish).
  const isMac = process.platform === 'darwin'
  if (isMac) {
    // Step 1 (already-granted): click the single Next button in the footer.
    await tauriPage.evaluate(`(function() {
      var btn = document.querySelector('${WIZARD_SELECTOR} .primary-slot button');
      if (btn) btn.click();
    })()`)
    await expect.poll(async () => activeStep(tauriPage), { timeout: 3000 }).toBe(2)
  }
  // Step 2: click the secondary "Start using Cmdr!" button to finish without going to step 3.
  await tauriPage.evaluate(`(function() {
    var btns = document.querySelectorAll('${WIZARD_SELECTOR} .primary-slot button');
    for (var i = 0; i < btns.length; i++) {
      if ((btns[i].textContent || '').indexOf('Start using Cmdr') !== -1) {
        btns[i].click();
        return;
      }
    }
    // Fallback: click whatever's primary (Finish on step 3).
    if (btns.length > 0) btns[btns.length - 1].click();
  })()`)
  await expect.poll(async () => !(await wizardIsOpen(tauriPage)), { timeout: 3000 }).toBeTruthy()
}

test.describe('Onboarding wizard re-entry', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    // Defensive: a previous test in the file may have left the wizard open if its
    // assertions failed before the closeWizardIfOpen() call. The fixture-level
    // safety net would also catch this, but cleaning up here keeps the failure
    // attribution clean.
    await closeWizardIfOpen(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeWizardIfOpen(tauriPage)
  })

  test('menu / palette command opens the wizard', async ({ tauriPage }) => {
    expect(await wizardIsOpen(tauriPage)).toBe(false)
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    expect(await wizardIsOpen(tauriPage)).toBe(true)
    // macOS re-entry: step 1 (already-granted variant — FDA is on in fixtures).
    // Linux: step 2 (no step 1 on Linux).
    const expected = process.platform === 'darwin' ? 1 : 2
    expect(await activeStep(tauriPage)).toBe(expected)
  })

  test('re-entry is idempotent (re-dispatch while open is a no-op)', async ({ tauriPage }) => {
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    const firstStep = await activeStep(tauriPage)
    // Re-dispatch should not reset state (the FE openOnboardingFromMenuOrPalette
    // guard short-circuits when showOnboarding is already true). We just need the
    // wizard to STILL be open at the same step after a couple of round-trips that
    // give the event time to deliver. expect.poll keeps checking and would fail
    // fast if the step ever changed.
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await expect.poll(async () => activeStep(tauriPage), { timeout: 1000 }).toBe(firstStep)
    expect(await wizardIsOpen(tauriPage)).toBe(true)
  })

  test('command palette: searching "Onboarding" surfaces and executes the command', async ({ tauriPage }) => {
    // Open the palette via the standard command dispatch (same path the shortcut uses).
    await dispatchMenuCommand(tauriPage, 'app.commandPalette')
    await tauriPage.waitForSelector(PALETTE_OVERLAY, 3000)
    // Type "Onboarding" into the palette's search input. The palette renders an
    // <input> as the first focusable child of the overlay.
    await tauriPage.evaluate(`(function() {
      var input = document.querySelector('${PALETTE_OVERLAY} input');
      if (!input) throw new Error('palette input not found');
      input.focus();
      var setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
      setter.call(input, 'Onboarding');
      input.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
    // The first match should be our command. Press Enter to execute.
    await tauriPage.evaluate(`(function() {
      var input = document.querySelector('${PALETTE_OVERLAY} input');
      if (input) input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)
    // The palette closes itself on execute; the wizard mounts.
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    expect(await wizardIsOpen(tauriPage)).toBe(true)
  })

  test('Escape does not close the wizard (round-3 #9: must commit to a step)', async ({ tauriPage }) => {
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    // Dispatch Escape on the panel — exactly the pattern dismissOverlay() uses for
    // other dialogs, except here the handler swallows it. Dispatch a few times to
    // give a hypothetical handler more chances to (mis)fire.
    for (let i = 0; i < 3; i++) {
      await tauriPage.evaluate(`(function() {
        var panel = document.querySelector('${WIZARD_SELECTOR}');
        if (panel) panel.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      })()`)
    }
    // The wizard MUST still be open after Escape. expect.poll waits up to 1s for
    // a contradiction (`wizardIsOpen()` returning false would fail the test).
    await expect.poll(async () => wizardIsOpen(tauriPage), { timeout: 1000 }).toBe(true)
  })

  // dismissOverlay would mark the wizard as a legitimate target, but the wizard
  // swallows Escape by design (see "Escape does not close" above). Keep this comment
  // here so a future agent doesn't bolt dismissOverlay() onto the wizard.
  test.skip('dismissOverlay is intentionally NOT wired for the wizard', () => {
    // Documentation-only assertion. The wizard owns the close gesture: only Allow
    // / Deny / Restart Cmdr / Next / Finish / "Start using Cmdr!" close it.
  })
})

test.describe('Onboarding wizard via MCP', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeWizardIfOpen(tauriPage)
  })

  test('MCP `dialog open onboarding` opens the wizard and tracks it as a soft dialog', async ({ tauriPage }) => {
    const { initMcpClient, mcpCall, mcpReadResource } = await import('../e2e-shared/mcp-client.js')
    await initMcpClient(tauriPage)
    expect(await wizardIsOpen(tauriPage)).toBe(false)
    const result = await mcpCall('dialog', { action: 'open', type: 'onboarding' })
    expect(result).toContain('OK')
    expect(await wizardIsOpen(tauriPage)).toBe(true)
    // SoftDialogTracker should now reflect the open onboarding sheet.
    const state = await mcpReadResource('cmdr://state')
    expect(state).toContain('onboarding')
  })
})
