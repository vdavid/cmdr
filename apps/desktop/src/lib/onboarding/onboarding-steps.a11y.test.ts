/**
 * Tier 3 a11y tests for the onboarding wizard and each of its steps.
 *
 * One file per step would cost about five times as much: `svelte-tests` charges
 * per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its step's own doc comment, props, and
 * assertions.
 *
 * Two stubs are mutable because the source files disagreed: `checkFullDiskAccess`
 * answers `false` for the FDA step and `true` for the AI step, and only the
 * wizard and the FDA step forced `isMacOS`. Forcing that file-wide would change
 * what the other steps render on a Linux CI runner.
 *
 * The standalone pieces live in `onboarding-parts.a11y.test.ts`; one merged file
 * for the whole directory would clear the 800-line `file-length` mark.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import {
  closeWizard,
  resetForTesting,
  openWizard,
  setCurrentStep,
  setStep1Granted,
  setStep1Restart,
  setStep1Variant,
  setStepTwoBanner,
} from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { TERMS_VERSION } from '$lib/legal/terms'

const stubs = vi.hoisted(() => ({
  settingsMap: Object.create(null) as Record<string, unknown>,
  getSetting: null as ((id: string) => unknown) | null,
  fullDiskAccess: false,
  isMacOS: null as (() => boolean) | null,
}))

// The union of what these five blocks reach for, over the real module: each
// source file stubbed a different slice, so a bare union would hand a step a
// missing export it never had.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  checkFullDiskAccess: vi.fn(() => Promise.resolve(stubs.fullDiskAccess)),
  checkFullDiskAccessQuiet: vi.fn(() => Promise.resolve(stubs.fullDiskAccess)),
  getMacosMajorVersion: vi.fn(() => Promise.resolve(14)),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  startIndexingAfterFdaDecision: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  startAiDownload: vi.fn(() => Promise.resolve()),
  cancelAiDownload: vi.fn(() => Promise.resolve()),
  checkAiConnection: vi.fn(() =>
    Promise.resolve({ connected: true, authError: false, models: ['gpt-4.1-mini'], error: null }),
  ),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKeyStatus: vi.fn(() => Promise.resolve({ isSet: false, fingerprint: '' })),
  configureAi: vi.fn(() => Promise.resolve({ secretStoreError: null })),
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
  betaSignup: vi.fn(() => Promise.resolve({ kind: 'subscribed' as const })),
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realGetSetting = actual.getSetting as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string) => (stubs.getSetting ? stubs.getSetting(id) : realGetSetting(id)),
    setSetting: (id: string, value: unknown) => {
      stubs.settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
    // Overridden, not spread through: the real one reaches the plugin store.
    forceSave: () => Promise.resolve(true),
  }
})

vi.mock('$lib/settings/settings-store', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/settings/ai-config', () => ({
  pushConfigToBackend: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(() => Promise.resolve()),
}))

// jsdom isn't macOS, so the safety-net guard would short-circuit the FDA render.
// Resume-rule platform logic is unit-tested separately.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realIsMacOS = actual.isMacOS as () => boolean
  return { ...actual, isMacOS: () => (stubs.isMacOS ? stubs.isMacOS() : realIsMacOS()) }
})

import OnboardingWizard from './OnboardingWizard.svelte'
import StepAi from './StepAi.svelte'
import StepBeta from './StepBeta.svelte'
import StepFda from './StepFda.svelte'
import StepOptional from './StepOptional.svelte'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

/** Records the mounted instance so `afterEach` can unmount it. */
function track(target: HTMLElement, instance: ReturnType<typeof mount>): void {
  mounted = { target, instance }
}

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

