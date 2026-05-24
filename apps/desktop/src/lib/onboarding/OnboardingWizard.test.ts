/**
 * Behaviour tests for the onboarding wizard shell (M2).
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
  const actual = (await importOriginal()) as Record<string, unknown>
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

function backButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>('.back-button')
}

describe('OnboardingWizard', () => {
  let mounted: ReturnType<typeof mountWizard> | undefined

  beforeEach(() => {
    closeWizard()
    resetForTesting()
  })

  afterEach(() => {
    if (mounted) {
      unmount(mounted.instance)
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
    expect(next?.textContent?.trim()).toBe('Next')
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
    expect(btn?.textContent?.trim()).toBe('Restart Cmdr')
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
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(3)
    expect(primaryFooterButton(mounted.target)?.textContent?.trim()).toBe('Finish')
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    expect(onComplete).toHaveBeenCalledOnce()
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

  it('wraps Tab from the last focusable back to the first', async () => {
    // Force `already-granted` so step 1 has a single Next footer button, then advance
    // to step 2 where both Back and Next are visible.
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    const panel = getPanel(mounted.target)
    const back = backButton(mounted.target)
    const next = primaryFooterButton(mounted.target)
    if (!back || !next) throw new Error('back + next must exist on step 2')
    next.focus()
    expect(document.activeElement).toBe(next)
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    panel.dispatchEvent(tab)
    flushSync()
    expect(tab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(back)
  })

  it('wraps Shift+Tab from the first focusable back to the last', async () => {
    openWizard('menu')
    setStep1Variant('already-granted')
    mounted = mountWizard()
    await tick()
    primaryFooterButton(mounted.target)?.click()
    flushSync()
    await tick()
    const panel = getPanel(mounted.target)
    const back = backButton(mounted.target)
    const next = primaryFooterButton(mounted.target)
    if (!back || !next) throw new Error('back + next must exist on step 2')
    back.focus()
    expect(document.activeElement).toBe(back)
    const shiftTab = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })
    panel.dispatchEvent(shiftTab)
    flushSync()
    expect(shiftTab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(next)
  })
})
