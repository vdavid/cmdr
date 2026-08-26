/**
 * Unit tests for `ai-config.ts`: AI configuration plumbing shared by Settings, the onboarding
 * wizard, and the settings-applier listener.
 *
 * Covers the two exports:
 * 1. `migrateApiKeysFromSettings()`: lifts pre-launch `apiKey` strings from settings.json into
 *    the OS secret store. Per-provider semantics: failure for one provider leaves that entry in
 *    settings.json; others still migrate.
 * 2. `pushConfigToBackend()`: read-fresh push of the current AI config to Rust. Surfaces secret
 *    store failures as a deduped persistent toast and keeps pushing the rest of the config so the
 *    user sees something rather than a silent backend.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

interface KeyStatus {
  isSet: boolean
  fingerprint: string
}
interface ConfigureOutcome {
  secretStoreError: unknown
}

const saveAiApiKey = vi.fn<(payload: { providerId: string; apiKey: string }) => Promise<null>>(() =>
  Promise.resolve(null),
)
const getAiApiKeyStatus = vi.fn<(id: string) => Promise<KeyStatus>>(() =>
  Promise.resolve({ isSet: false, fingerprint: '' }),
)
const configureAi = vi.fn<
  (payload: {
    provider: string
    contextSize: number
    cloudProviderId: string
    cloudBaseUrl: string
    cloudModel: string
    cloudRequiresApiKey: boolean
  }) => Promise<ConfigureOutcome>
>(() => Promise.resolve({ secretStoreError: null }))

vi.mock('$lib/tauri-commands', () => ({
  saveAiApiKey: (providerId: string, apiKey: string) => saveAiApiKey({ providerId, apiKey }),
  getAiApiKeyStatus: (id: string) => getAiApiKeyStatus(id),
  configureAi: (
    provider: string,
    contextSize: number,
    cloudProviderId: string,
    cloudBaseUrl: string,
    cloudModel: string,
    cloudRequiresApiKey: boolean,
  ) => configureAi({ provider, contextSize, cloudProviderId, cloudBaseUrl, cloudModel, cloudRequiresApiKey }),
}))

const settingsMap: Record<string, string> = {}
// Backs the raw-store helpers (`getRawStoreValue`/`deleteRawStoreKeys`) the legacy-key migration
// uses for non-registry keys; in a real app these hit the Tauri store plugin.
const rawStoreMap: Record<string, string> = {}
vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id] ?? '',
    setSetting: (id: string, value: string) => {
      settingsMap[id] = value
    },
    getRawStoreValue: (key: string) => Promise.resolve(rawStoreMap[key]),
    deleteRawStoreKeys: (keys: readonly string[]) => {
      for (const k of keys) {
        delete rawStoreMap[k]
      }
      return Promise.resolve()
    },
  }
})

const addToast = vi.fn<(...args: unknown[]) => void>()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...args)
  },
}))

const loggerWarn = vi.fn<(...args: unknown[]) => void>()
const loggerInfo = vi.fn<(...args: unknown[]) => void>()
const loggerError = vi.fn<(...args: unknown[]) => void>()
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    warn: (...args: unknown[]) => {
      loggerWarn(...args)
    },
    info: (...args: unknown[]) => {
      loggerInfo(...args)
    },
    error: (...args: unknown[]) => {
      loggerError(...args)
    },
    debug: () => {},
  }),
}))

// Import AFTER mocks are wired so the module captures the mocked references.
import { migrateApiKeysFromSettings, pushConfigToBackend } from './ai-config'

function resetState(): void {
  for (const k of Object.keys(settingsMap)) {
    delete settingsMap[k]
  }
  for (const k of Object.keys(rawStoreMap)) {
    delete rawStoreMap[k]
  }
  saveAiApiKey.mockReset()
  saveAiApiKey.mockResolvedValue(null)
  getAiApiKeyStatus.mockReset()
  getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })
  configureAi.mockReset()
  configureAi.mockResolvedValue({ secretStoreError: null })
  addToast.mockReset()
  loggerWarn.mockReset()
  loggerInfo.mockReset()
  loggerError.mockReset()
}

describe('migrateApiKeysFromSettings', () => {
  beforeEach(resetState)

  it('returns silently when ai.cloudProviderConfigs is missing', async () => {
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
  })

  it('returns silently when ai.cloudProviderConfigs is not valid JSON', async () => {
    settingsMap['ai.cloudProviderConfigs'] = 'not-json-{'
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
  })

  it('migrates a legacy apiKey to the secret store and removes it from settings.json', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: 'sk-legacy', model: 'gpt-4o' },
    })
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-legacy' })
    const updated = JSON.parse(settingsMap['ai.cloudProviderConfigs']) as Record<string, unknown>
    const openai = updated.openai as Record<string, unknown>
    expect(openai.apiKey).toBeUndefined()
    expect(openai.model).toBe('gpt-4o')
    expect(loggerInfo).toHaveBeenCalled()
  })

  it('migrates multiple providers in one pass', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: 'sk-one', model: 'gpt-4o' },
      anthropic: { apiKey: 'sk-ant', model: 'claude' },
    })
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-one' })
    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'anthropic', apiKey: 'sk-ant' })
  })

  it('keeps the legacy entry in settings.json when the secret store rejects the save', async () => {
    saveAiApiKey.mockRejectedValueOnce(new Error('keyring locked'))
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: 'sk-stays', model: 'gpt-4o' },
    })
    await migrateApiKeysFromSettings()
    const updated = JSON.parse(settingsMap['ai.cloudProviderConfigs']) as Partial<Record<string, { apiKey?: string }>>
    expect(updated.openai?.apiKey).toBe('sk-stays')
    expect(loggerWarn).toHaveBeenCalled()
  })

  it('migrates other providers even when one fails', async () => {
    saveAiApiKey.mockImplementation(({ providerId }) => {
      if (providerId === 'openai') return Promise.reject(new Error('keyring locked'))
      return Promise.resolve(null)
    })
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: 'sk-fails', model: 'gpt-4o' },
      anthropic: { apiKey: 'sk-works', model: 'claude' },
    })
    await migrateApiKeysFromSettings()
    const updated = JSON.parse(settingsMap['ai.cloudProviderConfigs']) as Partial<Record<string, { apiKey?: string }>>
    expect(updated.openai?.apiKey).toBe('sk-fails')
    expect(updated.anthropic?.apiKey).toBeUndefined()
  })

  it('drops an empty-string apiKey without calling the secret store', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: '', model: 'gpt-4o' },
    })
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
    const updated = JSON.parse(settingsMap['ai.cloudProviderConfigs']) as Partial<
      Record<string, Record<string, unknown>>
    >
    expect(updated.openai && 'apiKey' in updated.openai).toBe(false)
  })

  it('skips providers with no apiKey field altogether', async () => {
    const original = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.cloudProviderConfigs'] = original
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
    // Original JSON stays byte-equal because nothing mutated.
    expect(settingsMap['ai.cloudProviderConfigs']).toBe(original)
  })

  it('ignores non-string apiKey values', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({
      openai: { apiKey: 42, model: 'gpt-4o' },
    })
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
  })

  it('skips entries where the provider config is null', async () => {
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: null })
    await migrateApiKeysFromSettings()
    expect(saveAiApiKey).not.toHaveBeenCalled()
  })

  it('lifts a stranded legacy ai.openaiApiKey into the secret store, then drops the flat keys', async () => {
    rawStoreMap['ai.openaiApiKey'] = 'sk-legacy-123'
    rawStoreMap['ai.openaiBaseUrl'] = 'https://api.openai.com/v1'
    rawStoreMap['ai.openaiModel'] = 'gpt-4o-mini'
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' }) // not yet in the secret store

    await migrateApiKeysFromSettings()

    expect(saveAiApiKey).toHaveBeenCalledWith({ providerId: 'openai', apiKey: 'sk-legacy-123' })
    expect(rawStoreMap['ai.openaiApiKey']).toBeUndefined()
    expect(rawStoreMap['ai.openaiBaseUrl']).toBeUndefined()
    expect(rawStoreMap['ai.openaiModel']).toBeUndefined()
  })

  it('drops the flat keys without re-saving when the secret store already has the key', async () => {
    rawStoreMap['ai.openaiApiKey'] = 'sk-legacy-123'
    getAiApiKeyStatus.mockResolvedValue({ isSet: true, fingerprint: 'abc123' }) // already migrated

    await migrateApiKeysFromSettings()

    expect(saveAiApiKey).not.toHaveBeenCalled()
    expect(rawStoreMap['ai.openaiApiKey']).toBeUndefined()
  })

  it('keeps the legacy key if the secret-store save fails (never loses the only copy)', async () => {
    rawStoreMap['ai.openaiApiKey'] = 'sk-legacy-123'
    getAiApiKeyStatus.mockResolvedValue({ isSet: false, fingerprint: '' })
    saveAiApiKey.mockRejectedValueOnce(new Error('keychain locked'))

    await migrateApiKeysFromSettings()

    expect(rawStoreMap['ai.openaiApiKey']).toBe('sk-legacy-123')
  })
})

describe('pushConfigToBackend', () => {
  beforeEach(resetState)

  it('reads provider + base URL fresh and pushes the provider ID, never a key, to configureAi', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '32768'

    await pushConfigToBackend()

    // The backend reads the key from the OS secret store itself; this window never sees it.
    // OpenAI requires a key, so requiresApiKey is true.
    expect(configureAi).toHaveBeenCalledWith({
      provider: 'cloud',
      contextSize: 32768,
      cloudProviderId: 'openai',
      cloudBaseUrl: expect.stringContaining('openai.com'),
      cloudModel: 'gpt-4o',
      cloudRequiresApiKey: true,
    })
  })

  it('passes requiresApiKey=false for a keyless local endpoint (Ollama)', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'ollama'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ ollama: { model: 'llama3.2' } })
    settingsMap['ai.localContextSize'] = '32768'

    await pushConfigToBackend()

    expect(configureAi).toHaveBeenCalledWith({
      provider: 'cloud',
      contextSize: 32768,
      cloudProviderId: 'ollama',
      cloudBaseUrl: expect.stringContaining('localhost'),
      cloudModel: 'llama3.2',
      cloudRequiresApiKey: false,
    })
  })

  it('surfaces a persistent toast when the backend reports a secret-store read failure', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '16384'
    configureAi.mockResolvedValue({ secretStoreError: { type: 'access_denied', message: 'keyring locked' } })

    await pushConfigToBackend()

    expect(addToast).toHaveBeenCalledTimes(1)
    const [body, opts] = addToast.mock.calls[0]
    expect(typeof body).toBe('string')
    expect(opts).toMatchObject({ dismissal: 'persistent' })
    expect(loggerError).toHaveBeenCalled()
  })

  it('stays quiet when the backend read the key fine', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '16384'

    await pushConfigToBackend()

    expect(addToast).not.toHaveBeenCalled()
  })

  it('logs and swallows configureAi failures', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = '{}'
    settingsMap['ai.localContextSize'] = '65536'
    configureAi.mockRejectedValueOnce(new Error('IPC down'))

    await expect(pushConfigToBackend()).resolves.toBeUndefined()
    expect(loggerError).toHaveBeenCalled()
  })

  it('coerces ai.localContextSize via Number()', async () => {
    settingsMap['ai.provider'] = 'local'
    settingsMap['ai.cloudProvider'] = ''
    settingsMap['ai.cloudProviderConfigs'] = '{}'
    settingsMap['ai.localContextSize'] = '16384'

    await pushConfigToBackend()

    expect(configureAi).toHaveBeenCalledWith({
      provider: 'local',
      contextSize: 16384,
      cloudProviderId: '',
      cloudBaseUrl: expect.any(String),
      cloudModel: expect.any(String),
      cloudRequiresApiKey: false,
    })
  })

  it('never reaches for a command that reads the key back', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '16384'

    await pushConfigToBackend()

    expect(getAiApiKeyStatus).not.toHaveBeenCalled()
  })
})
