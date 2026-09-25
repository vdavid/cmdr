/**
 * Behaviour tests for `StepAi.svelte`.
 *
 * Covers:
 * - The three FDA-banner branches (granted / denied / stuck), driven by the FDA probe
 *   on step-2 entry plus the persisted `fullDiskAccessChoice`.
 * - The three radio choices (cloud / local / off) and their side effects:
 *   - Cloud renders the picker + setup grid.
 *   - Local kicks off `startAiDownload()` when supported.
 *   - Switching away from local cancels.
 *   - The "off" radio renders no provider UI.
 * - Intel-Mac gate: when `getAiRuntimeStatus().localAiSupported === false`, the local
 *   radio is disabled, doesn't fire `startAiDownload`, and `setSetting('ai.provider',
 *   'local')` does not run.
 * - The single forward footer button ("Next") registers via `setFooterOverride`. Clicking
 *   it persists + calls `pushConfigToBackend` + advances to the Beta page (step 3). It
 *   never completes onboarding: the Beta page is non-skippable.
 * - The missing-API-key confirm-once gate: cloud with no stored key warns on the first
 *   Next and goes through on the second, so nothing is ever hard-blocked (the
 *   no-key-blocks-advance rule) but nobody sails past a half-configured AI setup either.
 * - Cloud shows the Allow cloud AI switch above a locked setup, and the first Next with the
 *   switch off shows a confirm-once note instead of the missing-key one.
 * - "Thanks but no thanks" lands on all four pieces of state: `ai.provider = 'off'`,
 *   cloud consent declined, `askCmdr.proactive = false`, `askCmdr.enabled = false`. Picking
 *   a provider never grants consent and never touches `askCmdr.proactive` (the
 *   consent-bypass guard); it turns `askCmdr.enabled` on only when nobody set it yet. A
 *   failing decline still lets the user move on.
 *
 * Axe coverage lives in `StepAi.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import type { ConsentOutcome } from '$lib/ai/cloud-consent.svelte'
import StepAi from './StepAi.svelte'
import {
  closeWizard,
  resetForTesting,
  openWizard,
  setStepTwoBanner,
  setCurrentStep,
  getOnboardingState,
} from './onboarding-state.svelte'

const checkFullDiskAccess = vi.fn<() => Promise<boolean>>(() => Promise.resolve(true))
const startAiDownload = vi.fn<() => Promise<void>>(() => Promise.resolve())
const cancelAiDownload = vi.fn<() => Promise<void>>(() => Promise.resolve())
const checkAiConnection = vi.fn<
  (payload: { baseUrl: string; providerId: string }) => Promise<{
    connected: boolean
    authError: boolean
    models: string[]
    error: string | null
  }>
>(() => Promise.resolve({ connected: true, authError: false, models: ['gpt-4.1-mini'], error: null }))
const saveAiApiKey = vi.fn<(payload: { providerId: string; apiKey: string }) => Promise<null>>(() =>
  Promise.resolve(null),
)
const getAiApiKeyStatus = vi.fn<(id: string) => Promise<{ isSet: boolean; fingerprint: string }>>(() =>
  Promise.resolve({ isSet: false, fingerprint: '' }),
)
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const openPrivacySettings = vi.fn<() => Promise<void>>(() => Promise.resolve())
const configureAi = vi.fn<(...args: unknown[]) => Promise<{ secretStoreError: unknown }>>(() =>
  Promise.resolve({ secretStoreError: null }),
)
const getAiRuntimeStatus = vi.fn(() =>
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
)

vi.mock('$lib/tauri-commands', () => ({
  checkFullDiskAccess: () => checkFullDiskAccess(),
  startAiDownload: () => startAiDownload(),
  cancelAiDownload: () => cancelAiDownload(),
  checkAiConnection: (baseUrl: string, providerId: string) => checkAiConnection({ baseUrl, providerId }),
  saveAiApiKey: (providerId: string, apiKey: string) => saveAiApiKey({ providerId, apiKey }),
  getAiApiKeyStatus: (id: string) => getAiApiKeyStatus(id),
  openExternalUrl: (url: string) => openExternalUrl(url),
  openPrivacySettings: () => openPrivacySettings(),
  configureAi: (...args: unknown[]) => configureAi(...args),
  getAiRuntimeStatus: () => getAiRuntimeStatus(),
}))

// Settings store mock: in-memory key-value, mirroring what `$lib/settings` exposes.
// We reset it per test so previous picks don't leak.
const settingsMap: Record<string, unknown> = {}
/** Ids "explicitly set", as the real store's sparse-persistence ledger tracks them. */
const explicitlySet = new Set<string>()
function resetSettings(): void {
  for (const k of Object.keys(settingsMap)) {
    delete settingsMap[k]
  }
  explicitlySet.clear()
  settingsMap['ai.provider'] = 'off'
  settingsMap['ai.cloudProvider'] = 'openai'
  settingsMap['ai.cloudProviderConfigs'] = '{}'
  settingsMap['ai.localContextSize'] = '4096'
  // Mirrors the registry: `askCmdr.proactive` ships ON, so it's armed unless something
  // turns it off.
  settingsMap['askCmdr.proactive'] = true
}

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id] ?? '',
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
      explicitlySet.add(id)
    },
    isExplicitlySet: (id: string) => explicitlySet.has(id),
    onSpecificSettingChange: () => () => {},
  }
})

