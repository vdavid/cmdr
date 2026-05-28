/**
 * Behaviour tests for the onboarding wizard shell.
 *
 * Covers:
 * - Escape is a no-op (the wizard intentionally swallows it).
 * - The step-1 footer button reflects state: hidden in `decide` mode, `Restart Cmdr` in
 *   `restart` mode, `Next` for the `already-granted` variant.
 * - Forward navigation from `already-granted` → step 2 → step 3 → Finish fires `onComplete`.
 * - Back from step 2 returns to step 1 and resets the footer to `decide` mode.
 * - The hand-rolled focus trap wraps Tab and Shift+Tab through the panel's focusables.
 *
 * Axe-based a11y coverage lives in `OnboardingWizard.a11y.test.ts`.
 * Per-step component behaviour lives in `StepFda.test.ts` etc.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import OnboardingWizard from './OnboardingWizard.svelte'
import {
  closeWizard,
  getOnboardingState,
  resetForTesting,
  openWizard,
  setStep1Variant,
  setStep1Restart,
  setCurrentStep,
} from './onboarding-state.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  checkFullDiskAccess: vi.fn(() => Promise.resolve(false)),
  getMacosMajorVersion: vi.fn(() => Promise.resolve(14)),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  startIndexingAfterFdaDecision: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  // StepAi pulls in the AI pipeline. None of these need real behaviour here;
  // the wizard test only cares about navigation + footer plumbing.
  startAiDownload: vi.fn(() => Promise.resolve()),
  cancelAiDownload: vi.fn(() => Promise.resolve()),
  checkAiConnection: vi.fn(() => Promise.resolve({ connected: false, authError: false, models: [], error: null })),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKey: vi.fn(() => Promise.resolve('')),
  configureAi: vi.fn(() => Promise.resolve()),
  getAiRuntimeStatus: vi.fn(() =>
    Promise.resolve({
      serverRunning: false,
      serverStarting: false,
      pid: null,
      port: null,
      modelInstalled: false,
      modelName: 'Ministral 3B',
      modelSizeBytes: 0,
      modelSizeFormatted: '0 B',
      downloadInProgress: false,
      localAiSupported: true,
      kvBytesPerToken: 0,
      baseOverheadBytes: 0,
    }),
  ),
}))

vi.mock('$lib/settings/ai-config', () => ({
  pushConfigToBackend: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings-store', () => ({
  saveSettings: vi.fn(() => Promise.resolve()),
}))

// Force macOS in jsdom so the wizard renders step 1 (StepFda) instead of skipping it.
// Resume-rule behaviour on Linux is unit-tested in onboarding-state.test.ts.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, isMacOS: () => true }
})

function mountWizard(onComplete: () => void = () => {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(OnboardingWizard, { target, props: { onComplete } })
  return { target, instance }
}

function getPanel(target: HTMLElement): HTMLDivElement {
  const panel = target.querySelector<HTMLDivElement>('.wizard-panel')
  if (!panel) throw new Error('wizard panel not found')
  return panel
}

function primaryFooterButton(target: HTMLElement): HTMLButtonElement | null {
  const slot = target.querySelector<HTMLElement>('.primary-slot')
  if (!slot) return null
  return slot.querySelector<HTMLButtonElement>('button')
}

/** Returns the LAST button in the primary slot. On step 2 the wizard renders two */
/** footer buttons; the "advance" one ("One more optional setup step") is the last. */
function lastPrimaryFooterButton(target: HTMLElement): HTMLButtonElement | null {
  const slot = target.querySelector<HTMLElement>('.primary-slot')
  if (!slot) return null
  const buttons = slot.querySelectorAll<HTMLButtonElement>('button')
  return buttons[buttons.length - 1] ?? null
}

function backButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>('.back-button')
}

