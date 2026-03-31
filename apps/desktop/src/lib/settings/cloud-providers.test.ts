import { describe, expect, it } from 'vitest'
import {
  cloudProviderPresets,
  getCloudProvider,
  getProviderConfigs,
  setProviderConfig,
  resolveCloudConfig,
} from './cloud-providers'

describe('cloudProviderPresets', () => {
  it('contains expected providers', () => {
    const ids = cloudProviderPresets.map((p) => p.id)
    expect(ids).toContain('openai')
    expect(ids).toContain('anthropic')
    expect(ids).toContain('ollama')
    expect(ids).toContain('custom')
  })

  it('has unique IDs', () => {
    const ids = cloudProviderPresets.map((p) => p.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('all presets have required fields', () => {
    for (const preset of cloudProviderPresets) {
      expect(preset.id).toBeTruthy()
      expect(preset.name).toBeTruthy()
      expect(typeof preset.requiresApiKey).toBe('boolean')
      expect(typeof preset.supportsModelList).toBe('boolean')
      expect(typeof preset.isLocal).toBe('boolean')
      expect(preset.description).toBeTruthy()
    }
  })
})

describe('getCloudProvider', () => {
  it('returns a preset by ID', () => {
    const openai = getCloudProvider('openai')
    expect(openai?.name).toBe('OpenAI')
    expect(openai?.baseUrl).toBe('https://api.openai.com/v1')
  })

  it('returns undefined for unknown ID', () => {
    expect(getCloudProvider('nonexistent')).toBeUndefined()
  })
})

describe('getProviderConfigs', () => {
  it('parses valid JSON', () => {
    const json = '{"openai":{"apiKey":"sk-123","model":"gpt-4"}}'
    const configs = getProviderConfigs(json)
    expect(configs['openai']?.apiKey).toBe('sk-123')
    expect(configs['openai']?.model).toBe('gpt-4')
  })

  it('returns empty object for invalid JSON', () => {
    expect(getProviderConfigs('not json')).toEqual({})
  })

  it('returns empty object for empty string', () => {
    expect(getProviderConfigs('')).toEqual({})
  })

  it('returns empty object for empty JSON object', () => {
    expect(getProviderConfigs('{}')).toEqual({})
  })
})

describe('setProviderConfig', () => {
  it('adds a new provider config', () => {
    const result = setProviderConfig('{}', 'openai', { apiKey: 'sk-123', model: 'gpt-4' })
    const parsed = getProviderConfigs(result)
    expect(parsed['openai']?.apiKey).toBe('sk-123')
    expect(parsed['openai']?.model).toBe('gpt-4')
  })

  it('updates an existing provider config', () => {
    const initial = '{"openai":{"apiKey":"sk-old","model":"gpt-3"}}'
    const result = setProviderConfig(initial, 'openai', { apiKey: 'sk-new', model: 'gpt-4' })
    const parsed = getProviderConfigs(result)
    expect(parsed['openai']?.apiKey).toBe('sk-new')
    expect(parsed['openai']?.model).toBe('gpt-4')
  })

  it('preserves other providers when updating one', () => {
    const initial = '{"openai":{"apiKey":"sk-1","model":"gpt-4"},"groq":{"apiKey":"gsk-2","model":"llama"}}'
    const result = setProviderConfig(initial, 'openai', { apiKey: 'sk-new', model: 'gpt-4.1' })
    const parsed = getProviderConfigs(result)
    expect(parsed['openai']?.apiKey).toBe('sk-new')
    expect(parsed['groq']?.apiKey).toBe('gsk-2')
  })
})

describe('resolveCloudConfig', () => {
  it('uses preset base URL for known providers', () => {
    const config = resolveCloudConfig('openai', '{}')
    expect(config.baseUrl).toBe('https://api.openai.com/v1')
    expect(config.model).toBe('gpt-4.1-mini')
    expect(config.apiKey).toBe('')
  })

  it('uses stored API key and model when available', () => {
    const configsJson = '{"openai":{"apiKey":"sk-test","model":"gpt-4"}}'
    const config = resolveCloudConfig('openai', configsJson)
    expect(config.apiKey).toBe('sk-test')
    expect(config.model).toBe('gpt-4')
    expect(config.baseUrl).toBe('https://api.openai.com/v1')
  })

  it('uses stored base URL for custom provider', () => {
    const configsJson = '{"custom":{"apiKey":"key","model":"model","baseUrl":"https://my-api.com/v1"}}'
    const config = resolveCloudConfig('custom', configsJson)
    expect(config.baseUrl).toBe('https://my-api.com/v1')
    expect(config.apiKey).toBe('key')
    expect(config.model).toBe('model')
  })

  it('uses stored base URL for azure-openai provider', () => {
    const configsJson =
      '{"azure-openai":{"apiKey":"key","model":"gpt-4","baseUrl":"https://myresource.openai.azure.com/openai/v1"}}'
    const config = resolveCloudConfig('azure-openai', configsJson)
    expect(config.baseUrl).toBe('https://myresource.openai.azure.com/openai/v1')
  })

  it('falls back to preset defaults when no config exists', () => {
    const config = resolveCloudConfig('groq', '{}')
    expect(config.baseUrl).toBe('https://api.groq.com/openai/v1')
    expect(config.model).toBe('llama-3.3-70b-versatile')
    expect(config.apiKey).toBe('')
  })

  it('returns empty strings for unknown provider', () => {
    const config = resolveCloudConfig('nonexistent', '{}')
    expect(config.baseUrl).toBe('')
    expect(config.model).toBe('')
    expect(config.apiKey).toBe('')
  })
})
