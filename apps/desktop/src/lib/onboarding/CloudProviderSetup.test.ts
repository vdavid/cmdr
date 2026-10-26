/**
 * Behaviour tests for `CloudProviderSetup.svelte`: per-provider tutorial steps,
 * checkmarks flipping on completion, API-key persist + connection-check pipeline,
 * provider switching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import type { AiConnectionCheckResult, ManagedAiRefusal } from '#lib/ipc/bindings.js'
import CloudProviderSetup from './CloudProviderSetup.svelte'

/** A check answer: connected with two models unless the overrides say otherwise. */
function checkResult(overrides: Partial<AiConnectionCheckResult>): AiConnectionCheckResult {
  return {
    models: ['gpt-4.1-mini', 'gpt-4o-mini'],
    problem: null,
    cloudConsentMissing: false,
    managed: null,
    ...overrides,
  }
}

const checkAiConnection = vi.fn<(payload: { baseUrl: string; providerId: string }) => Promise<AiConnectionCheckResult>>(
  () => Promise.resolve(checkResult({})),
)
const saveAiApiKey = vi.fn<(payload: { providerId: string; apiKey: string }) => Promise<null>>(() =>
  Promise.resolve(null),
)
const getAiApiKeyStatus = vi.fn<(id: string) => Promise<{ isSet: boolean; fingerprint: string }>>(() =>
  Promise.resolve({ isSet: false, fingerprint: '' }),
)
const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const cloudAiHostVerdicts = vi.fn<(baseUrls: string[]) => Promise<(ManagedAiRefusal | null)[]>>((urls) =>
  Promise.resolve(urls.map(() => null)),
)

// The shared controller suppresses its auto-check on open under `isE2eRun()`. These tests are
// about the everyday path, so they pin it false rather than lean on an unresolved mode; the
// suppression itself is covered in `#lib/ai-provider-setup/provider-setup.svelte.test.ts`.
vi.mock('#lib/app-mode.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isE2eRun: () => false,
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  checkAiConnection: (baseUrl: string, providerId: string) => checkAiConnection({ baseUrl, providerId }),
  saveAiApiKey: (providerId: string, apiKey: string) => saveAiApiKey({ providerId, apiKey }),
  getAiApiKeyStatus: (id: string) => getAiApiKeyStatus(id),
  openExternalUrl: (url: string) => openExternalUrl(url),
  cloudAiHostVerdicts: (baseUrls: string[]) => cloudAiHostVerdicts(baseUrls),
}))

const settingsMap: Record<string, unknown> = {}
function resetSettings(): void {
  for (const k of Object.keys(settingsMap)) {
    delete settingsMap[k]
  }
  settingsMap['ai.cloudProviderConfigs'] = '{}'
}

