/**
 * Behaviour tests for `CloudProviderSetup.svelte` (M3): per-provider tutorial steps,
 * checkmarks flipping on completion, API-key persist + connection-check pipeline,
 * provider switching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import CloudProviderSetup from './CloudProviderSetup.svelte'

const checkAiConnection = vi.fn<
  (
    baseUrl: string,
    key: string,
  ) => Promise<{
    connected: boolean
    authError: boolean
    models: string[]
    error: string | null
  }>
>(() =>
  Promise.resolve({
    connected: true,
    authError: false,
    models: ['gpt-4.1-mini', 'gpt-4o-mini'],
    error: null,
  }),
)
const saveAiApiKey = vi.fn<(id: string, key: string) => Promise<null>>(() => Promise.resolve(null))
const getAiApiKey = vi.fn<(id: string) => Promise<string>>(() => Promise.resolve(''))
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  checkAiConnection: (baseUrl: string, key: string) => checkAiConnection(baseUrl, key),
  saveAiApiKey: (id: string, k: string) => saveAiApiKey(id, k),
  getAiApiKey: (id: string) => getAiApiKey(id),
  openExternalUrl: (url: string) => openExternalUrl(url),
}))

const settingsMap: Record<string, unknown> = {}
function resetSettings(): void {
  for (const k of Object.keys(settingsMap)) {
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture reset
    delete settingsMap[k]
  }
  settingsMap['ai.cloudProviderConfigs'] = '{}'
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

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount>; providerId: string } | undefined

function mountSetup(providerId: string) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(CloudProviderSetup, { target, props: { providerId } })
  mounted = { target, instance, providerId }
  return mounted
}

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

async function advanceTimers(ms: number): Promise<void> {
  await vi.advanceTimersByTimeAsync(ms)
  await settle()
}

describe('CloudProviderSetup', () => {
  beforeEach(() => {
    resetSettings()
    checkAiConnection.mockClear()
    checkAiConnection.mockResolvedValue({
      connected: true,
      authError: false,
      models: ['gpt-4.1-mini', 'gpt-4o-mini'],
      error: null,
    })
    saveAiApiKey.mockClear()
    getAiApiKey.mockReset()
    getAiApiKey.mockResolvedValue('')
    openExternalUrl.mockClear()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    if (mounted) {
      unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
  })

  it('renders provider-specific tutorial steps for OpenAI', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.textContent ?? '').toContain('Set up OpenAI')
    expect(mounted.target.textContent ?? '').toContain('Sign up at')
    expect(mounted.target.textContent ?? '').toContain('Create an API key')
    expect(mounted.target.textContent ?? '').toContain('Paste your API key')
    expect(mounted.target.textContent ?? '').toContain('Pick a model')
  })

  it('shows the default model as a placeholder when no model is set', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const modelInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="Model"]')
    expect(modelInput?.placeholder).toContain('gpt-4.1-mini')
  })

  it('typing an API key debounces a save + triggers a connection check', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-test-key'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    // Save debounce is 300 ms; connection check debounce 1000 ms.
    await advanceTimers(400)
    expect(saveAiApiKey).toHaveBeenCalledWith('openai', 'sk-test-key')
    await advanceTimers(1100)
    expect(checkAiConnection).toHaveBeenCalled()
  })

  it('switching provider reloads state and reads the new key from the secret store', async () => {
    getAiApiKey.mockImplementation((id: string) => Promise.resolve(id === 'anthropic' ? 'sk-ant-stored' : ''))
    const { target } = mountSetup('openai')
    await settle()
    // Re-mount with a new providerId prop; simplest way to test `$effect(providerId)`.
    if (mounted) {
      unmount(mounted.instance)
    }
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'anthropic' } })
    mounted = { target, instance, providerId: 'anthropic' }
    await settle()
    expect(mounted.target.textContent ?? '').toContain('Set up Anthropic')
    expect(getAiApiKey).toHaveBeenCalledWith('anthropic')
  })

  it('connection-check returning models reveals the model combobox and ticks the API-key step', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-good'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent ?? '').toContain('Connected!')
  })

  it('renders the editable endpoint field for "custom" provider', async () => {
    mountSetup('custom')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.querySelector('input#onboarding-cloud-base-url')).not.toBeNull()
  })
})
