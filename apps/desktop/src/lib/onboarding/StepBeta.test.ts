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
 *
 * Axe coverage lives in `StepBeta.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'

// `vi.mock` calls are hoisted above module-level `const`s, so any value a factory closes
// over must come from `vi.hoisted` (which runs first). The settings map + spies live here.
const { betaSignupMock, settingsMap, setSetting } = vi.hoisted(() => {
  const settingsMap: Record<string, unknown> = {
    'analytics.enabled': true,
    'analytics.email': '',
  }
  return {
    betaSignupMock: vi.fn(() => Promise.resolve({ kind: 'subscribed' as const })),
    settingsMap,
    // `setSetting` mutates the map AND records the call so we can assert which ids got written.
    setSetting: vi.fn((id: string, value: unknown) => {
      settingsMap[id] = value
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
  }
})

vi.mock('$lib/settings/settings-store', () => ({
  onSpecificSettingChange: () => () => {},
}))

import StepBeta from './StepBeta.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep, getOnboardingState } from './onboarding-state.svelte'

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
  const input = target.querySelector<HTMLInputElement>('input.email-input')
  if (!input) throw new Error('Email input missing')
  return input
}

describe('StepBeta', () => {
  let mounted: ReturnType<typeof mountStep> | undefined

  beforeEach(() => {
    settingsMap['analytics.enabled'] = true
    settingsMap['analytics.email'] = ''
    setSetting.mockClear()
    betaSignupMock.mockClear()
    betaSignupMock.mockResolvedValue({ kind: 'subscribed' as const })
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
    const control = mounted.target.querySelector<HTMLElement>('[aria-label="Send anonymous usage stats"]')
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