vi.mock('#lib/settings/index.js', async (importOriginal) => {
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
    checkAiConnection.mockResolvedValue(checkResult({}))
    saveAiApiKey.mockReset()
    saveAiApiKey.mockResolvedValue(null)
    getAiApiKeyStatus.mockReset()
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })
    openExternalUrl.mockReset()
    openExternalUrl.mockResolvedValue()
    cloudAiHostVerdicts.mockReset()
    cloudAiHostVerdicts.mockImplementation((urls) => Promise.resolve(urls.map(() => null)))
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
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-test-key' })
    await advanceTimers(1100)
    expect(checkAiConnection).toHaveBeenCalled()
  })

  it("switching provider reloads state and re-reads the new provider's key status", async () => {
    getAiApiKeyStatus.mockImplementation((id: string) =>
      Promise.resolve(id === 'anthropic' ? { isSet: true, fingerprint: 'abc123' } : { isSet: false, fingerprint: '' }),
    )
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
    expect(getAiApiKeyStatus).toHaveBeenCalledWith('anthropic')
  })

  /** The security contract: a saved key never comes back into a window, so the field can't be
   *  pre-filled with one. It shows a "your key is saved" placeholder instead. */
  it('a saved key leaves the field empty and shows the saved placeholder', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc123' })
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    expect(keyInput.value).toBe('')
    expect(keyInput.placeholder).toContain('saved')
  })

  /** A stored key still makes the config checkable, even though the window can't see the key. */
  it('checks the connection on open when a key is already stored', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc123' })
    mountSetup('openai')
    // The on-open path asks the model cache first, and its key is a Web Crypto digest, so
    // the check lands a macrotask later than the mount.
    await advanceTimers(1)
    expect(checkAiConnection).toHaveBeenCalledWith({
      baseUrl: expect.stringContaining('openai.com') as string,
      providerId: 'openai',
    })
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
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = 'custom-test-key'
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(400)
    checkAiConnection.mockClear()
    const baseUrlInput = mounted.target.querySelector<HTMLInputElement>('#onboarding-cloud-base-url')
    if (!baseUrlInput) throw new Error('base URL input missing')
    baseUrlInput.value = 'https://example.test/v1'
    baseUrlInput.dispatchEvent(new Event('input', { bubbles: true }))
    // saveBaseUrl writes to settings immediately and schedules a 1 s check.
    expect(settingsMap['ai.cloudProviderConfigs']).toContain('example.test')
    await advanceTimers(1100)
    expect(checkAiConnection).toHaveBeenCalled()
  })

  /** Types `apiKey` into the key field and waits out the save and check debounces. */
  async function typeKeyAndWait(apiKey: string): Promise<HTMLElement> {
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    const keyInput = mounted.target.querySelector<HTMLInputElement>('input[aria-label="API key"]')
    if (!keyInput) throw new Error('API key input missing')
    keyInput.value = apiKey
    keyInput.dispatchEvent(new Event('input', { bubbles: true }))
    await advanceTimers(1500)
    return mounted.target
  }

  it('a rejected key reads as its own translated line', async () => {
    checkAiConnection.mockResolvedValue(checkResult({ problem: { kind: 'keyRejected' } }))
    const target = await typeKeyAndWait('sk-bad')
    expect(target.textContent).toContain('That key didn’t work')
  })

  it("says the organization doesn't allow a refused service, and never probes it", async () => {
    cloudAiHostVerdicts.mockResolvedValue(['hostNotAllowed'])
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    expect(mounted.target.textContent).toContain('Your organization doesn’t allow this AI service.')
    expect(checkAiConnection).not.toHaveBeenCalled()
  })

  it('an unreachable service reads as the connection line', async () => {
    checkAiConnection.mockResolvedValue(checkResult({ problem: { kind: 'unreachable' } }))
    const target = await typeKeyAndWait('sk-down')
    expect(target.textContent).toContain('Can’t reach the service right now')
  })

  it('an insecure endpoint tells the user to use https://', async () => {
    checkAiConnection.mockResolvedValue(checkResult({ problem: { kind: 'insecureEndpoint' } }))
    const target = await typeKeyAndWait('sk-http')
    expect(target.textContent).toContain('use an address starting with https://')
  })

  it("a provider's refusal shows a translated lead line plus the provider's own sentence", async () => {
    checkAiConnection.mockResolvedValue(
      checkResult({ problem: { kind: 'providerRefused', status: 429, detail: 'Quota exceeded for this project.' } }),
    )
    const target = await typeKeyAndWait('sk-quota')
    expect(target.textContent).toContain('The service turned the request down (code 429).')
    expect(target.querySelector('.status-detail')?.textContent).toBe('Quota exceeded for this project.')
  })

  it('a thrown checkAiConnection becomes a generic "Something went wrong" status, never the raw message', async () => {
    checkAiConnection.mockRejectedValue(new Error('boom'))
    const target = await typeKeyAndWait('sk-throws')
    expect(target.textContent).toContain('Something went wrong')
    expect(target.textContent).not.toContain('boom')
  })

  it('a "connected" result with no models still flips the API-key check; the model field stays usable', async () => {
    checkAiConnection.mockResolvedValue(checkResult({ models: [] }))
    const target = await typeKeyAndWait('sk-no-models')
    expect(target.textContent).toContain('Connected!')
    // The model field is always the combobox now (never morphs): it stays present even with no
    // suggestions, so the user can still type a custom model.
    expect(target.querySelector('input[aria-label="Model"]')).not.toBeNull()
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
    // popup open/close mechanics (that's Ark's job, covered by the primitive's own tests). The popup
    // portals to `document.body`, so its rows are there, not under the mount target.
    expect(document.body.textContent).toContain('gpt-4.1-mini')
    expect(document.body.textContent).toContain('gpt-4o-mini')
  })

  it('a secret store read failure surfaces an inline error', async () => {
    getAiApiKeyStatus.mockRejectedValue(new Error('keyring locked'))
    mountSetup('openai')
    await settle()
    if (!mounted) throw new Error('not mounted')
    // The describeSecretError title for a generic failure starts with "Couldn't read".
    expect(mounted.target.textContent).toContain('read your saved API key')
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
    expect(mounted.target.textContent).toContain('save your API key')
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
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-mid-flight' })
  })

  it('renders without a preset block when the providerId is unknown', async () => {
    mountSetup('made-up-provider-id')
    await settle()
    if (!mounted) throw new Error('not mounted')
    // No "Set up …" header renders when the preset lookup misses.
    expect(mounted.target.textContent).not.toContain('Set up')
  })
})
