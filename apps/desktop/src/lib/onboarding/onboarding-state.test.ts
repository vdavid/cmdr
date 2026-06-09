/**
 * Unit tests for the onboarding state machine.
 *
 * Covers `resumeStepFor`, `step1VariantFor`, and `stepTwoBannerFor`: all pure logic
 * over the persisted-flag triple `(fullDiskAccessChoice, isOnboarded, hasFda)`. Platform
 * is passed in explicitly via `ctx.isMac` so jsdom's userAgent doesn't matter.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'

// Pretend we're on macOS so `previousStep` / `isAtFirstStep` / `openWizard(... null)`
// resolve to the macOS branches. Linux behaviour is exercised via the explicit `isMac`
// override on `ResumeContext`, not via the jsdom userAgent.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, isMacOS: () => true }
})

import {
  resumeStepFor,
  step1VariantFor,
  stepTwoBannerFor,
  resetForTesting,
  openWizard,
  setStep1Restart,
  setCurrentStep,
  nextStep,
  previousStep,
  isAtLastStep,
  requestWizardComplete,
  getOnboardingState,
  ONBOARDING_STEP_COUNT,
  type ResumeContext,
} from './onboarding-state.svelte'

const ctxMac = (overrides: Partial<ResumeContext>): ResumeContext => ({
  fullDiskAccessChoice: 'notAskedYet',
  isOnboarded: false,
  hasFda: false,
  isMac: true,
  ...overrides,
})

describe('resumeStepFor', () => {
  it('macOS: notAskedYet → step 1', () => {
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'notAskedYet' }))).toBe(1)
  })

  it('macOS: allow + !hasFda + isOnboarded (revoked-later) → step 1', () => {
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true, hasFda: false }))).toBe(1)
  })

  it('macOS: allow + hasFda → step 2', () => {
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: false, hasFda: true }))).toBe(2)
  })

  it('macOS: allow + !hasFda + !isOnboarded (first-time stuck) → step 2', () => {
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: false, hasFda: false }))).toBe(2)
  })

  it('macOS: deny → step 2', () => {
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'deny' }))).toBe(2)
  })

  it('Linux: always step 2', () => {
    expect(resumeStepFor({ ...ctxMac({}), isMac: false })).toBe(2)
    expect(resumeStepFor({ ...ctxMac({ fullDiskAccessChoice: 'allow' }), isMac: false })).toBe(2)
    expect(resumeStepFor({ ...ctxMac({ fullDiskAccessChoice: 'deny' }), isMac: false })).toBe(2)
  })
})

describe('step1VariantFor', () => {
  it('hasFda → already-granted regardless of source', () => {
    expect(step1VariantFor(ctxMac({ hasFda: true }), 'menu')).toBe('already-granted')
    expect(step1VariantFor(ctxMac({ hasFda: true }), 'first-launch')).toBe('already-granted')
  })

  it('allow + !hasFda + isOnboarded → revoked', () => {
    expect(step1VariantFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true }), 'first-launch')).toBe(
      'revoked',
    )
  })

  it('notAskedYet + first-launch → first-ask', () => {
    expect(step1VariantFor(ctxMac({}), 'first-launch')).toBe('first-ask')
  })

  it('menu re-entry without FDA and not onboarded → first-ask (don\'t lie about "revoked")', () => {
    expect(step1VariantFor(ctxMac({ isOnboarded: false, hasFda: false }), 'menu')).toBe('first-ask')
  })

  it('menu re-entry without FDA but onboarded → revoked', () => {
    expect(step1VariantFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true, hasFda: false }), 'menu')).toBe(
      'revoked',
    )
  })
})

describe('stepTwoBannerFor', () => {
  it('macOS hasFda → granted', () => {
    expect(stepTwoBannerFor(ctxMac({ hasFda: true }))).toBe('granted')
  })

  it('macOS deny → denied', () => {
    expect(stepTwoBannerFor(ctxMac({ fullDiskAccessChoice: 'deny' }))).toBe('denied')
  })

  it('macOS allow but !hasFda → stuck', () => {
    expect(stepTwoBannerFor(ctxMac({ fullDiskAccessChoice: 'allow', hasFda: false }))).toBe('stuck')
  })

  it('Linux → linux', () => {
    expect(stepTwoBannerFor({ ...ctxMac({}), isMac: false })).toBe('linux')
  })
})

describe('navigation state', () => {
  beforeEach(() => {
    resetForTesting()
  })

  it('nextStep refuses to advance past step 1 while footer mode is restart', () => {
    openWizard('first-launch', ctxMac({}))
    setStep1Restart()
    nextStep()
    expect(getOnboardingState().currentStep).toBe(1)
  })

  it('previousStep from step 2 returns to step 1 and resets footer mode to decide', () => {
    // Land on step 2 via the resume rule (deny is the cleanest seed).
    openWizard('first-launch', ctxMac({ fullDiskAccessChoice: 'deny' }))
    expect(getOnboardingState().currentStep).toBe(2)
    previousStep()
    expect(getOnboardingState().currentStep).toBe(1)
    expect(getOnboardingState().step1FooterMode).toBe('decide')
  })
})

describe('four-step flow (FDA → AI → Beta → Optional)', () => {
  beforeEach(() => {
    resetForTesting()
  })

  it('has a step count of 4 (the Beta page was inserted at step 3)', () => {
    expect(ONBOARDING_STEP_COUNT).toBe(4)
  })

  it('forward navigation walks 1 → 2 → 3 → 4 and stops at the last step', () => {
    openWizard('first-launch', ctxMac({})) // notAskedYet → step 1
    expect(getOnboardingState().currentStep).toBe(1)
    nextStep()
    expect(getOnboardingState().currentStep).toBe(2)
    nextStep()
    expect(getOnboardingState().currentStep).toBe(3)
    nextStep()
    expect(getOnboardingState().currentStep).toBe(4)
    // No step past the last: nextStep is a no-op on step 4.
    nextStep()
    expect(getOnboardingState().currentStep).toBe(4)
  })

  it('isAtLastStep is true only on step 4', () => {
    openWizard('first-launch', ctxMac({}))
    for (const step of [1, 2, 3] as const) {
      setCurrentStep(step)
      expect(isAtLastStep()).toBe(false)
    }
    setCurrentStep(4)
    expect(isAtLastStep()).toBe(true)
  })

  it('resume rule is unaffected by the Beta insertion (FDA/AI resume cases still resolve)', () => {
    // The resume rule only ever lands the user on step 1 or 2; inserting Beta at step 3
    // must not change those outcomes.
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'notAskedYet' }))).toBe(1)
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true, hasFda: false }))).toBe(1)
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'allow', hasFda: true }))).toBe(2)
    expect(resumeStepFor(ctxMac({ fullDiskAccessChoice: 'deny' }))).toBe(2)
    expect(resumeStepFor({ ...ctxMac({}), isMac: false })).toBe(2)
  })
})

describe('menu / palette re-entry always opens step 1 on macOS', () => {
  beforeEach(() => {
    resetForTesting()
  })

  it('menu source with hasFda + isOnboarded → step 1 (already-granted variant)', () => {
    openWizard('menu', ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true, hasFda: true }))
    const s = getOnboardingState()
    expect(s.currentStep).toBe(1)
    expect(s.step1Variant).toBe('already-granted')
  })

  it('palette source with deny + isOnboarded → step 1 (resume rule would have said step 2)', () => {
    openWizard('palette', ctxMac({ fullDiskAccessChoice: 'deny', isOnboarded: true, hasFda: false }))
    expect(getOnboardingState().currentStep).toBe(1)
  })

  it('first-launch source still honours the resume rule (smoke-test)', () => {
    openWizard('first-launch', ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: false, hasFda: true }))
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('Linux menu re-entry lands on step 2 (no step 1 to render)', () => {
    openWizard('menu', { fullDiskAccessChoice: 'notAskedYet', isOnboarded: true, hasFda: false, isMac: false })
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('openWizard resets finishRequestTick so re-entry does NOT fire onComplete on remount', () => {
    // Simulate a previous wizard session that requested completion (bumped the tick).
    openWizard('first-launch', ctxMac({ fullDiskAccessChoice: 'deny' }))
    requestWizardComplete()
    expect(getOnboardingState().finishRequestTick).toBe(1)
    // openWizard a second time. The counter MUST be reset; otherwise the new wizard
    // instance's `$effect` (which starts with `lastSeenFinishTick = 0`) would fire
    // `onComplete()` immediately on first observation, and the wizard would visibly
    // never appear on menu / palette re-entry. Regression guard for a Playwright failure
    // that surfaced when the counter wasn't being reset between wizard mounts.
    openWizard('menu', ctxMac({ fullDiskAccessChoice: 'allow', isOnboarded: true, hasFda: true }))
    expect(getOnboardingState().finishRequestTick).toBe(0)
  })
})
