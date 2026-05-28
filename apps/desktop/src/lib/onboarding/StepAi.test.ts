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
 * - The dual-button footer registers via `setFooterOverride`. Clicking "Start using
 *   Cmdr!" persists + calls `pushConfigToBackend` + bumps the wizard's
 *   `finishRequestTick`. Clicking "One more optional setup step" persists + advances.
 * - No-API-key-blocks-advance rule: cloud + empty key still advances; `pushConfigToBackend`
 *   still fires.
 *
 * Axe coverage lives in `StepAi.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
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
  (
    baseUrl: string,
    apiKey: string,
  ) => Promise<{
    connected: boolean
    authError: boolean
    models: string[]
    error: string | null
  }>
>(() => Promise.resolve({ connected: true, authError: false, models: ['gpt-4.1-mini'], error: null }))
const saveAiApiKey = vi.fn<(id: string, key: string) => Promise<null>>(() => Promise.resolve(null))
const getAiApiKey = vi.fn<(id: string) => Promise<string>>(() => Promise.resolve(''))
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const openPrivacySettings = vi.fn<() => Promise<void>>(() => Promise.resolve())
const configureAi = vi.fn<(...args: unknown[]) => Promise<void>>(() => Promise.resolve())
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
  checkAiConnection: (baseUrl: string, apiKey: string) => checkAiConnection(baseUrl, apiKey),
  saveAiApiKey: (id: string, key: string) => saveAiApiKey(id, key),
  getAiApiKey: (id: string) => getAiApiKey(id),
  openExternalUrl: (url: string) => openExternalUrl(url),
  openPrivacySettings: () => openPrivacySettings(),
  configureAi: (...args: unknown[]) => configureAi(...args),
  getAiRuntimeStatus: () => getAiRuntimeStatus(),
}))

// Settings store mock: in-memory key-value, mirroring what `$lib/settings` exposes.
// We reset it per test so previous picks don't leak.
const settingsMap: Record<string, unknown> = {}
function resetSettings(): void {
  for (const k of Object.keys(settingsMap)) {
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture reset
    delete settingsMap[k]
  }
  settingsMap['ai.provider'] = 'off'
  settingsMap['ai.cloudProvider'] = 'openai'
  settingsMap['ai.cloudProviderConfigs'] = '{}'
  settingsMap['ai.localContextSize'] = '4096'
}

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id] ?? '',
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

const loadSettings = vi.fn(() =>
  Promise.resolve({ showHiddenFiles: true, fullDiskAccessChoice: 'allow', isOnboarded: false }),
)
vi.mock('$lib/settings-store', () => ({
  loadSettings: () => loadSettings(),
  saveSettings: vi.fn(() => Promise.resolve()),
}))

const pushConfigToBackend = vi.fn(() => Promise.resolve())
vi.mock('$lib/settings/ai-config', () => ({
  pushConfigToBackend: () => pushConfigToBackend(),
}))

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

function radioByValue(target: HTMLElement, value: string): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>(`input[type="radio"][value="${value}"]`)
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
    startAiDownload.mockClear()
    cancelAiDownload.mockClear()
    checkAiConnection.mockClear()
    saveAiApiKey.mockClear()
    getAiApiKey.mockReset()
    getAiApiKey.mockResolvedValue('')
    openExternalUrl.mockClear()
    pushConfigToBackend.mockClear()
    loadSettings.mockReset()
    loadSettings.mockResolvedValue({
      showHiddenFiles: true,
      fullDiskAccessChoice: 'allow',
      isOnboarded: false,
    })
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
    loadSettings.mockResolvedValue({
      showHiddenFiles: true,
      fullDiskAccessChoice: 'deny',
      isOnboarded: false,
    })
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain('You chose not to enable full disk access')
  })

  it('shows the "stuck" banner when FDA was requested but not granted', async () => {
    setStepTwoBanner('stuck')
    checkFullDiskAccess.mockResolvedValue(false)
    loadSettings.mockResolvedValue({
      showHiddenFiles: true,
      fullDiskAccessChoice: 'allow',
      isOnboarded: false,
    })
    mounted = mountStep()
    await waitForAsync()
    expect(mounted.target.textContent).toContain("Cmdr doesn't seem to have full disk access yet")
  })

  it('picking cloud reveals the provider picker and setup grid', async () => {
    mounted = mountStep()
    await waitForAsync()
    const cloud = radioByValue(mounted.target, 'cloud')
    if (!cloud) throw new Error('cloud radio missing')
    cloud.checked = true
    cloud.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    expect(mounted.target.querySelector('[aria-label="Cloud AI providers"]')).not.toBeNull()
  })

  it('picking local fires startAiDownload when localAiSupported is true', async () => {
    mounted = mountStep()
    await waitForAsync()
    const local = radioByValue(mounted.target, 'local')
    if (!local) throw new Error('local radio missing')
    local.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    expect(startAiDownload).toHaveBeenCalled()
  })

  it('switching away from local calls cancelAiDownload', async () => {
    mounted = mountStep()
    await waitForAsync()
    radioByValue(mounted.target, 'local')?.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    startAiDownload.mockClear()
    radioByValue(mounted.target, 'off')?.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    expect(cancelAiDownload).toHaveBeenCalled()
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
    expect(local.disabled).toBe(true)
    // Force-dispatch change even though native UI would skip; assert no side effects.
    local.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    expect(startAiDownload).not.toHaveBeenCalled()
    expect(settingsMap['ai.provider']).toBe('off')
  })

  it('registers two footer buttons via setFooterOverride', async () => {
    mounted = mountStep()
    await waitForAsync()
    const buttons = getOnboardingState().footerOverride
    expect(buttons).not.toBeNull()
    expect(buttons?.map((b) => b.label)).toEqual(['Start using Cmdr!', 'One more optional setup step'])
    expect(buttons?.[0].variant).toBe('secondary')
    expect(buttons?.[1].variant).toBe('primary')
  })

  it('Start using Cmdr! persists the choice, pushes config to backend, and requests wizard finish', async () => {
    mounted = mountStep()
    await waitForAsync()
    radioByValue(mounted.target, 'cloud')?.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    const initialTick = getOnboardingState().finishRequestTick
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(pushConfigToBackend).toHaveBeenCalled()
    expect(getOnboardingState().finishRequestTick).toBe(initialTick + 1)
  })

  it('One more optional setup step persists and advances to step 3', async () => {
    mounted = mountStep()
    await waitForAsync()
    radioByValue(mounted.target, 'off')?.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    getOnboardingState().footerOverride?.[1].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('off')
    expect(pushConfigToBackend).toHaveBeenCalled()
    expect(getOnboardingState().currentStep).toBe(3)
  })

  it('No-key-blocks-advance: cloud with empty key still calls pushConfigToBackend', async () => {
    // Default mocks: getAiApiKey resolves '', so the key field stays empty.
    mounted = mountStep()
    await waitForAsync()
    radioByValue(mounted.target, 'cloud')?.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForAsync()
    getOnboardingState().footerOverride?.[0].onclick()
    await waitForAsync()
    expect(settingsMap['ai.provider']).toBe('cloud')
    expect(pushConfigToBackend).toHaveBeenCalled()
  })
})
