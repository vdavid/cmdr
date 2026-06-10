/**
 * AI provider configuration plumbing shared by the settings UI, the onboarding wizard,
 * and the live-apply listener.
 *
 * Two responsibilities:
 *
 * 1. **`pushConfigToBackend()`** — read-fresh push of the current AI provider config to
 *    Rust. Re-reads `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` /
 *    `ai.localContextSize` from `getSetting(...)` on every call, fetches the matching
 *    API key from the OS secret store, calls `configureAi(...)`. Surfaces secret-store
 *    failures via a deduped persistent toast so a silently-broken keyring isn't invisible.
 *    Callers MUST NOT pass cached values: the helper has read-fresh semantics so that the
 *    "user flips provider mid-flight" race resolves to whichever provider is current at
 *    the actual IPC moment (see `settings-applier.ts` for the listener wiring).
 *
 * 2. **`migrateApiKeysFromSettings()`** — one-time migration that lifts pre-launch
 *    `apiKey` strings out of `ai.cloudProviderConfigs` (in `settings.json`) into the OS
 *    secret store. Idempotent; once the JSON blob no longer carries an `apiKey` field
 *    for any provider, this is a near-zero-cost no-op.
 *
 * Lives in `lib/settings/` (not `lib/settings/sections/`) because the function isn't
 * UI-component-coupled — it's a service the wizard, the applier listener, and the
 * settings UI all reach for. `sections/` is reserved for UI subcomponents.
 */

import { getSetting, setSetting, resolveCloudConfig, getRawStoreValue, deleteRawStoreKeys } from '$lib/settings'
import { configureAi, getAiApiKey, saveAiApiKey, hasAiApiKey } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { addToast } from '$lib/ui/toast'
import { describeSecretError } from './sections/ai-secret-error'

const logger = getAppLogger('ai-settings')

/** Stable id so the same toast replaces in place across consecutive failed startup attempts. */
const secretErrorToastId = 'ai-secret-store-error'

/**
 * One-time migration: move `apiKey` fields out of `ai.cloudProviderConfigs` (settings.json) into
 * the OS secret store, then (via `migrateLegacyOpenAiKeys`) lift + drop the even older flat
 * `ai.openaiApiKey` / `ai.openaiBaseUrl` / `ai.openaiModel` keys. Runs idempotently at startup;
 * once the JSON blob and the flat keys are gone, this is a near-zero-cost no-op.
 *
 * Per-provider semantics: if saving to the secret store fails for one provider, that provider's
 * `apiKey` stays in settings.json so the user can retry later. Other providers still migrate.
 *
 * TODO: Remove this migration after 2026-09-01. By then, testers will have had time to migrate
 * their pre-launch keys, and continuing to ship the code only adds attack surface (the migration
 * step temporarily handles plaintext keys).
 */
export async function migrateApiKeysFromSettings(): Promise<void> {
  // Two independent steps. The flat-key cleanup runs even when the cloud-config blob is
  // missing or malformed (a returning user might have ONLY the pre-refactor flat keys).
  await migrateCloudProviderConfigKeys()
  await migrateLegacyOpenAiKeys()
}

/** Lifts `apiKey` fields out of the `ai.cloudProviderConfigs` JSON blob into the secret store. */
async function migrateCloudProviderConfigKeys(): Promise<void> {
  const raw = getSetting('ai.cloudProviderConfigs')
  let parsed: Record<string, Record<string, unknown> | undefined>
  try {
    parsed = JSON.parse(raw) as Record<string, Record<string, unknown> | undefined>
  } catch {
    return
  }

  let mutated = false
  for (const [providerId, config] of Object.entries(parsed)) {
    if (!config) continue
    const legacyApiKey = config.apiKey
    if (typeof legacyApiKey !== 'string' || legacyApiKey.length === 0) {
      if ('apiKey' in config) {
        // Empty string is harmless but pollutes the JSON. Drop it as part of the migration.
        delete config.apiKey
        mutated = true
      }
      continue
    }
    try {
      await saveAiApiKey(providerId, legacyApiKey)
      delete config.apiKey
      mutated = true
      logger.info('Migrated AI API key for provider {provider} to secret store', { provider: providerId })
    } catch (e) {
      logger.warn(
        "Couldn't migrate AI API key for provider {provider}. Leaving the legacy entry in settings.json: {error}",
        { provider: providerId, error: e },
      )
    }
  }

  if (mutated) {
    setSetting('ai.cloudProviderConfigs', JSON.stringify(parsed))
  }
}

