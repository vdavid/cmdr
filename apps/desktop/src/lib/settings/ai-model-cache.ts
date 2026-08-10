/**
 * Session-scoped cache of the model list returned by a cloud AI connection check.
 *
 * Why: `AiCloudSection` loses its `availableModels` on every close (it's component-local `$state`),
 * so reopening Settings refetches the list. This process-lifetime `Map` lets a warm reopen serve the
 * list instantly while a config change (new key/URL/provider) still misses and refetches.
 *
 * The key is a SHA-256 hex digest of `providerId \0 baseUrl \0 keyFingerprint` (Web Crypto, available
 * in the webview). The NUL separators stop boundary collisions (`('ab','c')` vs `('a','bc')`). The
 * key fingerprint comes from `getAiApiKeyStatus`: the raw API key is never readable from a window,
 * and the fingerprint still changes on every key change, so a revoked-vs-new key can't serve the
 * old, wrong list. We store only the opaque digest, never its input.
 *
 * Not reactive: a plain module `Map`, not `$state`. Consumers copy the list into their own `$state`
 * on a hit. The cache lives for the process; there's no eviction (a handful of entries at most).
 */

const FIELD_SEPARATOR = '\0'

const modelListByFingerprint = new Map<string, string[]>()

function toHex(buffer: ArrayBuffer): string {
  return Array.from(new Uint8Array(buffer))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
}

/**
 * SHA-256 hex digest of the provider config tuple. Async (Web Crypto's `digest` is). The input is
 * NUL-separated so field boundaries can't collide; it's never stored or logged.
 *
 * `keyFingerprint` is the backend's opaque handle for the stored key (`getAiApiKeyStatus`), not the
 * key itself.
 */
export async function computeModelCacheKey(
  providerId: string,
  baseUrl: string,
  keyFingerprint: string,
): Promise<string> {
  const input = [providerId, baseUrl, keyFingerprint].join(FIELD_SEPARATOR)
  const bytes = new TextEncoder().encode(input)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return toHex(digest)
}

/** The cached model list for this fingerprint, or `undefined` on a miss. */
export function getCachedModels(fingerprint: string): string[] | undefined {
  return modelListByFingerprint.get(fingerprint)
}

/** Stores (or replaces) the model list for this fingerprint. */
export function setCachedModels(fingerprint: string, models: string[]): void {
  modelListByFingerprint.set(fingerprint, models)
}

/** Test-only: drops every cached entry so each test sees a cold cache. */
export function clearModelCache(): void {
  modelListByFingerprint.clear()
}
