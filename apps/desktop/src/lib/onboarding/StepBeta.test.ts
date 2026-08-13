/**
 * Behaviour tests for `StepBeta.svelte` (the "Open beta" onboarding page).
 *
 * Covers:
 * - The opt-out switch is the registry-backed `<SettingSwitch id="analytics.enabled">`:
 *   it reflects the current `analytics.enabled` value and writes the new value on flip,
 *   reusing the exact Settings wiring.
 * - Committing a valid email calls the typed `betaSignup` wrapper and renders the gentle
 *   success copy. An invalid address does not call `betaSignup`.
 * - The footer override registers two buttons: a secondary "Start using Cmdr!" that
 *   finishes onboarding here (skipping the Optional step) and a primary "One more optional
 *   setup step" that advances to the final Optional step. The override clears on destroy.
 * - Terms acceptance gates BOTH footer buttons, records the accepted version + timestamp,
 *   and a blocked click sends the user to the checkbox instead of silently doing nothing.
 *
 * Axe coverage lives in `StepBeta.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'

// `vi.mock` calls are hoisted above module-level `const`s, so any value a factory closes
// over must come from `vi.hoisted` (which runs first). The settings map + spies live here.
const { betaSignupMock, settingsMap, setSetting, forceSaveMock } = vi.hoisted(() => {
  const settingsMap: Record<string, unknown> = {
    'analytics.enabled': true,
    'analytics.email': '',
    // Where the terms acceptance lands, alongside every other setting.
    'onboarding.termsAcceptedVersion': '',
    'onboarding.termsAcceptedAt': '',
  }
  return {
    betaSignupMock: vi.fn(() => Promise.resolve({ kind: 'subscribed' as const })),
    settingsMap,
    // `setSetting` mutates the map AND records the call so we can assert which ids got written.
    setSetting: vi.fn((id: string, value: unknown) => {
      settingsMap[id] = value
    }),
    forceSaveMock: vi.fn(() => {
      return Promise.resolve(true)
    }),
  }
})

// Spread the real barrel and override only `betaSignup`, so other `$lib/tauri-commands`
// exports the mounted tree might reach stay intact (a barrel mock that drops them silently
// corrupts the Svelte 5 reactive graph; see `lib/ipc/CLAUDE.md` § Test-mock upkeep).
vi.mock('$lib/tauri-commands', async () => {
  const real = await vi.importActual<typeof import('$lib/tauri-commands')>('$lib/tauri-commands')
  return { ...real, betaSignup: betaSignupMock }
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      setSetting(id, value)
    },
    // Overridden, not spread through: the real one reaches the plugin store.
    forceSave: forceSaveMock,
  }
})

vi.mock('$lib/settings/settings-store', () => ({
  onSpecificSettingChange: () => () => {},
}))

import StepBeta from './StepBeta.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep, getOnboardingState } from './onboarding-state.svelte'
import { TERMS_VERSION, TERMS_URL } from '$lib/legal/terms'

function mountStep(): { target: HTMLElement; instance: ReturnType<typeof mount> } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepBeta, { target, props: {} })
  return { target, instance }
}

async function waitForAsync(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

function getEmailInput(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('input[type="email"]')
  if (!input) throw new Error('Email input missing')
  return input
}

function getTermsCheckbox(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('.terms-block input[type="checkbox"]')
  if (!input) throw new Error('Terms checkbox missing')
  return input
}

/** Ticks the terms checkbox the way a user would, then lets the reactive graph settle. */
async function acceptTerms(target: HTMLElement): Promise<void> {
  getTermsCheckbox(target).click()
  await waitForAsync()
}