/**
 * The pre-`ai.cloudProvider` schema stored OpenAI config as three flat keys:
 * `ai.openaiApiKey` (a PLAINTEXT key), `ai.openaiBaseUrl`, `ai.openaiModel`. The current code
 * reads none of them (the key lives in the OS secret store; model/baseUrl in
 * `ai.cloudProviderConfigs`), and the registry-driven save loop only manages registered ids, so
 * they'd otherwise linger in `settings.json` forever — a plaintext key sitting in Time Machine
 * and cloud backups, exactly what the secret-store move was meant to prevent.
 *
 * This lifts the flat key into the secret store under `openai` IF that's its only copy (so a
 * tester who set it pre-refactor and never re-entered it doesn't lose it AND gets working AI
 * back), then drops all three dead keys. Idempotent: once the keys are gone it's a no-op.
 */
const LEGACY_OPENAI_KEYS = ['ai.openaiApiKey', 'ai.openaiBaseUrl', 'ai.openaiModel'] as const

async function migrateLegacyOpenAiKeys(): Promise<void> {
  const legacyKey = (await getRawStoreValue<string>('ai.openaiApiKey')) ?? ''
  if (legacyKey.length > 0 && !(await hasAiApiKey('openai'))) {
    try {
      await saveAiApiKey('openai', legacyKey)
      logger.info('Lifted a legacy flat OpenAI API key into the secret store')
    } catch (e) {
      // Don't delete the only copy if we couldn't preserve it; retry next launch.
      logger.warn("Couldn't move the legacy OpenAI key to the secret store; leaving settings.json untouched: {error}", {
        error: e,
      })
      return
    }
  }
  await deleteRawStoreKeys(LEGACY_OPENAI_KEYS)
}

/**
 * Push current AI config (provider, context size, cloud credentials) to the Rust backend. The API
 * key is fetched from the OS secret store; the rest comes from `settings.json`. Surfaces secret
 * store failures as a persistent toast (deduped) so a silently-broken keyring isn't invisible to
 * the user.
 *
 * **Read-fresh contract (load-bearing).** Every relevant setting is re-read from `getSetting(...)`
 * at call time. Callers MUST NOT pass cached values. The applier listener may fire while the user
 * is still toggling things in the wizard; reading fresh means whichever provider is current at the
 * actual IPC moment wins, which matches user expectations.
 */
export async function pushConfigToBackend(): Promise<void> {
  try {
    const providerId = getSetting('ai.cloudProvider')
    const resolved = resolveCloudConfig(providerId, getSetting('ai.cloudProviderConfigs'))

    let apiKey = ''
    try {
      apiKey = await getAiApiKey(providerId)
    } catch (e) {
      logger.error("Couldn't read AI API key from secret store: {error}", { error: e })
      const msg = describeSecretError(e, 'read')
      const body = msg.body ? `\n${msg.body}` : ''
      addToast(`${msg.title}${body}`, {
        level: msg.level,
        dismissal: 'persistent',
        id: secretErrorToastId,
      })
    }

    await configureAi(
      getSetting('ai.provider'),
      Number(getSetting('ai.localContextSize')),
      apiKey,
      resolved.baseUrl,
      resolved.model,
    )
  } catch (e) {
    logger.error("Couldn't push AI config to backend: {error}", { error: e })
  }
}
