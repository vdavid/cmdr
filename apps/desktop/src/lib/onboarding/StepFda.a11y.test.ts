/**
 * Tier 3 axe a11y tests for `StepFda.svelte`.
 *
 * One test per variant. Tier-3 a11y is structural (ARIA, labels, focusables); axe runs
 * in jsdom against the mounted component. Focus management and Escape behaviour live
 * in `OnboardingWizard.test.ts`.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import StepFda from './StepFda.svelte'
import { closeWizard, resetForTesting, openWizard, setStep1Granted, setStep1Variant } from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  checkFullDiskAccess: vi.fn(() => Promise.resolve(false)),
  checkFullDiskAccessQuiet: vi.fn(() => Promise.resolve(false)),
  getMacosMajorVersion: vi.fn(() => Promise.resolve(14)),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  startIndexingAfterFdaDecision: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings-store', () => ({
  saveSettings: vi.fn(() => Promise.resolve()),
}))

// Same reason as in `StepFda.test.ts`: jsdom isn't macOS so the safety-net guard would
// short-circuit the render. Resume-rule platform logic is unit-tested separately.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, isMacOS: () => true }
})

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

function mountStep() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepFda, { target, props: {} })
  mounted = { target, instance }
  return target
}

beforeEach(() => {
  closeWizard()
  resetForTesting()
  openWizard('first-launch')
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

describe('StepFda a11y', () => {
  it('first-ask variant has no a11y violations', async () => {
    setStep1Variant('first-ask')
    const target = mountStep()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('revoked variant has no a11y violations', async () => {
    setStep1Variant('revoked')
    const target = mountStep()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('already-granted variant has no a11y violations', async () => {
    setStep1Variant('already-granted')
    const target = mountStep()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('grant-detected success state has no a11y violations', async () => {
    setStep1Variant('first-ask')
    setStep1Granted()
    const target = mountStep()
    await tick()
    await expectNoA11yViolations(target)
  })
})
