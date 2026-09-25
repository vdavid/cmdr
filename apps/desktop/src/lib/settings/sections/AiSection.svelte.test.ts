/**
 * Tier-3 tests for the Allow cloud AI switch in Settings > AI > Provider: where it shows, what
 * it locks, and that flipping it never touches the AI mode (and switching modes never touches
 * consent). The consent state module runs for real over mocked IPC, so these pin the whole
 * frontend path from the click to the command.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'

const stubs = vi.hoisted(() => ({
  settings: Object.create(null) as Record<string, unknown>,
  // Several subscribers per id (the section AND each `SettingRow`'s reset affordance).
  listeners: new Map<string, Set<(value: unknown) => void>>(),
  consent: { accepted: false },
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => stubs.settings[id] ?? '',
    setSetting: vi.fn((id: string, value: unknown) => {
      stubs.settings[id] = value
    }),
    forceSave: vi.fn(() => Promise.resolve(true)),
    onSpecificSettingChange: (id: string, fn: (value: unknown) => void) => {
      const set = stubs.listeners.get(id) ?? new Set()
      set.add(fn)
      stubs.listeners.set(id, set)
      return () => set.delete(fn)
    },
  }
})

vi.mock('$lib/settings/ai-config', () => ({ pushConfigToBackend: vi.fn(() => Promise.resolve()) }))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getAiRuntimeStatus: vi.fn(() => Promise.resolve({ localAiSupported: true })),
  stopAiServer: vi.fn(() => Promise.resolve()),
  checkAiConnection: vi.fn(() =>
    Promise.resolve({ connected: false, authError: false, models: [], error: null, cloudConsentMissing: false }),
  ),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKeyStatus: vi.fn(() => Promise.resolve({ isSet: false, fingerprint: '' })),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  cloudAiConsentStatus: vi.fn(() =>
    Promise.resolve({
      accepted: stubs.consent.accepted,
      currentVersion: 1,
      acceptedVersion: stubs.consent.accepted ? 1 : null,
      acceptedAt: stubs.consent.accepted ? 1_760_000_000 : null,
    }),
  ),
  acceptCloudAiConsent: vi.fn(() => {
    stubs.consent.accepted = true
    return Promise.resolve()
  }),
  revokeCloudAiConsent: vi.fn(() => {
    stubs.consent.accepted = false
    return Promise.resolve()
  }),
  cloudAiConsentRevokePendingChanged: vi.fn(() => Promise.resolve()),
  onCloudAiConsentChanged: vi.fn(() => Promise.resolve(() => undefined)),
}))

import AiSection from './AiSection.svelte'
import { acceptCloudAiConsent, checkAiConnection, revokeCloudAiConsent } from '$lib/tauri-commands'
import { setSetting } from '$lib/settings'
import { _resetCloudConsentForTests } from '$lib/ai/cloud-consent.svelte'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) await Promise.resolve()
  await tick()
  flushSync()
}

async function mountSection(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(AiSection, { target, props: { searchQuery: '' } })
  mounted = { target, instance }
  await settle()
  return target
}

async function switchProvider(provider: string): Promise<void> {
  stubs.settings['ai.provider'] = provider
  for (const fn of stubs.listeners.get('ai.provider') ?? []) fn(provider)
  await settle()
}

function consentSwitch(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('input[data-test="cloud-ai-consent"]')
}

function cloudSetup(target: HTMLElement): HTMLElement | null {
  return target.querySelector<HTMLElement>('.cloud-setup')
}

beforeEach(() => {
  vi.clearAllMocks()
  _resetCloudConsentForTests()
  stubs.listeners.clear()
  stubs.consent.accepted = false
  stubs.settings = Object.create(null) as Record<string, unknown>
  stubs.settings['ai.provider'] = 'cloud'
  stubs.settings['ai.cloudProvider'] = 'openai'
  stubs.settings['ai.cloudProviderConfigs'] = '{}'
  stubs.settings['askCmdr.interactiveModel'] = ''
})

afterEach(() => {
  if (mounted) {
    void unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('Allow cloud AI in Settings > AI > Provider', () => {
  it('shows the switch on Cloud only: Local sends nothing off the Mac', async () => {
    stubs.settings['ai.provider'] = 'local'
    const target = await mountSection()
    expect(consentSwitch(target)).toBeNull()

    await switchProvider('cloud')
    expect(consentSwitch(target)).not.toBeNull()
  })

  it('locks the whole service setup while cloud AI is off, and says how to unlock it', async () => {
    const target = await mountSection()

    expect(cloudSetup(target)?.hasAttribute('inert')).toBe(true)
    expect(target.textContent).toContain('Turn on “Allow cloud AI” above to set up a service.')
    // The locked setup never probes the service: that alone would reach it.
    expect(checkAiConnection).not.toHaveBeenCalled()
  })

  it('unlocks the setup once cloud AI is on', async () => {
    stubs.consent.accepted = true
    const target = await mountSection()

    expect(cloudSetup(target)?.hasAttribute('inert')).toBe(false)
    expect(target.textContent).not.toContain('above to set up a service')
  })

  it('records consent on the switch, and unlocks without leaving the page', async () => {
    const target = await mountSection()

    consentSwitch(target)?.click()
    await settle()

    expect(acceptCloudAiConsent).toHaveBeenCalledOnce()
    expect(cloudSetup(target)?.hasAttribute('inert')).toBe(false)
    // The switch itself reads on, not only the setup below it.
    expect(consentSwitch(target)?.checked).toBe(true)
    expect(target.querySelector('.switch-control')?.getAttribute('data-state')).toBe('checked')
  })

  it('switching it off stops cloud AI and leaves the AI mode on Cloud', async () => {
    stubs.consent.accepted = true
    const target = await mountSection()

    consentSwitch(target)?.click()
    await settle()

    expect(revokeCloudAiConsent).toHaveBeenCalledOnce()
    expect(stubs.settings['ai.provider']).toBe('cloud')
    expect(vi.mocked(setSetting).mock.calls.some(([id]) => id === 'ai.provider')).toBe(false)
    expect(cloudSetup(target)?.hasAttribute('inert')).toBe(true)
  })

  it('moving Cloud to Local and back never grants or revokes consent', async () => {
    const target = await mountSection()

    await switchProvider('local')
    await switchProvider('cloud')

    expect(acceptCloudAiConsent).not.toHaveBeenCalled()
    expect(revokeCloudAiConsent).not.toHaveBeenCalled()
    expect(cloudSetup(target)?.hasAttribute('inert')).toBe(true)
  })
})