beforeEach(() => {
  for (const key of Object.keys(stubs.settingsMap)) delete stubs.settingsMap[key]
  stubs.getSetting = null
  stubs.fullDiskAccess = false
  stubs.isMacOS = null
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

/**
 * Tier 3 axe-based a11y tests for the onboarding wizard.
 *
 * Asserts axe-clean structure for each reachable wizard state. Step 1 has three variants
 * (first-ask, revoked, already-granted) and two footer modes (decide, restart); we exercise
 * the ones that change visible structure. Step 2 and step 3 a11y live in their own blocks
 * below.
 *
 * Focus trap + Escape-swallowing behaviour live in `OnboardingWizard.test.ts`.
 */
describe('OnboardingWizard a11y', () => {
  beforeEach(() => {
    stubs.isMacOS = () => true
  })

  it('step 1 first-ask (decide mode) has no a11y violations', async () => {
    const target = container()
    track(target, mount(OnboardingWizard, { target, props: { onComplete: () => {} } }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 1 already-granted variant has no a11y violations', async () => {
    openWizard('menu')
    setStep1Variant('already-granted')
    const target = container()
    track(target, mount(OnboardingWizard, { target, props: { onComplete: () => {} } }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 1 in restart footer mode has no a11y violations', async () => {
    openWizard('first-launch')
    setStep1Restart()
    const target = container()
    track(target, mount(OnboardingWizard, { target, props: { onComplete: () => {} } }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 2 stub has no a11y violations', async () => {
    openWizard('first-launch')
    setCurrentStep(2)
    const target = container()
    track(target, mount(OnboardingWizard, { target, props: { onComplete: () => {} } }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 3 stub has no a11y violations', async () => {
    openWizard('first-launch')
    setCurrentStep(3)
    const target = container()
    track(target, mount(OnboardingWizard, { target, props: { onComplete: () => {} } }))
    await tick()
    flushSync()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 axe a11y tests for `StepFda.svelte`.
 *
 * One test per variant. Tier-3 a11y is structural (ARIA, labels, focusables); axe runs
 * in jsdom against the mounted component. Focus management and Escape behaviour live
 * in `OnboardingWizard.test.ts`.
 */
describe('StepFda a11y', () => {
  beforeEach(() => {
    // Same reason as in `StepFda.test.ts`: jsdom isn't macOS so the safety-net guard would
    // short-circuit the render.
    stubs.isMacOS = () => true
    closeWizard()
    resetForTesting()
    openWizard('first-launch')
  })

  it('first-ask variant has no a11y violations', async () => {
    setStep1Variant('first-ask')
    const target = container()
    track(target, mount(StepFda, { target, props: {} }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('revoked variant has no a11y violations', async () => {
    setStep1Variant('revoked')
    const target = container()
    track(target, mount(StepFda, { target, props: {} }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('already-granted variant has no a11y violations', async () => {
    setStep1Variant('already-granted')
    const target = container()
    track(target, mount(StepFda, { target, props: {} }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('grant-detected success state has no a11y violations', async () => {
    setStep1Variant('first-ask')
    setStep1Granted()
    const target = container()
    track(target, mount(StepFda, { target, props: {} }))
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 axe a11y tests for `StepAi.svelte`.
 *
 * One test per meaningful state: each FDA banner branch + each radio choice. Axe
 * runs in jsdom (no contrast, no region; see `$lib/test-a11y`).
 */
describe('StepAi a11y', () => {
  beforeEach(() => {
    stubs.fullDiskAccess = true
    stubs.settingsMap['ai.provider'] = 'off'
    stubs.settingsMap['ai.cloudProvider'] = 'openai'
    stubs.settingsMap['ai.cloudProviderConfigs'] = '{}'
    stubs.settingsMap['ai.localContextSize'] = '4096'
    stubs.getSetting = (id: string) => stubs.settingsMap[id] ?? ''
    closeWizard()
    resetForTesting()
    openWizard('force')
    setCurrentStep(2)
  })

  it('granted banner has no a11y violations', async () => {
    setStepTwoBanner('granted')
    const target = container()
    track(target, mount(StepAi, { target, props: {} }))
    await settle()
    await expectNoA11yViolations(target)
  })

  it('denied banner has no a11y violations', async () => {
    setStepTwoBanner('denied')
    const target = container()
    track(target, mount(StepAi, { target, props: {} }))
    await settle()
    await expectNoA11yViolations(target)
  })

  it('stuck banner has no a11y violations', async () => {
    setStepTwoBanner('stuck')
    const target = container()
    track(target, mount(StepAi, { target, props: {} }))
    await settle()
    await expectNoA11yViolations(target)
  })

  it('cloud-picked state has no a11y violations', async () => {
    setStepTwoBanner('granted')
    stubs.settingsMap['ai.provider'] = 'cloud'
    const target = container()
    track(target, mount(StepAi, { target, props: {} }))
    await settle()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 axe a11y tests for `StepBeta.svelte` (the "Open beta" onboarding page).
 *
 * States: the opt-out switch on (default) and off, plus the required terms checkbox
 * unticked and ticked. The switch is labelled by its registry definition; the email input
 * carries its own aria-label; the terms checkbox is named by its inline label (which holds
 * a link) and marked `aria-required`. Axe runs in jsdom (no contrast checks; we cover those
 * in tier-1 scripts).
 */
describe('StepBeta a11y', () => {
  beforeEach(() => {
    stubs.settingsMap['analytics.enabled'] = true
    stubs.settingsMap['analytics.email'] = ''
    stubs.settingsMap['onboarding.termsAcceptedVersion'] = ''
    stubs.settingsMap['onboarding.termsAcceptedAt'] = ''
    stubs.getSetting = (id: string) => stubs.settingsMap[id]
    closeWizard()
    resetForTesting()
    openWizard('force')
    setCurrentStep(3)
  })

  it('default state (opt-out on) has no a11y violations', async () => {
    const target = container()
    track(target, mount(StepBeta, { target, props: {} }))
    await settle(10)
    await expectNoA11yViolations(target)
  })

  it('opted-out state (switch off) has no a11y violations', async () => {
    stubs.settingsMap['analytics.enabled'] = false
    const target = container()
    track(target, mount(StepBeta, { target, props: {} }))
    await settle(10)
    await expectNoA11yViolations(target)
  })

  it('the required terms checkbox is named and marked required, with no a11y violations', async () => {
    const target = container()
    track(target, mount(StepBeta, { target, props: {} }))
    await settle(10)

    const checkbox = target.querySelector<HTMLInputElement>('.terms-block input[type="checkbox"]')
    expect(checkbox?.getAttribute('aria-required')).toBe('true')
    // Named by the inline consent label, so a screen reader reads the sentence being agreed to.
    expect(checkbox?.getAttribute('aria-labelledby')).toBeTruthy()
    await expectNoA11yViolations(target)
  })

  it('accepted-terms state has no a11y violations', async () => {
    stubs.settingsMap['onboarding.termsAcceptedVersion'] = TERMS_VERSION
    stubs.settingsMap['onboarding.termsAcceptedAt'] = '2026-08-10T09:00:00.000Z'
    const target = container()
    track(target, mount(StepBeta, { target, props: {} }))
    await settle(10)
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 axe a11y tests for `StepOptional.svelte`.
 *
 * Two states: default (all four toggles on) and one-off (networking off). Each switch
 * is labelled by its registry definition; the section heading gives the question
 * context. Axe runs in jsdom (no contrast checks; we cover those in tier-1 scripts).
 */
describe('StepOptional a11y', () => {
  beforeEach(() => {
    stubs.settingsMap['network.enabled'] = true
    stubs.settingsMap['indexing.enabled'] = true
    stubs.settingsMap['updates.autoCheck'] = true
    stubs.settingsMap['fileOperations.mtpEnabled'] = true
    stubs.getSetting = (id: string) => stubs.settingsMap[id]
    closeWizard()
    resetForTesting()
    openWizard('force')
    setCurrentStep(3)
  })

  it('default state (all toggles on) has no a11y violations', async () => {
    const target = container()
    track(target, mount(StepOptional, { target, props: {} }))
    await settle(10)
    await expectNoA11yViolations(target)
  })

  it('one-off state (networking off) has no a11y violations', async () => {
    stubs.settingsMap['network.enabled'] = false
    const target = container()
    track(target, mount(StepOptional, { target, props: {} }))
    await settle(10)
    await expectNoA11yViolations(target)
  })
})