const pushConfigToBackend = vi.fn(() => Promise.resolve())
vi.mock('$lib/settings/ai-config', () => ({
  pushConfigToBackend: () => pushConfigToBackend(),
}))

// Cloud AI consent lives in `main.db`, not the registry, so the step drives it through this
// module. The wizard's own code may only ever REVOKE (see the consent-bypass guard below);
// granting is the switch's click alone. `declineCloudConsent` is the one "no" path (retry,
// then hold); its legs are pinned in `cloud-consent.svelte.test.ts`, so here it's the seam.
// Consent reads "allowed" unless a test says otherwise, so the older Cloud cases below meet
// only the gates they're about. A plain object: these tests read it once per action.
const declineConsent = vi.fn<() => Promise<ConsentOutcome>>(() => Promise.resolve('done'))
const acceptConsent = vi.fn<() => Promise<ConsentOutcome>>(() => Promise.resolve('done'))
const cloudConsent = vi.hoisted(() => {
  // An annotation, not an `as`: the lint auto-fix strips an assertion it thinks is unnecessary.
  const state: { accepted: boolean | null; acceptedAt: number | null } = { accepted: true, acceptedAt: null }
  return state
})
vi.mock('$lib/ai/cloud-consent.svelte', () => ({
  cloudConsentState: cloudConsent,
  refreshCloudConsent: () => Promise.resolve(),
  cloudAiBlocked: (provider: string) => provider === 'cloud' && cloudConsent.accepted !== true,
  declineCloudConsent: () => declineConsent(),
  acceptCloudConsent: () => acceptConsent(),
  CLOUD_CONSENT_ANCHOR: 'settings-ai-cloud-consent',
}))

// The step's logger, so a test can tell a logged failure from a logged cancel. Lazy
// wrappers: the step calls `getAppLogger` while its module loads, before these exist.
const logWarn = vi.fn<(...args: unknown[]) => void>()
const logInfo = vi.fn<(...args: unknown[]) => void>()
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    warn: (...args: unknown[]) => {
      logWarn(...args)
    },
    info: (...args: unknown[]) => {
      logInfo(...args)
    },
    debug: () => undefined,
    error: () => undefined,
  }),
}))

/** A start that stays pending until the test settles it, like a real download in flight. */
function pendingStart(): { reject: (error: unknown) => void } {
  const handle = { reject: (_error: unknown): void => undefined }
  startAiDownload.mockImplementationOnce(
    () =>
      new Promise<void>((_resolve, reject) => {
        handle.reject = reject
      }),
  )
  return handle
}

