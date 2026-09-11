/**
 * The shared setup state machine: the race guards that keep a stale provider's answer off
 * the screen, the key-save debounce, and the auto-check gates.
 *
 * These used to live twice, once per surface, and only one copy was ever tested.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { ProviderSetupController } from './provider-setup.svelte'
import { clearModelCache } from '$lib/settings/ai-model-cache'

interface CheckResult {
  connected: boolean
  authError: boolean
  models: string[]
  error: string | null
}

// One object payload per spy, so the assertions name the argument they mean and the
// positional pair can't be silently swapped (`cmdr/no-confusable-callback-params`).
const checkAiConnection = vi.fn<(payload: { baseUrl: string; providerId: string }) => Promise<CheckResult>>()
const saveAiApiKey = vi.fn<(payload: { providerId: string; apiKey: string }) => Promise<null>>()
const getAiApiKeyStatus = vi.fn<(id: string) => Promise<{ isSet: boolean; fingerprint: string }>>()

vi.mock('$lib/tauri-commands', () => ({
  checkAiConnection: (baseUrl: string, providerId: string) => checkAiConnection({ baseUrl, providerId }),
  saveAiApiKey: (providerId: string, apiKey: string) => saveAiApiKey({ providerId, apiKey }),
  getAiApiKeyStatus: (id: string) => getAiApiKeyStatus(id),
}))

const settingsMap: Record<string, unknown> = {}
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

// The real `isE2eRun()` reads a mode resolved over IPC, which a unit test never has. This mock
// puts it under the test's control instead.
const e2e = { on: false }
vi.mock('$lib/app-mode', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isE2eRun: () => e2e.on,
}))

/**
 * Lets the controller's chained promises settle. Two macrotask yields, not just a
 * microtask drain: the model-cache key is a Web Crypto digest, which resolves a tick out.
 */
async function settle(): Promise<void> {
  for (let round = 0; round < 3; round++) {
    await new Promise((resolve) => setTimeout(resolve, 0))
    for (let i = 0; i < 20; i++) {
      await Promise.resolve()
    }
  }
}

function connected(models: string[]): CheckResult {
  return { connected: true, authError: false, models, error: null }
}

let controller: ProviderSetupController

describe('ProviderSetupController', () => {
  beforeEach(() => {
    for (const key of Object.keys(settingsMap)) delete settingsMap[key]
    settingsMap['ai.cloudProviderConfigs'] = '{}'
    clearModelCache()
    e2e.on = false
    checkAiConnection.mockReset()
    checkAiConnection.mockResolvedValue(connected(['gpt-4.1-mini']))
    saveAiApiKey.mockReset()
    saveAiApiKey.mockResolvedValue(null)
    getAiApiKeyStatus.mockReset()
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })
    controller = new ProviderSetupController({ logScope: 'test' })
  })

  afterEach(() => {
    controller.destroy()
  })

  it('loads the preset endpoint and default model when nothing is stored', async () => {
    controller.setProvider('openai')
    await settle()
    expect(controller.resolvedBaseUrl).toBe('https://api.openai.com/v1')
    expect(controller.model).toBe('gpt-4.1-mini')
    expect(controller.apiKey).toBe('')
  })

  it('prefers the user endpoint over the preset for a provider that has an editable one', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ custom: { model: 'm', baseUrl: 'https://mine.test/v1' } })
    controller.setProvider('custom')
    await settle()
    expect(controller.resolvedBaseUrl).toBe('https://mine.test/v1')
  })

  /** The guard that stops one provider's answer from being rendered against another. */
  it('drops a connection result that lands after the user switched providers', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    let releaseFirst: ((result: CheckResult) => void) | undefined
    checkAiConnection.mockImplementationOnce(
      () =>
        new Promise<CheckResult>((resolve) => {
          releaseFirst = resolve
        }),
    )
    checkAiConnection.mockResolvedValue(connected(['claude-sonnet-4-5']))

    controller.setProvider('openai')
    await settle()
    controller.setProvider('anthropic')
    await settle()

    releaseFirst?.(connected(['gpt-4.1-mini']))
    await settle()

    expect(controller.providerId).toBe('anthropic')
    expect(controller.models).toEqual(['claude-sonnet-4-5'])
  })

  /** Same guard, one layer down: the keychain read is async too. */
  it('drops a key-status read that lands after the user switched providers', async () => {
    let releaseFirst: ((status: { isSet: boolean; fingerprint: string }) => void) | undefined
    getAiApiKeyStatus.mockImplementationOnce(
      () =>
        new Promise<{ isSet: boolean; fingerprint: string }>((resolve) => {
          releaseFirst = resolve
        }),
    )
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })

    controller.setProvider('openai')
    controller.setProvider('anthropic')
    await settle()
    releaseFirst?.({ isSet: true, fingerprint: 'openai-fp' })
    await settle()

    expect(controller.keyIsSet).toBe(false)
  })

  it('checks a stored key on open, so returning users hear whether it still works', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    expect(checkAiConnection).toHaveBeenCalledWith({ baseUrl: 'https://api.openai.com/v1', providerId: 'openai' })
    expect(controller.isConnected).toBe(true)
  })

  it('makes no unprompted request in an automated run', async () => {
    e2e.on = true
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    expect(checkAiConnection).not.toHaveBeenCalled()
  })

  it('saves a typed key against the provider it was typed for, even after a switch', async () => {
    controller.setProvider('openai')
    await settle()
    controller.handleApiKeyChange('sk-typed-for-openai')
    // Switch inside the 300 ms save debounce: the flush has to target the OLD entry.
    controller.setProvider('anthropic')
    await settle()
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-typed-for-openai' })
  })

  it('treats an auth failure as an auth failure, not a generic one', async () => {
    checkAiConnection.mockResolvedValue({ connected: false, authError: true, models: [], error: 'Invalid key' })
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    expect(controller.status).toBe('auth-error')
    expect(controller.error).toBe('Invalid key')
    expect(controller.isConnected).toBe(false)
  })

  it('reports a secret-store read failure to its owner as well as its own state', async () => {
    const seen: (string | null)[] = []
    controller = new ProviderSetupController({
      logScope: 'test',
      onSecretErrorChange: (error) => seen.push(error?.title ?? null),
    })
    getAiApiKeyStatus.mockRejectedValue(new Error('keyring locked'))
    controller.setProvider('openai')
    await settle()
    expect(controller.secretError?.title).toContain('read your saved API key')
    expect(seen.at(-1)).toContain('read your saved API key')
  })

  it('serves a second visit from the model cache instead of hitting the network again', async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    expect(checkAiConnection).toHaveBeenCalledTimes(1)

    const second = new ProviderSetupController({ logScope: 'test' })
    second.setProvider('openai')
    await settle()
    expect(checkAiConnection).toHaveBeenCalledTimes(1)
    expect(second.models).toEqual(['gpt-4.1-mini'])
    second.destroy()
  })

  it('does not offer to check a key-backed provider until there is a key', async () => {
    controller.setProvider('openai')
    await settle()
    expect(controller.hasCheckableConfig).toBe(false)
    expect(checkAiConnection).not.toHaveBeenCalled()
  })

  it('is ready to check a local provider straight away: it needs no key', async () => {
    controller.setProvider('ollama')
    await settle()
    expect(controller.hasCheckableConfig).toBe(true)
  })
})
