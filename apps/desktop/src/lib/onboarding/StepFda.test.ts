/**
 * Behaviour tests for `StepFda.svelte` (M2).
 *
 * Covers the three variants (first-ask, revoked, already-granted), the Allow path
 * (re-probe → persist → openPrivacySettings → flip footer to restart), the Deny path
 * (persist → startIndexingAfterFdaDecision → advance to step 2), and the macOS-version
 * branch in the "find Cmdr in the list" copy.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
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
const getMacosMajorVersion = vi.fn<() => Promise<number>>(() => Promise.resolve(14))
const openPrivacySettings = vi.fn<() => Promise<void>>(() => Promise.resolve())
const startIndexingAfterFdaDecision = vi.fn<() => Promise<void>>(() => Promise.resolve())
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const saveSettings = vi.fn<(s: unknown) => Promise<void>>(() => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  checkFullDiskAccess: () => checkFullDiskAccess(),
  getMacosMajorVersion: () => getMacosMajorVersion(),
  openPrivacySettings: () => openPrivacySettings(),
  startIndexingAfterFdaDecision: () => startIndexingAfterFdaDecision(),
  openExternalUrl: (url: string) => openExternalUrl(url),
}))

vi.mock('$lib/settings-store', () => ({
  saveSettings: (settings: unknown) => saveSettings(settings),
}))

// jsdom's userAgent doesn't contain "mac" by default, so the Linux safety net inside
// `StepFda.svelte` would return null and leave us nothing to assert against. Pretend
// we're on macOS for these tests; Linux flow is enforced by the wizard's resume rule,
// not by the step body, and the resume-rule unit tests live in onboarding-state.test.ts.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, isMacOS: () => true }
})

function mountStep() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepFda, { target, props: {} })
  return { target, instance }
}

function findButton(target: HTMLElement, label: string): HTMLButtonElement | null {
  return (
    Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label) ??
    null
  )
}

function findButtonContaining(target: HTMLElement, fragment: string): HTMLButtonElement | null {
  return (
    Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find((b) =>
      (b.textContent ?? '').includes(fragment),
    ) ?? null
  )
}

describe('StepFda', () => {
  let mounted: ReturnType<typeof mountStep> | undefined

  beforeEach(() => {
    closeWizard()
    resetForTesting()
    openWizard('first-launch')
    checkFullDiskAccess.mockClear()
    getMacosMajorVersion.mockClear()
    openPrivacySettings.mockClear()
    startIndexingAfterFdaDecision.mockClear()
    openExternalUrl.mockClear()
    saveSettings.mockClear()
    checkFullDiskAccess.mockResolvedValue(false)
    getMacosMajorVersion.mockResolvedValue(14)
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

  it('first-ask variant renders the welcome + pros/cons + Allow + Deny', async () => {
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent ?? '').toContain('Welcome to Cmdr!')
    expect(mounted.target.textContent ?? '').toContain('full disk access')
    expect(findButtonContaining(mounted.target, 'Open')).not.toBeNull()
    expect(findButton(mounted.target, 'Deny')).not.toBeNull()
  })

  it('revoked variant renders the "previously revoked" framing', async () => {
    setStep1Variant('revoked')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent ?? '').toContain('accepted full disk access before but then revoked it')
    expect(findButtonContaining(mounted.target, 'Open')).not.toBeNull()
    expect(findButton(mounted.target, 'Deny')).not.toBeNull()
  })

  it('already-granted variant renders the single-line copy and no Allow/Deny', async () => {
    setStep1Variant('already-granted')
    mounted = mountStep()
    await tick()
    expect(mounted.target.textContent ?? '').toContain('Cmdr currently has full disk access')
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
    expect(saveSettings).toHaveBeenCalledWith({ fullDiskAccessChoice: 'allow' })
    expect(openPrivacySettings).toHaveBeenCalled()
    expect(getOnboardingState().step1FooterMode).toBe('restart')
    expect(getOnboardingState().currentStep).toBe(1)
    // The post-action hint appears once the flip has happened.
    expect(mounted.target.textContent ?? '').toContain('Cmdr needs to restart')
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
    expect(saveSettings).toHaveBeenCalledWith({ fullDiskAccessChoice: 'deny' })
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
    expect(mounted.target.textContent ?? '').toContain('at the end of the list')
  })

  it('macOS 13+ (Ventura+) shows the alphabetical wording', async () => {
    getMacosMajorVersion.mockResolvedValue(14)
    setStep1Variant('first-ask')
    mounted = mountStep()
    await tick()
    await Promise.resolve()
    await tick()
    expect(mounted.target.textContent ?? '').toContain('Find Cmdr in the list')
  })
})
