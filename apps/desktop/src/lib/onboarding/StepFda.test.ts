/**
 * Behaviour tests for `StepFda.svelte`.
 *
 * Covers the three variants (first-ask, revoked, already-granted), the Allow path
 * (re-probe → persist → openPrivacySettings → flip footer to restart), the Deny path
 * (persist → startIndexingAfterFdaDecision → advance to step 2), and the macOS-version
 * branch in the "find Cmdr in the list" copy.
 */

import { describe, it, expect, vi, beforeEach, afterEach, onTestFinished } from 'vitest'
import { withCapabilities, withPlatform } from '#lib/platform/test-platform.js'
import { mount, tick, unmount, flushSync } from 'svelte'
import StepFda from './StepFda.svelte'
import {
  closeWizard,
  resetForTesting,
  openWizard,
  setStep1Variant,
  getOnboardingState,
} from './onboarding-state.svelte'

const checkFullDiskAccess = vi.fn<() => Promise<boolean>>(() => Promise.resolve(false))
const checkFullDiskAccessQuiet = vi.fn<() => Promise<boolean>>(() => Promise.resolve(false))
const getMacosMajorVersion = vi.fn<() => Promise<number>>(() => Promise.resolve(14))
const openPrivacySettings = vi.fn<() => Promise<void>>(() => Promise.resolve())
const startIndexingAfterFdaDecision = vi.fn<() => Promise<void>>(() => Promise.resolve())
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const setSetting = vi.fn<(id: string, value: unknown) => void>()
const forceSave = vi.fn<() => Promise<boolean>>(() => Promise.resolve(true))

vi.mock('#lib/tauri-commands/index.js', () => ({
  checkFullDiskAccess: () => checkFullDiskAccess(),
  checkFullDiskAccessQuiet: () => checkFullDiskAccessQuiet(),
  getMacosMajorVersion: () => getMacosMajorVersion(),
  openPrivacySettings: () => openPrivacySettings(),
  startIndexingAfterFdaDecision: () => startIndexingAfterFdaDecision(),
  openExternalUrl: (url: string) => openExternalUrl(url),
}))

vi.mock('#lib/settings/index.js', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    setSetting: (id: string, value: unknown) => {
      setSetting(id, value)
    },
    // Overridden, not spread through: the real one reaches the plugin store.
    forceSave: () => forceSave(),
  }
})

// Without the `fullDiskAccess` capability, Vitest's default, the safety net inside `StepFda.svelte` would return
// null and leave us nothing to assert against. Run on macOS with it; the no-FDA flow is enforced by the
// wizard's resume rule, not by the step body, and the resume-rule unit tests live in
// onboarding-state.test.ts. The Linux block at the bottom takes it away for its own tests.
vi.hoisted(() => {
  vi.stubGlobal('__CMDR_PLATFORM__', 'macos')
  vi.stubGlobal('__CMDR_CAPABILITIES__', { fullDiskAccess: true })
})

function mountStep() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepFda, { target, props: {} })
  return { target, instance }
}

function findButton(target: HTMLElement, label: string): HTMLButtonElement | null {
  return (
    Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent.trim() === label) ?? null
  )
}

function findButtonContaining(target: HTMLElement, fragment: string): HTMLButtonElement | null {
  return (
    Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent.includes(fragment)) ??
    null
  )
}

