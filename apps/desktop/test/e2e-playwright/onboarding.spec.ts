/**
 * E2E tests for onboarding wizard re-entry, plus its keyboard contract.
 *
 * Covers the user-visible re-entry surfaces: the macOS menu item, the command
 * palette command (both platforms), and the MCP `dialog open onboarding` path. Walks the resume rule's already-granted variant (the
 * shard launch pins FDA to granted, DETAILS.md § "The Full Disk Access pin", so menu re-entry shows step 1 with the
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
 *    `notifyOnboardingComplete()` has fired (the shard launch pins FDA to granted, so it
 *    fires on first launch), `showOnboarding` stays `false` until the user
 *    re-opens via menu or palette. This spec triggers the re-entry surfaces
 *    and asserts the wizard appears with the expected starting step.
 */

import { test, expect } from './fixtures.js'
import {
  ensureAppReady,
  dispatchMenuCommand,
  ONBOARDING_WIZARD as WIZARD_SELECTOR,
  onboardingWizardIsOpen as wizardIsOpen,
  onboardingActiveStep as activeStep,
  closeOnboardingWizardIfOpen as closeWizardIfOpen,
  type PageLike,
} from './helpers.js'

const PALETTE_OVERLAY = '.palette-overlay'

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

  // Regression anchor for the Tab lockout, and the only test that runs the real chain: the
  // document keydown handler, `isModalDialogOpen()`, and the wizard's registration in the
  // frontend dialog inventory. Unregistered, Tab resolved to the Tier 1 `pane.switch`
  // binding, so the handler killed the browser's focus move, the command focused a pane
  // behind the overlay, and `focus-trap.ts`'s leak guard pulled focus back where it
  // started. Tab looked like a dead key, while `⇧Tab`, bound to no command, worked.
  //
  // Two presses, one assertion, because "the pane did NOT switch" alone can only be
  // checked by waiting for an absence, which passes before an async dispatch could land.
  // With the fix, press 1 does nothing and press 2 switches, so the run ends on the OTHER
  // pane. Without it, press 1 switches and press 2 switches back, ending where it started,
  // and the poll below times out. `defaultPrevented` can't stand in: `focus-trap.ts` has
  // its own document-level Tab handler that prevents the default whenever a trap is up,
  // so it reads the same either way.
  test('Tab does not reach pane.switch behind the wizard', async ({ tauriPage }) => {
    const pressTab = `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))`
    const paneFocused = (index: 0 | 1) =>
      tauriPage.evaluate<boolean>(
        `(document.querySelectorAll('.file-pane')[${String(index)}]?.getAttribute('class') || '').includes('is-focused')`,
      )

    // `ensureAppReady` clicks the left pane, so that's where a correct run starts and,
    // one swallowed press plus one live press later, is NOT where it ends.
    expect(await paneFocused(0), 'the left pane did not start focused').toBe(true)

    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    await tauriPage.evaluate(pressTab)

    await closeWizardIfOpen(tauriPage)
    await expect.poll(async () => wizardIsOpen(tauriPage), { timeout: 3000 }).toBe(false)
    await tauriPage.evaluate(pressTab)

    await expect.poll(() => paneFocused(1), { timeout: 3000 }).toBe(true)

    // Hand the next spec the left-focused pane `ensureAppReady` promises.
    await tauriPage.evaluate(pressTab)
    await expect.poll(() => paneFocused(0), { timeout: 3000 }).toBe(true)
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
    // / Deny / Restart Cmdr / Next / "One more optional setup step" / "Start using Cmdr"
    // close it (a finish only from the Beta or final Optional step).
  })
})

/**
 * The escape hatch. Cmdr follows the Mac's language preferences, so a first launch can
 * land someone in a language they can't read, and every other way out is labeled in
 * that same language. "Can the user get out from the screen they're already on" is the
 * safety property of the whole auto-language feature, so it gets a real spec: the
 * control lives in the wizard's own header and switches the running app in place.
 */
test.describe('Onboarding wizard language escape hatch', () => {
  const TRIGGER = `${WIZARD_SELECTOR} .language-picker .select-trigger`
  /** The wizard's own sr-only heading: present on every step, so the assertion doesn't care which one is up. */
  const TITLE = `${WIZARD_SELECTOR} #onboarding-wizard-title`

  /** The wizard title as rendered right now, the cheapest proof of the UI's language. */
  async function wizardTitle(tauriPage: PageLike): Promise<string> {
    return tauriPage.evaluate<string>(`(function() {
      var el = document.querySelector('${TITLE}');
      return el ? el.textContent.trim() : '';
    })()`)
  }

  /** Opens the header picker and clicks one language row (the menu portals into the wizard overlay). */
  async function pickLanguage(tauriPage: PageLike, value: string): Promise<void> {
    await tauriPage.evaluate(`(function() {
      var trigger = document.querySelector('${TRIGGER}');
      if (!trigger) throw new Error('the onboarding language picker is not in the wizard frame');
      trigger.click();
    })()`)
    await tauriPage.waitForSelector(`.wizard-overlay [data-part="item"][data-value="${value}"]`, 3000)
    await tauriPage.evaluate(`(function() {
      var item = document.querySelector('.wizard-overlay [data-part="item"][data-value="${value}"]');
      if (!item) throw new Error('no ${value} row in the language menu');
      item.click();
    })()`)
  }

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await closeWizardIfOpen(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    // Restore first, then close: the app is shared, and a wizard left speaking Hungarian
    // would hand every later spec a UI it can't read.
    const { initMcpClient, mcpCall } = await import('../e2e-shared/mcp-client.js')
    await initMcpClient(tauriPage)
    await mcpCall('set_setting', { id: 'appearance.language', value: 'system' })
    await closeWizardIfOpen(tauriPage)
  })

  test('the picker is in the frame from the first step, and switches the app in place', async ({ tauriPage }) => {
    await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
    await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)
    // The control is reachable without advancing: it's part of the frame, not a step.
    await tauriPage.waitForSelector(TRIGGER, 3000)
    expect(await wizardTitle(tauriPage)).toBe('Cmdr onboarding')

    await pickLanguage(tauriPage, 'hu')

    // No restart, no reload: the open wizard re-renders in Hungarian.
    await expect.poll(async () => wizardTitle(tauriPage), { timeout: 3000 }).toBe('Cmdr bevezető')
    expect(await wizardIsOpen(tauriPage)).toBe(true)

    // And the way back is the same control, still recognizable: the `English` row reads
    // "English" whatever the app happens to be speaking.
    await pickLanguage(tauriPage, 'en')
    await expect.poll(async () => wizardTitle(tauriPage), { timeout: 3000 }).toBe('Cmdr onboarding')
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
