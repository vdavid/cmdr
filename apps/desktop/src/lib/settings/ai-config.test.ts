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

const saveAiApiKey = vi.fn<(id: string, key: string) => Promise<null>>(() => Promise.resolve(null))
const getAiApiKey = vi.fn<(id: string) => Promise<string>>(() => Promise.resolve(''))
const hasAiApiKey = vi.fn<(id: string) => Promise<boolean>>(() => Promise.resolve(false))
const configureAi = vi.fn<
  (provider: string, contextSize: number, apiKey: string, baseUrl: string, model: string) => Promise<null>
>(() => Promise.resolve(null))

vi.mock('$lib/tauri-commands', () => ({
  saveAiApiKey: (id: string, key: string) => saveAiApiKey(id, key),
  getAiApiKey: (id: string) => getAiApiKey(id),
  hasAiApiKey: (id: string) => hasAiApiKey(id),
  configureAi: (provider: string, contextSize: number, apiKey: string, baseUrl: string, model: string) =>
    configureAi(provider, contextSize, apiKey, baseUrl, model),
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
        // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture
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
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture reset
    delete settingsMap[k]
  }
  for (const k of Object.keys(rawStoreMap)) {
    // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- test fixture reset
    delete rawStoreMap[k]
  }
  saveAiApiKey.mockReset()
  saveAiApiKey.mockResolvedValue(null)
  getAiApiKey.mockReset()
  getAiApiKey.mockResolvedValue('')
  hasAiApiKey.mockReset()
  hasAiApiKey.mockResolvedValue(false)
  configureAi.mockReset()
  configureAi.mockResolvedValue(null)
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
    expect(saveAiApiKey).toHaveBeenCalledWith('openai', 'sk-legacy')
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
    expect(saveAiApiKey).toHaveBeenCalledWith('openai', 'sk-one')
    expect(saveAiApiKey).toHaveBeenCalledWith('anthropic', 'sk-ant')
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
    saveAiApiKey.mockImplementation((id: string) => {
      if (id === 'openai') return Promise.reject(new Error('keyring locked'))
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
    hasAiApiKey.mockResolvedValue(false) // not yet in the secret store

    await migrateApiKeysFromSettings()

    expect(saveAiApiKey).toHaveBeenCalledWith('openai', 'sk-legacy-123')
    expect(rawStoreMap['ai.openaiApiKey']).toBeUndefined()
    expect(rawStoreMap['ai.openaiBaseUrl']).toBeUndefined()
    expect(rawStoreMap['ai.openaiModel']).toBeUndefined()
  })

  it('drops the flat keys without re-saving when the secret store already has the key', async () => {
    rawStoreMap['ai.openaiApiKey'] = 'sk-legacy-123'
    hasAiApiKey.mockResolvedValue(true) // already migrated

    await migrateApiKeysFromSettings()

    expect(saveAiApiKey).not.toHaveBeenCalled()
    expect(rawStoreMap['ai.openaiApiKey']).toBeUndefined()
  })

  it('keeps the legacy key if the secret-store save fails (never loses the only copy)', async () => {
    rawStoreMap['ai.openaiApiKey'] = 'sk-legacy-123'
    hasAiApiKey.mockResolvedValue(false)
    saveAiApiKey.mockRejectedValueOnce(new Error('keychain locked'))

    await migrateApiKeysFromSettings()

    expect(rawStoreMap['ai.openaiApiKey']).toBe('sk-legacy-123')
  })
})

describe('pushConfigToBackend', () => {
  beforeEach(resetState)

  it('reads provider + key + base URL fresh and pushes to configureAi', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '8192'
    getAiApiKey.mockResolvedValue('sk-fresh')

    await pushConfigToBackend()

    expect(getAiApiKey).toHaveBeenCalledWith('openai')
    expect(configureAi).toHaveBeenCalledWith('cloud', 8192, 'sk-fresh', expect.stringContaining('openai.com'), 'gpt-4o')
  })

  it('surfaces a persistent toast and keeps pushing when the secret store read fails', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = JSON.stringify({ openai: { model: 'gpt-4o' } })
    settingsMap['ai.localContextSize'] = '4096'
    getAiApiKey.mockRejectedValue(new Error('keyring locked'))

    await pushConfigToBackend()

    expect(addToast).toHaveBeenCalledTimes(1)
    const [body, opts] = addToast.mock.calls[0]
    expect(typeof body).toBe('string')
    expect(opts).toMatchObject({ dismissal: 'persistent' })
    // Still pushed with an empty key so the rest of the config reaches the backend.
    expect(configureAi).toHaveBeenCalledWith('cloud', 4096, '', expect.any(String), 'gpt-4o')
    expect(loggerError).toHaveBeenCalled()
  })

  it('logs and swallows configureAi failures', async () => {
    settingsMap['ai.provider'] = 'cloud'
    settingsMap['ai.cloudProvider'] = 'openai'
    settingsMap['ai.cloudProviderConfigs'] = '{}'
    settingsMap['ai.localContextSize'] = '2048'
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

    expect(configureAi).toHaveBeenCalledWith('local', 16384, '', expect.any(String), expect.any(String))
  })
})