describe('StepFda', () => {
  let mounted: ReturnType<typeof mountStep> | undefined

  beforeEach(() => {
    closeWizard()
    resetForTesting()
    openWizard('first-launch')
    checkFullDiskAccess.mockClear()
    checkFullDiskAccessQuiet.mockClear()
    getMacosMajorVersion.mockClear()
    openPrivacySettings.mockClear()
    startIndexingAfterFdaDecision.mockClear()
    openExternalUrl.mockClear()
    setSetting.mockClear()
    forceSave.mockClear()
    checkFullDiskAccess.mockResolvedValue(false)
    checkFullDiskAccessQuiet.mockResolvedValue(false)
    getMacosMajorVersion.mockResolvedValue(14)
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

  it('first-ask variant opens on the apology and the decision, pros and cons folded away', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent).toContain('Welcome to Cmdr!')
    expect(mounted.target.textContent).toContain('Sorry to bother you with this first')
    // The reasoning is a click away, not on screen: a first launch shouldn't open on it.
    expect(mounted.target.textContent).not.toContain('Pro:')
    expect(findButtonContaining(mounted.target, 'Open')).not.toBeNull()
    expect(findButton(mounted.target, 'Deny')).not.toBeNull()
  })

  it('"Why?" unfolds the explanation and the pros-and-cons grid', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    const why = findButton(mounted.target, 'Why?')
    expect(why?.getAttribute('aria-expanded')).toBe('false')
    why?.click()
    flushSync()
    expect(why?.getAttribute('aria-expanded')).toBe('true')
    expect(mounted.target.textContent).toContain('Cmdr is a file manager')
    expect(mounted.target.textContent).toContain('Pro:')
    expect(mounted.target.textContent).toContain('Con:')
  })

  it('revoked variant renders the "previously revoked" framing', async () => {
    setStep1Variant('revoked')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent).toContain('granted full disk access before but then turned it off')
    expect(findButtonContaining(mounted.target, 'Open')).not.toBeNull()
    expect(findButton(mounted.target, 'Deny')).not.toBeNull()
  })

  it('already-granted variant renders the single-line copy and no Allow/Deny', async () => {
    setStep1Variant('already-granted')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent).toContain('Cmdr currently has full disk access')
    expect(findButtonContaining(mounted.target, 'Open')).toBeNull()
    expect(findButton(mounted.target, 'Deny')).toBeNull()
  })

  it('Allow re-probes TCC before opening Settings, persists allow, and flips footer to restart', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    const allow = findButtonContaining(mounted.target, 'Open')
    if (!allow) throw new Error('Allow button missing')
    allow.click()
    // handleAllow has multiple awaits; flush microtasks repeatedly to let each Promise resolve.
    for (let i = 0; i < 10; i++) {
      await Promise.resolve()
    }
    flushSync()
    expect(checkFullDiskAccess).toHaveBeenCalled()
    expect(setSetting).toHaveBeenCalledWith('onboarding.fullDiskAccessChoice', 'allow')
    expect(forceSave).toHaveBeenCalled()
    expect(openPrivacySettings).toHaveBeenCalled()
    expect(getOnboardingState().step1FooterMode).toBe('restart')
    expect(getOnboardingState().currentStep).toBe(1)
    // The post-action hint appears once the flip has happened.
    expect(mounted.target.textContent).toContain('Cmdr needs to restart')
  })

  it('Allow shows the way to Full Disk Access when System Settings didn’t open', async () => {
    openPrivacySettings.mockRejectedValueOnce(new Error('`open` exited with 1'))
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    const allow = findButtonContaining(mounted.target, 'Open')
    if (!allow) throw new Error('Allow button missing')
    allow.click()
    for (let i = 0; i < 10; i++) {
      await Promise.resolve()
    }
    flushSync()
    const alert = mounted.target.querySelector('[role="alert"]')
    expect(alert?.textContent).toContain('Privacy & Security')
    expect(alert?.textContent).toContain('Full Disk Access')
    expect(alert?.textContent).not.toContain('exited')
    // The person can still grant it by hand, and a restart is still what makes the grant take effect.
    expect(getOnboardingState().step1FooterMode).toBe('restart')
  })

  it('Deny persists deny, fires startIndexingAfterFdaDecision, and advances to step 2', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    const deny = findButton(mounted.target, 'Deny')
    if (!deny) throw new Error('Deny button missing')
    deny.click()
    for (let i = 0; i < 10; i++) {
      await Promise.resolve()
    }
    flushSync()
    expect(setSetting).toHaveBeenCalledWith('onboarding.fullDiskAccessChoice', 'deny')
    expect(forceSave).toHaveBeenCalled()
    expect(startIndexingAfterFdaDecision).toHaveBeenCalledOnce()
    expect(getOnboardingState().currentStep).toBe(2)
    expect(getOnboardingState().stepTwoBanner).toBe('denied')
  })

  it('macOS 12 (pre-Ventura) shows the "end of the list" wording', async () => {
    getMacosMajorVersion.mockResolvedValue(12)
    setStep1Variant('first-ask')
    mounted = mountStep()
    // Give the onMount await a chance to resolve.
    await tick()
    await Promise.resolve()
    await tick()
    await tick()
    expect(mounted.target.textContent).toContain('at the end of the list')
  })

  it('macOS 13+ (Ventura+) shows the alphabetical wording', async () => {
    getMacosMajorVersion.mockResolvedValue(14)
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    await Promise.resolve()
    await tick()
    expect(mounted.target.textContent).toContain('Find Cmdr in the list')
  })

  // The poller fires once immediately on mount (so an already-in-place grant is caught
  // fast), then on the 500 ms interval. These tests drive the immediate-poll path, which
  // doesn't depend on the interval ticking.
  it('polls the quiet FDA probe on mount when not granted', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    expect(checkFullDiskAccessQuiet).toHaveBeenCalled()
  })

  it('switches to the success state and flips the footer to restart when the probe flips to granted', async () => {
    checkFullDiskAccessQuiet.mockResolvedValue(true)
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    flushSync()
    expect(mounted.target.textContent).toContain('You granted full disk access!')
    expect(getOnboardingState().step1Granted).toBe(true)
    expect(getOnboardingState().step1FooterMode).toBe('restart')
  })

  it('does not poll on the already-granted variant', async () => {
    setStep1Variant('already-granted')
    mounted = mountStep()
    await tick()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    expect(checkFullDiskAccessQuiet).not.toHaveBeenCalled()
  })

  it('stops polling after the unmount cleanup runs', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    for (let i = 0; i < 10; i++) await Promise.resolve()
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
    checkFullDiskAccessQuiet.mockClear()
    // After unmount the interval is cleared, so even after the poll cadence elapses the
    // quiet probe is never called again.
    await new Promise((resolve) => setTimeout(resolve, 700))
    expect(checkFullDiskAccessQuiet).not.toHaveBeenCalled()
  })
})

// Linux: StepFda renders nothing and the poller never runs.
describe('StepFda on Linux', () => {
  beforeEach(() => {
    onTestFinished(withPlatform('linux'))
    onTestFinished(withCapabilities())
    closeWizard()
    resetForTesting()
    openWizard('first-launch')
    setStep1Variant('first-ask')
    checkFullDiskAccessQuiet.mockClear()
    checkFullDiskAccessQuiet.mockResolvedValue(true)
  })

  afterEach(() => {
    closeWizard()
    resetForTesting()
  })

  it('renders nothing and never polls', async () => {
    const { target, instance } = mountStep()
    await tick()
    for (let i = 0; i < 10; i++) await Promise.resolve()

    expect(target.textContent.trim()).toBe('')
    expect(checkFullDiskAccessQuiet).not.toHaveBeenCalled()

    await unmount(instance)
    target.remove()
  })
})
