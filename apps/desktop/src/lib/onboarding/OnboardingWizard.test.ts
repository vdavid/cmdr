/**
 * Behaviour tests for the onboarding wizard shell.
 *
 * Covers:
 * - Escape is a no-op (the wizard intentionally swallows it).
 * - The step-1 footer button reflects state: hidden in `decide` mode, `Restart Cmdr` in
 *   `restart` mode, `Next` for the `already-granted` variant.
 * - Forward navigation walks step 1 → 2 (AI) → 3 (Beta) → 4 (Optional) → Finish fires
 *   `onComplete`. The AI step's "Next" routes to the non-skippable Beta page, never
 *   straight to completion. The Beta page offers "Start using Cmdr!" (finish here) and
 *   "One more optional setup step" (continue to Optional).
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
  // StepBeta calls `betaSignup` on email commit; the wizard test never commits an email,
  // but the import must resolve.
  betaSignup: vi.fn(() => Promise.resolve({ kind: 'subscribed' })),
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

function footerButtonByLabel(target: HTMLElement, label: string): HTMLButtonElement | null {
  const slot = target.querySelector<HTMLElement>('.primary-slot')
  if (!slot) return null
  return (
    Array.from(slot.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent.trim() === label) ?? null
  )
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

  it('Finish on the last step fires onComplete after walking 1 → 2 → 3 → 4', async () => {
    const onComplete = vi.fn()
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard(onComplete)
    await tick()
    // Step 1 → step 2 via wizard's default Next.
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    // Step 2 (AI) owns a single "Next" forward button. Click it to land on the Beta page
    // (step 3). Allow microtasks for the persist + nextStep() chain to settle.
    expect(primaryFooterButton(mounted.target)?.textContent.trim()).toBe('Next')
    primaryFooterButton(mounted.target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
    expect(getOnboardingState().currentStep).toBe(3)
    // Step 3 (Beta) owns two buttons. "One more optional setup step" continues to Optional (step 4).
    expect(footerButtonByLabel(mounted.target, 'One more optional setup step')).not.toBeNull()
    footerButtonByLabel(mounted.target, 'One more optional setup step')?.click()
    flushSync()
    await tick()
    expect(getOnboardingState().currentStep).toBe(4)
    // Step 4 (Optional) registers its own footer override ("Start using Cmdr"). The
    // wizard's built-in "Finish" label only renders for steps that don't override.
    expect(primaryFooterButton(mounted.target)?.textContent.trim()).toBe('Start using Cmdr')
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    expect(onComplete).toHaveBeenCalledOnce()
  })

  it('the AI step\'s "Next" routes to the non-skippable Beta page (step 3), never straight to completion', async () => {
    const onComplete = vi.fn()
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard(onComplete)
    await tick()
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    // Click the AI step's single forward button.
    primaryFooterButton(mounted.target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
    // Lands on Beta (step 3); onComplete must NOT have fired.
    expect(getOnboardingState().currentStep).toBe(3)
    expect(onComplete).not.toHaveBeenCalled()
  })

  it('the Beta step\'s "Start using Cmdr!" finishes onboarding, skipping the Optional step', async () => {
    const onComplete = vi.fn()
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard(onComplete)
    await tick()
    // Step 1 → 2 (AI).
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    // Step 2 → 3 (Beta) via "Next".
    primaryFooterButton(mounted.target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
    expect(getOnboardingState().currentStep).toBe(3)
    // "Start using Cmdr!" finishes here without advancing to step 4.
    footerButtonByLabel(mounted.target, 'Start using Cmdr!')?.click()
    flushSync()
    expect(onComplete).toHaveBeenCalledOnce()
    expect(getOnboardingState().currentStep).toBe(3)
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
  // pinning specific elements (Back vs Next vs a switch input) because step content
  // changes the focusable set as steps evolve (the Beta page includes a <SettingSwitch>
  // checkbox plus an email input). The wizard's job is to wrap; whether the bookend
  // happens to be a button or an input is incidental.
  const FOCUSABLE_SELECTOR =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

  function focusables(panel: HTMLElement): HTMLElement[] {
    return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
  }

  async function advanceToBeta(target: HTMLElement): Promise<void> {
    // Step 1 → step 2 (AI).
    primaryFooterButton(target)?.click()
    flushSync()
    await tick()
    // Step 2 → step 3 (Beta) via the single "Next" forward button.
    primaryFooterButton(target)?.click()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    await tick()
  }

  it('wraps Tab from the last focusable back to the first', async () => {
    // Force `already-granted` so step 1 has a single Next footer button, then advance
    // to the Beta page to exercise the full focusable set (Back + switch + email input +
    // footer primary).
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    await advanceToBeta(mounted.target)
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
    await advanceToBeta(mounted.target)
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
