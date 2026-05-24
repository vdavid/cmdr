/**
 * Tier 3 axe-based a11y tests for the M2 onboarding wizard.
 *
 * Asserts axe-clean structure for each reachable wizard state. Step 1 has three variants
 * (first-ask, revoked, already-granted) and two footer modes (decide, restart); we exercise
 * the ones that change visible structure. Steps 2 and 3 are still stubs (M3/M4 ship them),
 * so we only assert their default a11y shape.
 *
 * Focus trap + Escape-swallowing behaviour live in `OnboardingWizard.test.ts`. Per-step
 * a11y lives in `StepFda.a11y.test.ts` (M2) and the step-2/3 a11y files (M3/M4).
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import OnboardingWizard from './OnboardingWizard.svelte'
import {
  closeWizard,
  resetForTesting,
  openWizard,
  setStep1Variant,
  setStep1Restart,
  setCurrentStep,
} from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, isMacOS: () => true }
})

let lastInstance: ReturnType<typeof mount> | undefined
let lastTarget: HTMLDivElement | undefined

function mountAt(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  lastTarget = target
  lastInstance = mount(OnboardingWizard, { target, props: { onComplete: () => {} } })
  return target
}

afterEach(() => {
  if (lastInstance) {
    unmount(lastInstance)
    lastInstance = undefined
  }
  if (lastTarget) {
    lastTarget.remove()
    lastTarget = undefined
  }
  closeWizard()
  resetForTesting()
})

describe('OnboardingWizard a11y', () => {
  it('step 1 first-ask (decide mode) has no a11y violations', async () => {
    const target = mountAt()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 1 already-granted variant has no a11y violations', async () => {
    openWizard('menu')
    setStep1Variant('already-granted')
    const target = mountAt()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 1 in restart footer mode has no a11y violations', async () => {
    openWizard('first-launch')
    setStep1Restart()
    const target = mountAt()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 2 stub has no a11y violations', async () => {
    openWizard('first-launch')
    setCurrentStep(2)
    const target = mountAt()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 3 stub has no a11y violations', async () => {
    openWizard('first-launch')
    setCurrentStep(3)
    const target = mountAt()
    await tick()
    flushSync()
    await expectNoA11yViolations(target)
  })
})
