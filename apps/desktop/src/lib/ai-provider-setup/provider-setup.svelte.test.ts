/**
 * The shared setup state machine: the race guards that keep a stale provider's answer off
 * the screen, the key-save debounce, and the auto-check gates.
 *
 * These used to live twice, once per surface, and only one copy was ever tested.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import type { ManagedAiRefusal } from '$lib/ipc/bindings'
import { ProviderSetupController } from './provider-setup.svelte'
import { clearModelCache } from '$lib/settings/ai-model-cache'

interface CheckResult {
  connected: boolean
  authError: boolean
  models: string[]
  error: string | null
  cloudConsentMissing?: boolean
  managed?: 'aiOff' | 'cloudAiOff' | 'hostNotAllowed' | null
}

// One object payload per spy, so the assertions name the argument they mean and the
// positional pair can't be silently swapped (`cmdr/no-confusable-callback-params`).
const checkAiConnection = vi.fn<(payload: { baseUrl: string; providerId: string }) => Promise<CheckResult>>()
const saveAiApiKey = vi.fn<(payload: { providerId: string; apiKey: string }) => Promise<null>>()
const getAiApiKeyStatus = vi.fn<(id: string) => Promise<{ isSet: boolean; fingerprint: string }>>()
const deleteAiApiKey = vi.fn<(id: string) => Promise<void>>()
const cloudAiHostVerdicts = vi.fn<(baseUrls: string[]) => Promise<(ManagedAiRefusal | null)[]>>()

vi.mock('$lib/tauri-commands', () => ({
  checkAiConnection: (baseUrl: string, providerId: string) => checkAiConnection({ baseUrl, providerId }),
  saveAiApiKey: (providerId: string, apiKey: string) => saveAiApiKey({ providerId, apiKey }),
  getAiApiKeyStatus: (id: string) => getAiApiKeyStatus(id),
  deleteAiApiKey: (id: string) => deleteAiApiKey(id),
  cloudAiHostVerdicts: (baseUrls: string[]) => cloudAiHostVerdicts(baseUrls),
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
    deleteAiApiKey.mockReset()
    deleteAiApiKey.mockResolvedValue(undefined)
    cloudAiHostVerdicts.mockReset()
    cloudAiHostVerdicts.mockImplementation((urls) => Promise.resolve(urls.map(() => null)))
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

  it("keeps the previous provider's key-save failure off the provider the user switched to", async () => {
    controller.setProvider('openai')
    await settle()
    let rejectSave: ((error: Error) => void) | undefined
    saveAiApiKey.mockImplementationOnce(
      () =>
        new Promise<null>((_resolve, reject) => {
          rejectSave = reject
        }),
    )
    controller.handleApiKeyChange('sk-typed-for-openai')
    // The switch flushes the pending save against openai; it fails only after anthropic is up.
    controller.setProvider('anthropic')
    await settle()
    rejectSave?.(new Error('keyring locked'))
    await settle()

    expect(controller.providerId).toBe('anthropic')
    expect(controller.secretError).toBeNull()
  })

  it("keeps the previous provider's key-removal failure off the provider the user switched to", async () => {
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    let rejectDelete: ((error: Error) => void) | undefined
    deleteAiApiKey.mockImplementationOnce(
      () =>
        new Promise<void>((_resolve, reject) => {
          rejectDelete = reject
        }),
    )
    const removal = controller.removeApiKey()
    controller.setProvider('anthropic')
    await settle()
    rejectDelete?.(new Error('keyring locked'))
    await removal

    expect(controller.secretError).toBeNull()
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

  it('reads a refused probe (cloud AI not allowed yet) as idle, not as a connection problem', async () => {
    // The backend sent nothing, so "couldn't connect" would be a false claim about the service.
    checkAiConnection.mockResolvedValue({
      connected: false,
      authError: false,
      models: [],
      error: null,
      cloudConsentMissing: true,
    })
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
    controller.setProvider('openai')
    await settle()
    expect(controller.status).toBe('idle')
    expect(controller.error).toBeNull()
  })

  describe("the organization's policy", () => {
    it('reads a check the policy refused as managed, with the rule, not as a connection problem', async () => {
      checkAiConnection.mockResolvedValue({
        connected: false,
        authError: false,
        models: [],
        error: null,
        managed: 'hostNotAllowed',
      })
      getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
      controller.setProvider('openai')
      await settle()
      expect(controller.status).toBe('managed')
      expect(controller.managedRefusal).toBe('hostNotAllowed')
      expect(controller.error).toBeNull()
    })

    it('says a refused preset is refused on open, without a key and without probing it', async () => {
      cloudAiHostVerdicts.mockResolvedValue(['hostNotAllowed'])
      controller.setProvider('openai')
      await settle()
      expect(cloudAiHostVerdicts).toHaveBeenCalledWith(['https://api.openai.com/v1'])
      expect(controller.status).toBe('managed')
      expect(controller.managedRefusal).toBe('hostNotAllowed')
      expect(checkAiConnection).not.toHaveBeenCalled()
    })

    it('shows the reason the backend gave, never one it works out itself', async () => {
      // The policy flipped to on-device only while the picker was open: the reason is cloud off.
      cloudAiHostVerdicts.mockResolvedValue(['cloudAiOff'])
      controller.setProvider('openai')
      await settle()
      expect(controller.status).toBe('managed')
      expect(controller.managedRefusal).toBe('cloudAiOff')
    })

    it('checks a typed endpoint once it is entered, even before there is a key', async () => {
      vi.useFakeTimers()
      try {
        controller.setProvider('azure-openai')
        await vi.runAllTimersAsync()
        cloudAiHostVerdicts.mockResolvedValue(['hostNotAllowed'])
        controller.saveBaseUrl('https://elsewhere.example/v1')
        await vi.runAllTimersAsync()
        expect(cloudAiHostVerdicts).toHaveBeenLastCalledWith(['https://elsewhere.example/v1'])
        expect(controller.status).toBe('managed')
        expect(checkAiConnection).not.toHaveBeenCalled()

        // Moving to a host the policy allows lifts it again.
        cloudAiHostVerdicts.mockResolvedValue([null])
        controller.saveBaseUrl('https://tenant.openai.azure.com/openai/v1')
        await vi.runAllTimersAsync()
        expect(controller.status).toBe('idle')
        expect(controller.managedRefusal).toBeNull()
      } finally {
        vi.useRealTimers()
      }
    })

    it('drops a refusal for a provider the user already left', async () => {
      let releaseFirst: ((verdicts: (ManagedAiRefusal | null)[]) => void) | undefined
      cloudAiHostVerdicts.mockImplementationOnce(
        () =>
          new Promise<(ManagedAiRefusal | null)[]>((resolve) => {
            releaseFirst = resolve
          }),
      )
      controller.setProvider('openai')
      await settle()
      controller.setProvider('anthropic')
      await settle()
      releaseFirst?.(['hostNotAllowed'])
      await settle()
      expect(controller.providerId).toBe('anthropic')
      expect(controller.status).not.toBe('managed')
    })
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

  describe('removing a saved key', () => {
    it('takes the key out of the store and forgets everything it unlocked', async () => {
      const changes: string[] = []
      controller = new ProviderSetupController({ logScope: 'test', onKeyChanged: () => changes.push('changed') })
      getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
      controller.setProvider('openai')
      await settle()
      expect(controller.isConnected).toBe(true)

      await controller.removeApiKey()

      expect(deleteAiApiKey).toHaveBeenCalledWith('openai')
      expect(controller.keyIsSet).toBe(false)
      expect(controller.status).toBe('idle')
      expect(controller.models).toEqual([])
      expect(controller.hasCheckableConfig).toBe(false)
      // So Settings re-pushes the AI config and the backend stops using the old key.
      expect(changes).toEqual(['changed'])
    })

    it('drops a key still in the save debounce instead of saving it after the removal', async () => {
      getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
      controller.setProvider('openai')
      await settle()
      controller.handleApiKeyChange('sk-half-typed')

      await controller.removeApiKey()
      await new Promise((resolve) => setTimeout(resolve, 400))

      expect(saveAiApiKey).not.toHaveBeenCalled()
      expect(controller.apiKey).toBe('')
    })

    it('keeps the key on screen and says why when the store refuses', async () => {
      getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'fp' })
      deleteAiApiKey.mockRejectedValue(new Error('keyring locked'))
      controller.setProvider('openai')
      await settle()

      await controller.removeApiKey()

      expect(controller.keyIsSet).toBe(true)
      expect(controller.secretError?.title).toContain('remove your saved API key')
    })
  })
})