describe('OnboardingWizard', () => {
  let mounted: ReturnType<typeof mountWizard> | undefined

  beforeEach(() => {
    closeWizard()
    resetForTesting()
  })

  afterEach(async () => {
    if (mounted) {
      await unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
    closeWizard()
    resetForTesting()
  })

  it('step 1 in decide mode hides the footer primary button (body owns Allow/Deny)', async () => {
    mounted = mountWizard()
    await tick()
    expect(getOnboardingState().currentStep).toBe(1)
    expect(getOnboardingState().step1Variant).toBe('first-ask')
    expect(getOnboardingState().step1FooterMode).toBe('decide')
    expect(primaryFooterButton(mounted.target)).toBeNull()
    expect(backButton(mounted.target)).toBeNull()
  })

  it('step 1 `already-granted` shows a Next footer button and advances to step 2', async () => {
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    const next = primaryFooterButton(mounted.target)
    expect(next?.textContent.trim()).toBe('Next')
    next?.click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('step 1 in `restart` footer mode shows a Restart Cmdr button that calls relaunch()', async () => {
    openWizard('first-launch')
    setStep1Restart()
    mounted = mountWizard()
    await tick()
    const btn = primaryFooterButton(mounted.target)
    expect(btn?.textContent.trim()).toBe('Restart Cmdr')
    btn?.click()
    flushSync()
    await tick()
    const { relaunch } = await import('@tauri-apps/plugin-process')
    expect(relaunch).toHaveBeenCalledOnce()
  })

  it('Finish on the last step fires onComplete', async () => {
    const onComplete = vi.fn()
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard(onComplete)
    await tick()
    // Step 1 → step 2 via wizard's default Next.
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    // Step 2 owns its footer override: [Start (secondary), One more (primary)]. Click
    // the LAST button to advance to step 3 without skipping. Allow microtasks for the
    // step-2 persist + nextStep() chain to settle.
    lastPrimaryFooterButton(mounted.target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
    expect(getOnboardingState().currentStep).toBe(3)
    // Step 3 registers its own footer override ("Start using Cmdr") via setFooterOverride().
    // The wizard's built-in "Finish" label only renders for steps that don't override.
    expect(primaryFooterButton(mounted.target)?.textContent.trim()).toBe('Start using Cmdr')
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    expect(onComplete).toHaveBeenCalledOnce()
  })

  it('step 2 "Start using Cmdr!" requests wizard finish (skips step 3)', async () => {
    const onComplete = vi.fn()
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard(onComplete)
    await tick()
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    // Step 2 footer's FIRST button is "Start using Cmdr!" (secondary).
    primaryFooterButton(mounted.target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
    expect(onComplete).toHaveBeenCalledOnce()
    // Should NOT have advanced to step 3: the finish request short-circuits.
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('Back from step 2 resets the footer mode to `decide` (Allow/Deny live again)', async () => {
    openWizard('first-launch')
    setCurrentStep(2)
    mounted = mountWizard()
    await tick()
    backButton(mounted.target)?.click()
    flushSync()
    await tick()
    expect(getOnboardingState().currentStep).toBe(1)
    expect(getOnboardingState().step1FooterMode).toBe('decide')
  })

  it('swallows Escape: pressing it does not change the step or unmount', async () => {
    mounted = mountWizard()
    await tick()
    const initialStep = getOnboardingState().currentStep
    const panel = getPanel(mounted.target)
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    panel.dispatchEvent(event)
    flushSync()
    expect(event.defaultPrevented).toBe(true)
    expect(getOnboardingState().currentStep).toBe(initialStep)
    expect(mounted.target.querySelector('.wizard-panel')).not.toBeNull()
  })

  // Focus trap wrap-around. We assert structurally (first ↔ last focusable) rather than
  // pinning specific elements (Back vs Finish vs a switch input) because step content
  // changes the focusable set as steps evolve (step 3 now includes <SettingSwitch>
  // checkbox inputs that sort last in DOM order). The wizard's job is to wrap; whether
  // the bookend happens to be a button or an input is incidental.
  const FOCUSABLE_SELECTOR =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

  function focusables(panel: HTMLElement): HTMLElement[] {
    return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
  }

  async function advanceToStep3(target: HTMLElement): Promise<void> {
    // Step 1 → step 2.
    primaryFooterButton(target)?.click()
    flushSync()
    await tick()
    // Step 2 → step 3 (use LAST footer button to advance, not Start-using-Cmdr).
    lastPrimaryFooterButton(target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
  }

  it('wraps Tab from the last focusable back to the first', async () => {
    // Force `already-granted` so step 1 has a single Next footer button, then advance
    // to step 3 to exercise the full focusable set (Back + body controls + footer
    // primary).
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    await advanceToStep3(mounted.target)
    const panel = getPanel(mounted.target)
    const items = focusables(panel)
    expect(items.length).toBeGreaterThanOrEqual(2)
    const first = items[0]
    const last = items[items.length - 1]
    last.focus()
    expect(document.activeElement).toBe(last)
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    panel.dispatchEvent(tab)
    flushSync()
    expect(tab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(first)
  })

  it('wraps Shift+Tab from the first focusable back to the last', async () => {
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    await advanceToStep3(mounted.target)
    const panel = getPanel(mounted.target)
    const items = focusables(panel)
    expect(items.length).toBeGreaterThanOrEqual(2)
    const first = items[0]
    const last = items[items.length - 1]
    first.focus()
    expect(document.activeElement).toBe(first)
    const shiftTab = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })
    panel.dispatchEvent(shiftTab)
    flushSync()
    expect(shiftTab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(last)
  })
})
