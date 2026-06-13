/**
 * Unit tests for the session-scoped AI model cache.
 *
 * The cache spares the AI settings panel a refetch on every close+reopen, keyed by a SHA-256
 * fingerprint of the provider config. Two invariants the cache must defend:
 *   - The fingerprint is collision-free across DIFFERENT keys, even equal-length ones (a
 *     revoked-vs-new key of the same length must NOT serve the old, wrong list).
 *   - The same tuple maps to the same digest, so a warm reopen is a hit.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { computeModelCacheKey, getCachedModels, setCachedModels, clearModelCache } from './ai-model-cache'

describe('computeModelCacheKey', () => {
  it('returns a stable hex digest for the same tuple', async () => {
    const a = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-abc')
    const b = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-abc')
    expect(a).toBe(b)
    expect(a).toMatch(/^[0-9a-f]{64}$/)
  })

  it('produces different digests for different API keys of equal length', async () => {
    // Equal length so a length-based key would collide and serve a stale/wrong list.
    const a = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-aaaaaa')
    const b = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-bbbbbb')
    expect(a).not.toBe(b)
  })

  it('produces different digests when only the provider differs', async () => {
    const a = await computeModelCacheKey('openai', 'https://api.example.com/v1', 'sk-abc')
    const b = await computeModelCacheKey('anthropic', 'https://api.example.com/v1', 'sk-abc')
    expect(a).not.toBe(b)
  })

  it('produces different digests when only the base URL differs', async () => {
    const a = await computeModelCacheKey('custom', 'https://api.one.com/v1', 'sk-abc')
    const b = await computeModelCacheKey('custom', 'https://api.two.com/v1', 'sk-abc')
    expect(a).not.toBe(b)
  })

  it('does not collide when the field boundary shifts (NUL-separated, not concatenated)', async () => {
    // Without a separator, ('ab', 'c') and ('a', 'bc') would concatenate to the same string.
    const a = await computeModelCacheKey('ab', 'c', '')
    const b = await computeModelCacheKey('a', 'bc', '')
    expect(a).not.toBe(b)
  })
})

describe('model cache get/set', () => {
  beforeEach(() => {
    clearModelCache()
  })

  it('returns undefined on a cold miss', async () => {
    const key = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-cold')
    expect(getCachedModels(key)).toBeUndefined()
  })

  it('returns the stored list on a warm hit', async () => {
    const key = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-warm')
    setCachedModels(key, ['gpt-4o', 'gpt-4o-mini'])
    expect(getCachedModels(key)).toEqual(['gpt-4o', 'gpt-4o-mini'])
  })

  it('misses after the key changes (a different config fingerprint)', async () => {
    const oldKey = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-old')
    setCachedModels(oldKey, ['gpt-4o'])
    const newKey = await computeModelCacheKey('openai', 'https://api.openai.com/v1', 'sk-new')
    expect(getCachedModels(newKey)).toBeUndefined()
    // The old fingerprint still resolves, so a hop back to the old config stays warm.
    expect(getCachedModels(oldKey)).toEqual(['gpt-4o'])
  })

  it('overwrites the list for the same fingerprint on a re-set', async () => {
    const key = await computeModelCacheKey('groq', 'https://api.groq.com/openai/v1', 'gsk-x')
    setCachedModels(key, ['a'])
    setCachedModels(key, ['a', 'b'])
    expect(getCachedModels(key)).toEqual(['a', 'b'])
  })
})
