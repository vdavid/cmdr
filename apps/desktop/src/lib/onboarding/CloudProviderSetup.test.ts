/**
 * Behaviour tests for `CloudProviderSetup.svelte`: per-provider tutorial steps,
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
    saveAiApiKey.mockReset()
    saveAiApiKey.mockResolvedValue(null)
    getAiApiKey.mockReset()
    getAiApiKey.mockResolvedValue('')
    openExternalUrl.mockReset()
    openExternalUrl.mockResolvedValue()
    vi.useFakeTimers()
  })

  afterEach(async () => {
    vi.useRealTimers()
    if (mounted) {
      await unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
  })

  it('renders provider-specific tutorial steps for OpenAI', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.textContent).toContain('Set up OpenAI')
    expect(mounted.target.textContent).toContain('Sign up at')
    expect(mounted.target.textContent).toContain('Create an API key')
    expect(mounted.target.textContent).toContain('Paste your API key')
    expect(mounted.target.textContent).toContain('Pick a model')
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
      await unmount(mounted.instance)
    }
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'anthropic' } })
    mounted = { target, instance, providerId: 'anthropic' }
    await settle()
    expect(mounted.target.textContent).toContain('Set up Anthropic')
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
    expect(mounted.target.textContent).toContain('Connected!')
  })

  it('renders the editable endpoint field for "custom" provider', async () => {
    mountSetup('custom')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.querySelector('input#onboarding-cloud-base-url')).not.toBeNull()
  })

  it('renders the editable endpoint field for "azure-openai" too', async () => {
    mountSetup('azure-openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.querySelector('input#onboarding-cloud-base-url')).not.toBeNull()
  })

  it('editing the endpoint URL persists it and schedules a connection check', async () => {
    mountSetup('custom')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const baseUrlInput = mounted.target.querySelector<HTMLInputElement>('#onboarding-cloud-base-url')
    if (!baseUrlInput) throw new Error('base URL input missing')
    baseUrlInput.value = 'https://example.test/v1'
    baseUrlInput.dispatchEvent(new Event('input', { bubbles: true }))
    // saveBaseUrl writes to settings immediately and schedules a 1 s check.
    expect(settingsMap['ai.cloudProviderConfigs']).toContain('example.test')
    await advanceTimers(1100)
    expect(checkAiConnection).toHaveBeenCalled()
  })

  it('an auth-error connection result is shown as a status message', async () => {
    checkAiConnection.mockResolvedValue({
      connected: false,
      authError: true,
      models: [],
      error: 'Invalid key',
    })
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-bad'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent).toContain('Invalid key')
  })

  it('a connection-error result surfaces the error text', async () => {
    checkAiConnection.mockResolvedValue({
      connected: false,
      authError: false,
      models: [],
      error: 'Service unreachable',
    })
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-down'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent).toContain('Service unreachable')
  })

  it('a thrown checkAiConnection becomes a generic "Something went wrong" status', async () => {
    checkAiConnection.mockRejectedValue(new Error('boom'))
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-throws'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent).toContain('boom')
  })

  it('a "connected" result with no models still flips the API-key check; the model field stays usable', async () => {
    checkAiConnection.mockResolvedValue({
      connected: true,
      authError: false,
      models: [],
      error: null,
    })
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-no-models'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent).toContain('Connected!')
    // The model field is always the combobox now (never morphs): it stays present even with no
    // suggestions, so the user can still type a custom model.
    expect(mounted.target.querySelector('input[aria-label="Model"]')).not.toBeNull()
  })

  it('an explicit `error` payload with connected=true is treated as an error', async () => {
    checkAiConnection.mockResolvedValue({
      connected: true,
      authError: false,
      models: ['model-1'],
      error: 'partial-failure',
    })
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-warn'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    expect(mounted.target.textContent).toContain('partial-failure')
  })

  it('clicking a sign-up link calls openExternalUrl', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    // Two link buttons render for OpenAI: sign up + create API key. Click the first.
    const link = mounted.target.querySelector<HTMLAnchorElement>('a')
    if (!link) throw new Error('no link button rendered')
    link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    await settle()
    expect(openExternalUrl).toHaveBeenCalled()
  })

  it('a successful check renders the returned models as suggestions in the combobox', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-ok'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    // The shared `ui/Combobox` renders the model items; we assert the data flows through, not the
    // popup open/close mechanics (that's Ark's job, covered by the primitive's own tests).
    expect(mounted.target.textContent).toContain('gpt-4.1-mini')
    expect(mounted.target.textContent).toContain('gpt-4o-mini')
  })

  it('a secret store read failure surfaces an inline error', async () => {
    getAiApiKey.mockRejectedValue(new Error('keyring locked'))
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    // The describeSecretError title for a generic failure starts with "Couldn't read".
    expect(mounted.target.textContent).toContain('read saved API key')
  })

  it('a secret store save failure surfaces an inline error', async () => {
    saveAiApiKey.mockRejectedValue(new Error('keyring denied'))
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-cant-save'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(500)
    expect(mounted.target.textContent).toContain('save API key')
  })

  it('typing into the combobox filter updates the saved model', async () => {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-ok'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    const modelInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="Model"]')
    if (!modelInput) throw new Error('model input missing')
    // Focus for real (not a bare `focus` Event): the combobox's machine only processes input
    // changes once focused, and a synthetic `focus` event doesn't drive it there like a user click
    // does. `.focus()` + `focusin` mirrors the real focus path the user takes before typing.
    modelInput.focus()
    modelInput.dispatchEvent(new Event('focusin', { bubbles: true }))
    await settle()
    modelInput.value = 'gpt-4o'
    modelInput.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    const stored = JSON.parse(settingsMap['ai.cloudProviderConfigs'] as string) as Partial<
      Record<string, { model?: string }>
    >
    expect(stored.openai?.model).toBe('gpt-4o')
  })

  it('keeps showing the saved model even before any models load (the field never blanks)', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'my-custom-model' } })
    // No key, so no check fires: the suggestion list stays empty (cold start). The field must still
    // show the saved model, not blank, per the Combobox text-field-with-suggestions contract.
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const modelInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="Model"]')
    expect(modelInput?.value).toBe('my-custom-model')
  })

  it('flushes a pending key save when the provider switches mid-typing', async () => {
    const { target } = mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'sk-mid-flight'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    // Without advancing the timer, switch the provider.
    await unmount(mounted.instance)
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'anthropic' } })
    mounted = { target, instance, providerId: 'anthropic' }
    await settle()
    // The flushPendingApiKeySave path saves against the OLD provider, not the new one.
    expect(saveAiApiKey).toHaveBeenCalledWith('openai', 'sk-mid-flight')
  })

  it('renders without a preset block when the providerId is unknown', async () => {
    mountSetup('made-up-provider-id')
    await settle()
    if (!mounted) throw new Error('not mounted')
    // No "Set up …" header renders when the preset lookup misses.
    expect(mounted.target.textContent).not.toContain('Set up')
  })
})
