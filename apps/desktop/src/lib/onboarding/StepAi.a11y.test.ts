/**
 * Tier 3 axe a11y tests for `StepAi.svelte` (M3).
 *
 * One test per meaningful state: each FDA banner branch + each radio choice. Axe
 * runs in jsdom (no contrast, no region — see `$lib/test-a11y`).
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import StepAi from './StepAi.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep, setStepTwoBanner } from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  checkFullDiskAccess: vi.fn(() => Promise.resolve(true)),
  startAiDownload: vi.fn(() => Promise.resolve()),
  cancelAiDownload: vi.fn(() => Promise.resolve()),
  checkAiConnection: vi.fn(() =>
    Promise.resolve({ connected: true, authError: false, models: ['gpt-4.1-mini'], error: null }),
  ),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKey: vi.fn(() => Promise.resolve('')),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  configureAi: vi.fn(() => Promise.resolve()),
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
}))

const settingsMap: Record<string, unknown> = {
  'ai.provider': 'off',
  'ai.cloudProvider': 'openai',
  'ai.cloudProviderConfigs': '{}',
  'ai.localContextSize': '4096',
}

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id] ?? '',
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn(() =>
    Promise.resolve({ showHiddenFiles: true, fullDiskAccessChoice: 'allow', isOnboarded: false }),
  ),
  saveSettings: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/ai-config', () => ({
  pushConfigToBackend: vi.fn(() => Promise.resolve()),
}))

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

function mountStep() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepAi, { target, props: {} })
  mounted = { target, instance }
  return target
}

beforeEach(() => {
  for (const k of Object.keys(settingsMap)) {
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture reset
    delete settingsMap[k]
  }
  settingsMap['ai.provider'] = 'off'
  settingsMap['ai.cloudProvider'] = 'openai'
  settingsMap['ai.cloudProviderConfigs'] = '{}'
  settingsMap['ai.localContextSize'] = '4096'
  closeWizard()
  resetForTesting()
  openWizard('force')
  setCurrentStep(2)
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

describe('StepAi a11y', () => {
  it('granted banner has no a11y violations', async () => {
    setStepTwoBanner('granted')
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('denied banner has no a11y violations', async () => {
    setStepTwoBanner('denied')
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('stuck banner has no a11y violations', async () => {
    setStepTwoBanner('stuck')
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('cloud-picked state has no a11y violations', async () => {
    setStepTwoBanner('granted')
    settingsMap['ai.provider'] = 'cloud'
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })
})
