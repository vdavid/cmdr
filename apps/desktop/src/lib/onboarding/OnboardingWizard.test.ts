/**
 * Behaviour tests for the onboarding wizard skeleton (M1).
 *
 * Covers:
 * - Escape is a no-op (the wizard intentionally swallows it).
 * - Back button is hidden on the first step, appears on later steps.
 * - Next advances; Back retreats. Finish on the last step fires `onComplete`.
 * - The hand-rolled focus trap wraps Tab and Shift+Tab through the panel's focusables,
 *   re-querying on every keystroke so it picks up controls added mid-step.
 *
 * Axe-based a11y coverage lives in `OnboardingWizard.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import OnboardingWizard from './OnboardingWizard.svelte'
import { closeWizard, getOnboardingState, resetForTesting } from './onboarding-state.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

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

function nextButton(target: HTMLElement): HTMLButtonElement {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('button'))
  const next = buttons.find((b) => b.textContent?.trim() === 'Next' || b.textContent?.trim() === 'Finish')
  if (!next) throw new Error('primary button (Next/Finish) not found')
  return next
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

  it('opens at step 1 with no Back button visible', async () => {
    mounted = mountWizard()
    await tick()
    expect(getOnboardingState().currentStep).toBe(1)
    expect(backButton(mounted.target)).toBeNull()
    expect(nextButton(mounted.target).textContent?.trim()).toBe('Next')
  })

  it('advances through steps on Next and retreats on Back', async () => {
    mounted = mountWizard()
    await tick()

    nextButton(mounted.target).click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(2)
    expect(backButton(mounted.target)).not.toBeNull()

    nextButton(mounted.target).click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(3)
    expect(nextButton(mounted.target).textContent?.trim()).toBe('Finish')

    backButton(mounted.target)?.click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('calls onComplete when Finish is clicked on the last step', async () => {
    const onComplete = vi.fn()
    mounted = mountWizard(onComplete)
    await tick()

    nextButton(mounted.target).click()
    flushSync()
    nextButton(mounted.target).click()
    flushSync()
    expect(getOnboardingState().currentStep).toBe(3)

    nextButton(mounted.target).click()
    flushSync()
    expect(onComplete).toHaveBeenCalledOnce()
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
    mounted = mountWizard()
    await tick()
    // Step 1 only has the Next button (no Back on step 1), so we need to advance to a step
    // where multiple focusables exist. Advance to step 2: now we have Back + Next.
    nextButton(mounted.target).click()
    flushSync()
    await tick()

    const panel = getPanel(mounted.target)
    const back = backButton(mounted.target)
    const next = nextButton(mounted.target)
    if (!back) throw new Error('back button must exist on step 2')

    // Focus the last focusable (Next) and press Tab; expect focus to wrap to Back.
    next.focus()
    expect(document.activeElement).toBe(next)
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    panel.dispatchEvent(tab)
    flushSync()
    expect(tab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(back)
  })

  it('wraps Shift+Tab from the first focusable back to the last', async () => {
    mounted = mountWizard()
    await tick()
    nextButton(mounted.target).click()
    flushSync()
    await tick()

    const panel = getPanel(mounted.target)
    const back = backButton(mounted.target)
    const next = nextButton(mounted.target)
    if (!back) throw new Error('back button must exist on step 2')

    back.focus()
    expect(document.activeElement).toBe(back)
    const shiftTab = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })
    panel.dispatchEvent(shiftTab)
    flushSync()
    expect(shiftTab.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(next)
  })
})