// Cloud setup component reaches into the secret store; the parent test mocks above
// cover it. No special mock for CloudProviderSetup itself; it renders inline.

function mountStep() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepAi, { target, props: {} })
  return { target, instance }
}

async function waitForAsync(): Promise<void> {
  for (let i = 0; i < 20; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

/**
 * The three AI options render through the house `RadioGroup` (Ark UI). An option is a
 * `<label class="radio-item">` wrapping a visually-hidden `<input type=radio value=…>`;
 * the input's `value` is the stable identity, so tests key off it rather than the label
 * copy (which changes) or Ark's generated ids (which are an implementation detail).
 * Picking one is a click on the LABEL: there is no `change` event to dispatch.
 */
function radioByValue(target: HTMLElement, value: string): HTMLElement | null {
  const input = target.querySelector<HTMLInputElement>(`.radio-item input[type="radio"][value="${value}"]`)
  return input?.closest<HTMLElement>('.radio-item') ?? null
}

function pickChoice(target: HTMLElement, value: string): void {
  const radio = radioByValue(target, value)
  if (!radio) throw new Error(`no AI option with value "${value}"`)
  radio.click()
}

describe('StepAi', () => {
  let mounted: ReturnType<typeof mountStep> | undefined

  beforeEach(() => {
    resetSettings()
    closeWizard()
    resetForTesting()
    // Land us on step 2 with a default banner; tests override per case.
    openWizard('force')
    setCurrentStep(2)
    setStepTwoBanner('granted')
    checkFullDiskAccess.mockReset()
    checkFullDiskAccess.mockResolvedValue(true)
    startAiDownload.mockReset()
    startAiDownload.mockResolvedValue(undefined)
    cancelAiDownload.mockClear()
    logWarn.mockClear()
    logInfo.mockClear()
    checkAiConnection.mockClear()
    saveAiApiKey.mockClear()
    getAiApiKeyStatus.mockReset()
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })
    openExternalUrl.mockClear()
    pushConfigToBackend.mockClear()
    declineConsent.mockReset()
    declineConsent.mockResolvedValue('done')
    acceptConsent.mockClear()
    cloudConsent.accepted = true
    settingsMap['onboarding.fullDiskAccessChoice'] = 'allow'
    settingsMap['onboarding.completed'] = false
    getAiRuntimeStatus.mockReset()
    getAiRuntimeStatus.mockResolvedValue({
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
    })
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

  it('renders the comparison table and the three radio choices', async () => {
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain('Here is how you do common actions')
    expect(radioByValue(mounted.target, 'cloud')).not.toBeNull()
    expect(radioByValue(mounted.target, 'local')).not.toBeNull()
    expect(radioByValue(mounted.target, 'off')).not.toBeNull()
  })

  it('shows the "granted" banner when FDA is on', async () => {
    setStepTwoBanner('granted')
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain('Thanks for granting full disk access')
  })

  it('shows the "denied" banner when the user denied FDA', async () => {
    setStepTwoBanner('denied')
    checkFullDiskAccess.mockResolvedValue(false)
    settingsMap['onboarding.fullDiskAccessChoice'] = 'deny'
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain('You chose not to enable full disk access')
  })

  it('shows the "stuck" banner when FDA was requested but not granted', async () => {
    setStepTwoBanner('stuck')
    checkFullDiskAccess.mockResolvedValue(false)
    settingsMap['onboarding.fullDiskAccessChoice'] = 'allow'
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain('Cmdr doesn’t seem to have full disk access yet')
  })

  it('picking cloud reveals the provider picker and setup grid', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    expect(mounted.target.querySelector('[aria-label="Cloud AI providers"]')).not.toBeNull()
  })

  it('picking local fires startAiDownload when localAiSupported is true', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    expect(startAiDownload).toHaveBeenCalled()
  })

  it('switching away from local calls cancelAiDownload', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    startAiDownload.mockClear()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    expect(cancelAiDownload).toHaveBeenCalled()
  })

  it('the download’s end after switching away from Local logs as the person’s own cancel, not a failure', async () => {
    const start = pendingStart()
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    logWarn.mockClear()
    logInfo.mockClear()

    start.reject(new Error('Download cancelled'))
    await waitForAsync()

    expect(logWarn).not.toHaveBeenCalled()
    expect(logInfo).toHaveBeenCalledOnce()
  })

  it('a cancel stays a cancel even when the person already picked Local again', async () => {
    // The first attempt's rejection lands after the second start: the call it answers was
    // cancelled by choice, whatever the step shows now.
    const first = pendingStart()
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    logWarn.mockClear()

    first.reject(new Error('Download cancelled'))
    await waitForAsync()

    expect(logWarn).not.toHaveBeenCalled()
  })

  it('a download that stops while Local is still picked logs as a failure, with its cause', async () => {
    const start = pendingStart()
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    const failure = new Error('HTTP 503')

    start.reject(failure)
    await waitForAsync()

    expect(logWarn).toHaveBeenCalledWith(expect.any(String), { error: failure })
    expect(logInfo).not.toHaveBeenCalled()
  })

  it('Intel gate: when localAiSupported is false the local radio is disabled and ignored', async () => {
    getAiRuntimeStatus.mockResolvedValue({
      serverRunning: false,
      serverStarting: false,
      pid: null,
      port: null,
      modelInstalled: false,
      modelName: 'Ministral 3B',
      modelSizeBytes: 0,
      modelSizeFormatted: '0 B',
      downloadInProgress: false,
      localAiSupported: false,
      kvBytesPerToken: 0,
      baseOverheadBytes: 0,
    })
    mounted = mountStep()
    await waitForAsync()
    const local = radioByValue(mounted.target, 'local')
    if (!local) throw new Error('local radio missing')
    expect(local.getAttribute('data-disabled')).not.toBeNull()
    // Click it anyway, the way a stray press would, and assert no side effects.
    local.click()
    await waitForAsync()
    expect(startAiDownload).not.toHaveBeenCalled()
    expect(settingsMap['ai.provider']).toBe('off')
  })

  it('registers a single "Next" forward button via setFooterOverride', async () => {
    mounted = mountStep()
    await waitForAsync()
    const buttons = getOnboardingState().footerOverride
    expect(buttons).not.toBeNull()
    expect(buttons?.map((b) => b.label)).toEqual(['Next'])
    expect(buttons?.[0].variant).toBe('primary')
  })

  it('Next persists the choice, pushes config to backend, and advances to the Beta page (step 3) without finishing', async () => {
    // A stored key, so the missing-key gate stays out of the way and this test is about
    // the forward button alone.
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc' })
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    const initialTick = getOnboardingState().finishRequestTick
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(pushConfigToBackend).toHaveBeenCalled()
    // Beta is non-skippable: this advances to step 3, it does NOT request wizard finish.
    expect(getOnboardingState().currentStep).toBe(3)
    expect(getOnboardingState().finishRequestTick).toBe(initialTick)
  })

  it('"Thanks but no thanks" turns cloud AI and Ask Cmdr off and disarms askCmdr.proactive', async () => {
    mounted = mountStep()
    await waitForAsync()
    // Start from cloud so the pick to 'off' is a real choice change, not the default.
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('off')
    expect(declineConsent).toHaveBeenCalledTimes(1)
    expect(settingsMap['askCmdr.proactive']).toBe(false)
    expect(settingsMap['askCmdr.enabled']).toBe(false)
    expect(getOnboardingState().currentStep).toBe(3)
  })

  it('picking cloud NEVER grants consent and leaves askCmdr.proactive alone (consent-bypass guard)', async () => {
    // A stored key, so the missing-key gate stays out of the way: this test is about consent.
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc' })
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(acceptConsent).not.toHaveBeenCalled()
    expect(declineConsent).not.toHaveBeenCalled()
    expect(settingsMap['askCmdr.proactive']).toBe(true)
  })

  it('picking local NEVER grants consent and leaves askCmdr.proactive alone', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('local')
    expect(acceptConsent).not.toHaveBeenCalled()
    expect(declineConsent).not.toHaveBeenCalled()
    expect(settingsMap['askCmdr.proactive']).toBe(true)
  })

  it('a "no" that not even a hold could keep is logged, and still never traps the person on the step', async () => {
    declineConsent.mockResolvedValue('notSaved')
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(logWarn).toHaveBeenCalledOnce()
    expect(getOnboardingState().currentStep).toBe(3)
  })

  it('a failing revoke still advances and leaves the forward button usable', async () => {
    declineConsent.mockResolvedValue('notSaved')
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().currentStep).toBe(3)
    expect(getOnboardingState().footerOverride?.[0].disabled).toBe(false)
    // The rest of the persist still runs: a hiccup in `main.db` doesn't cost the user
    // their provider choice.
    expect(pushConfigToBackend).toHaveBeenCalled()
  })

  it('Cloud shows the Allow cloud AI switch above a setup that stays locked until it is on', async () => {
    cloudConsent.accepted = false
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    const setupColumn = mounted.target.querySelector('.cloud-grid-setup')
    expect(setupColumn?.querySelector('input[data-test="cloud-ai-consent"]')).not.toBeNull()
    expect(setupColumn?.querySelector('.setup-lock')?.hasAttribute('inert')).toBe(true)
    // The locked setup never probes the service.
    expect(checkAiConnection).not.toHaveBeenCalled()
  })

  it('Cloud with cloud AI allowed shows the setup unlocked', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    expect(mounted.target.querySelector('.cloud-grid-setup .setup-lock')?.hasAttribute('inert')).toBe(false)
  })

  it('Cloud with the switch off: the first Next says cloud AI stays off, the second goes through', async () => {
    cloudConsent.accepted = false
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().footerNote?.textContent).toContain('Cloud AI stays off until you allow it.')
    // The locked setup can't take a key, so the missing-key note isn't the one shown.
    expect(getOnboardingState().footerNote?.textContent).not.toContain('API key')
    expect(getOnboardingState().currentStep).toBe(2)

    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(getOnboardingState().currentStep).toBe(3)
    expect(acceptConsent).not.toHaveBeenCalled()
  })

  it('Cloud or Local turns Ask Cmdr on for a fresh profile, but never over an answer already given', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc' })
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'local')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['askCmdr.enabled']).toBe(true)
    await unmount(mounted.instance)
    mounted.target.remove()

    // A re-run of the wizard by someone who switched Ask Cmdr off keeps it off.
    setCurrentStep(2)
    settingsMap['askCmdr.enabled'] = false
    explicitlySet.add('askCmdr.enabled')
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['askCmdr.enabled']).toBe(false)
  })

  it('cloud with no stored key: the first Next warns instead of advancing', async () => {
    // Default mocks: no key is stored for the provider.
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().footerNote?.textContent).toContain('API key')
    expect(getOnboardingState().currentStep).toBe(2)
    expect(pushConfigToBackend).not.toHaveBeenCalled()
  })

  it('cloud with no stored key: the second Next goes through, warning and all', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(pushConfigToBackend).toHaveBeenCalled()
    expect(getOnboardingState().currentStep).toBe(3)
    expect(getOnboardingState().footerNote).toBeNull()
  })

  it('the warning clears on the next thing the user does, so the gate asks again', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'cloud')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().footerNote).not.toBeNull()

    // Anything outside the wizard footer counts as the user moving on.
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await waitForAsync()
    expect(getOnboardingState().footerNote).toBeNull()

    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().footerNote).not.toBeNull()
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('no gate when the picked provider needs no key, or when AI is off', async () => {
    mounted = mountStep()
    await waitForAsync()
    pickChoice(mounted.target, 'off')
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(getOnboardingState().footerNote).toBeNull()
    expect(getOnboardingState().currentStep).toBe(3)
  })
})