describe('StepBeta', () => {
  let mounted: ReturnType<typeof mountStep> | undefined
  let scrollIntoViewSpy: Mock<(options?: boolean | ScrollIntoViewOptions) => void>

  beforeEach(() => {
    settingsMap['analytics.enabled'] = true
    settingsMap['analytics.email'] = ''
    settingsMap['onboarding.termsAcceptedVersion'] = ''
    settingsMap['onboarding.termsAcceptedAt'] = ''
    setSetting.mockClear()
    forceSaveMock.mockClear()
    betaSignupMock.mockClear()
    betaSignupMock.mockResolvedValue({ kind: 'subscribed' as const })
    // jsdom has no layout engine, so `scrollIntoView` doesn't exist. The blocked-click path
    // calls it; stub it so we can assert both that it ran and how it was asked to animate.
    scrollIntoViewSpy = vi.fn<(options?: boolean | ScrollIntoViewOptions) => void>()
    Element.prototype.scrollIntoView = scrollIntoViewSpy
    closeWizard()
    resetForTesting()
    openWizard('force')
    setCurrentStep(3)
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

  it('registers two footer buttons: secondary "Start using Cmdr!" and primary "One more optional setup step"', async () => {
    mounted = mountStep()
    await waitForAsync()
    const state = getOnboardingState()
    expect(state.footerOverride).not.toBeNull()
    expect(state.footerOverride).toHaveLength(2)
    expect(state.footerOverride?.[0].label).toBe('Start using Cmdr!')
    expect(state.footerOverride?.[0].variant).toBe('secondary')
    expect(state.footerOverride?.[1].label).toBe('One more optional setup step')
    expect(state.footerOverride?.[1].variant).toBe('primary')
  })

  it('"One more optional setup step" advances to the Optional step (step 4), it does not finish', async () => {
    mounted = mountStep()
    await waitForAsync()
    await acceptTerms(mounted.target)
    const state = getOnboardingState()
    const tickBefore = state.finishRequestTick
    state.footerOverride?.[1].onclick()
    await waitForAsync()
    expect(getOnboardingState().currentStep).toBe(4)
    expect(getOnboardingState().finishRequestTick).toBe(tickBefore)
  })

  it('"Start using Cmdr!" requests wizard completion, skipping the Optional step', async () => {
    mounted = mountStep()
    await waitForAsync()
    await acceptTerms(mounted.target)
    const state = getOnboardingState()
    const tickBefore = state.finishRequestTick
    state.footerOverride?.[0].onclick()
    await waitForAsync()
    // Stays on Beta (step 3); the wizard shell observes the bumped finish tick and closes.
    expect(getOnboardingState().currentStep).toBe(3)
    expect(getOnboardingState().finishRequestTick).toBe(tickBefore + 1)
  })

  it('clears the footer override on destroy', async () => {
    mounted = mountStep()
    await waitForAsync()
    expect(getOnboardingState().footerOverride).not.toBeNull()
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
    await waitForAsync()
    expect(getOnboardingState().footerOverride).toBeNull()
  })

  it('the opt-out switch reflects analytics.enabled and writes the new value on flip', async () => {
    mounted = mountStep()
    await waitForAsync()
    setSetting.mockClear()

    // The switch's aria-label mirrors the registry label.
    const control = mounted.target.querySelector<HTMLElement>('[aria-label="Send usage stats"]')
    expect(control).not.toBeNull()
    control?.click()
    await waitForAsync()

    expect(setSetting).toHaveBeenCalledWith('analytics.enabled', false)
  })

  it('commits a valid email through betaSignup and shows the success copy', async () => {
    mounted = mountStep()
    await waitForAsync()
    const input = getEmailInput(mounted.target)
    input.value = 'tester@example.com'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await waitForAsync()

    expect(setSetting).toHaveBeenCalledWith('analytics.email', 'tester@example.com')
    expect(betaSignupMock).toHaveBeenCalledWith('tester@example.com')
    expect(mounted.target.textContent).toContain('Check your inbox to confirm your email')
  })

  it('renders the GitHub-stars CTA linking the repo (helps Cmdr reach Homebrew)', async () => {
    mounted = mountStep()
    await waitForAsync()
    const link = mounted.target.querySelector<HTMLAnchorElement>('a[href="https://github.com/vdavid/cmdr"]')
    expect(link).not.toBeNull()
    expect(link?.textContent).toContain('here on GitHub')
    // The CTA sentence names the star ask around the link (the fork/watch claim was dropped).
    expect(mounted.target.textContent).toContain('star the repo')
  })

  it('marks the terms checkbox required for screen readers, not only with the asterisk', async () => {
    mounted = mountStep()
    await waitForAsync()
    const checkbox = getTermsCheckbox(mounted.target)
    expect(checkbox.getAttribute('aria-required')).toBe('true')
    // The red asterisk is decoration on top of `aria-required`, so it stays out of the a11y tree.
    const mark = mounted.target.querySelector('.terms-block .required-mark')
    expect(mark?.getAttribute('aria-hidden')).toBe('true')
  })

  it('links the public terms page and opens it externally rather than navigating in-app', async () => {
    mounted = mountStep()
    await waitForAsync()
    const link = mounted.target.querySelector<HTMLAnchorElement>(`.terms-block a[href="${TERMS_URL}"]`)
    expect(link).not.toBeNull()
    expect(link?.textContent).toContain('terms and conditions')
  })

  it('blocks both footer buttons until the terms are accepted, then unblocks them', async () => {
    mounted = mountStep()
    await waitForAsync()
    const blockedBefore = getOnboardingState().footerOverride?.map((b) => b.blockedReason)
    expect(blockedBefore?.[0]).toBe('Accept the terms and conditions to continue.')
    expect(blockedBefore?.[1]).toBe('Accept the terms and conditions to continue.')

    await acceptTerms(mounted.target)

    const blockedAfter = getOnboardingState().footerOverride?.map((b) => b.blockedReason)
    expect(blockedAfter?.[0]).toBeUndefined()
    expect(blockedAfter?.[1]).toBeUndefined()
  })

  it('clicking a blocked footer button scrolls the terms checkbox into view and focuses it', async () => {
    mounted = mountStep()
    await waitForAsync()
    const state = getOnboardingState()
    const tickBefore = state.finishRequestTick

    state.footerOverride?.[1].onclick()
    await waitForAsync()

    // The click explains itself instead of advancing.
    expect(getOnboardingState().currentStep).toBe(3)
    expect(getOnboardingState().finishRequestTick).toBe(tickBefore)
    expect(scrollIntoViewSpy).toHaveBeenCalledTimes(1)
    expect(scrollIntoViewSpy.mock.calls[0]?.[0]).toMatchObject({ block: 'center', behavior: 'smooth' })
    expect(document.activeElement).toBe(getTermsCheckbox(mounted.target))
  })

  it('skips the scroll animation under prefers-reduced-motion', async () => {
    const matchMedia = vi.fn((query: string) => ({ matches: query.includes('reduced-motion'), media: query }))
    vi.stubGlobal('matchMedia', matchMedia)
    try {
      mounted = mountStep()
      await waitForAsync()
      getOnboardingState().footerOverride?.[1].onclick()
      await waitForAsync()
      expect(scrollIntoViewSpy.mock.calls[0]?.[0]).toMatchObject({ behavior: 'auto' })
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('the blocked "Start using Cmdr!" button also routes to the checkbox instead of finishing', async () => {
    mounted = mountStep()
    await waitForAsync()
    const tickBefore = getOnboardingState().finishRequestTick

    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()

    expect(getOnboardingState().finishRequestTick).toBe(tickBefore)
    expect(document.activeElement).toBe(getTermsCheckbox(mounted.target))
  })

  it('persists the accepted terms version and the acceptance timestamp', async () => {
    mounted = mountStep()
    await waitForAsync()
    setSetting.mockClear()

    await acceptTerms(mounted.target)

    expect(setSetting).toHaveBeenCalledWith('onboarding.termsAcceptedVersion', TERMS_VERSION)
    const acceptedAt = settingsMap['onboarding.termsAcceptedAt'] as string
    // An ISO 8601 instant, so a later dispute can name the moment, not just the day.
    expect(new Date(acceptedAt).toISOString()).toBe(acceptedAt)
    // Consent doesn't wait out the 500 ms save debounce.
    expect(forceSaveMock).toHaveBeenCalled()
  })

  it('clearing the checkbox clears the stored acceptance', async () => {
    mounted = mountStep()
    await waitForAsync()
    await acceptTerms(mounted.target)
    setSetting.mockClear()

    await acceptTerms(mounted.target)

    expect(setSetting).toHaveBeenCalledWith('onboarding.termsAcceptedVersion', '')
    expect(setSetting).toHaveBeenCalledWith('onboarding.termsAcceptedAt', '')
    expect(getOnboardingState().footerOverride?.[0].blockedReason).toBe('Accept the terms and conditions to continue.')
  })

  it('pre-checks the box when the CURRENT terms version was already accepted', async () => {
    settingsMap['onboarding.termsAcceptedVersion'] = TERMS_VERSION
    settingsMap['onboarding.termsAcceptedAt'] = '2026-08-10T09:00:00.000Z'
    mounted = mountStep()
    await waitForAsync()

    expect(getTermsCheckbox(mounted.target).checked).toBe(true)
    expect(getOnboardingState().footerOverride?.[0].blockedReason).toBeUndefined()
  })

  it('asks again when the stored acceptance names an older terms version', async () => {
    settingsMap['onboarding.termsAcceptedVersion'] = '2020-01-01'
    settingsMap['onboarding.termsAcceptedAt'] = '2020-01-01T09:00:00.000Z'
    mounted = mountStep()
    await waitForAsync()

    expect(getTermsCheckbox(mounted.target).checked).toBe(false)
    expect(getOnboardingState().footerOverride?.[0].blockedReason).toBe('Accept the terms and conditions to continue.')
  })

  it('does not call betaSignup for an invalid email', async () => {
    mounted = mountStep()
    await waitForAsync()
    const input = getEmailInput(mounted.target)
    input.value = 'not-an-email'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await waitForAsync()

    expect(betaSignupMock).not.toHaveBeenCalled()
  })
})
