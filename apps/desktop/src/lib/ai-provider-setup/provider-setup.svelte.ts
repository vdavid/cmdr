/**
 * The one state machine behind "set up a cloud AI provider", shared by the onboarding
 * wizard's step 2 and Settings › AI › Provider.
 *
 * Owns: the per-provider load from the settings store + OS secret store, the 300 ms
 * API-key save debounce, the connection check and its 1 s debounce, the session model
 * cache, and the race guards that keep a slow answer for a provider the user has already
 * clicked away from out of the UI.
 *
 * Runes live here rather than in a plain `.ts` because the two surfaces read this state
 * reactively; that's also why the file carries the `.svelte.ts` extension.
 */

import { getCloudProvider, getProviderConfigs, setProviderConfig, getSetting, setSetting } from '$lib/settings'
import type { CloudProviderPreset } from '$lib/settings/cloud-providers'
import { checkAiConnection, getAiApiKeyStatus, saveAiApiKey } from '$lib/tauri-commands'
import { computeModelCacheKey, getCachedModels, setCachedModels } from '$lib/settings/ai-model-cache'
import { describeSecretError, type SecretErrorMessage } from '$lib/settings/sections/ai-secret-error'
import { isE2eRun } from '$lib/app-mode'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { providerHasEditableEndpoint } from './provider-setup-plan'

export type ConnectionStatus =
  | 'idle'
  | 'checking'
  | 'connected'
  | 'connected-no-models'
  | 'auth-error'
  | 'connection-error'
  | 'error'

export interface ProviderSetupOptions {
  /** Logger scope, so a warning says which surface it came from. */
  logScope: string
  /**
   * Fires on every change to the secret-store error, `null` included. Settings mirrors it
   * into a persistent toast so the user can act on it after closing the window; the wizard
   * leaves it at the inline message.
   */
  onSecretErrorChange?: (error: SecretErrorMessage | null) => void
  /**
   * Fires after a key lands in the secret store for the CURRENT provider. Settings uses it
   * to re-push the AI config; the wizard pushes once, from its own "Next" handler.
   */
  onKeyPersisted?: () => void
}

/**
 * Debounce API key saves so manual typing doesn't fire one secret-store write per
 * keystroke (especially on Linux, where every Secret Service call is a D-Bus round trip).
 * Paste arrives as a single `oninput`, so it sees no added latency. 300 ms feels
 * instantaneous after a pause and sits well under the connection-check debounce, so the
 * order is always: type → save → check.
 */
const API_KEY_SAVE_DEBOUNCE_MS = 300
const CONNECTION_CHECK_DEBOUNCE_MS = 1000

export class ProviderSetupController {
  #options: ProviderSetupOptions
  #log: ReturnType<typeof getAppLogger>

  /**
   * The provider every in-flight answer is compared against. A keychain read or a
   * connection check that resolves after the user picked another row is dropped, not
   * rendered against the new provider.
   */
  #providerId = $state('')

  /**
   * What the user has TYPED this session. A saved key never comes back from the backend
   * (`docs/security.md` § "AI API keys"), so this stays empty until they type, and the
   * field shows a "your key is saved" placeholder instead of dots standing in for a key.
   */
  #apiKey = $state('')
  /** Whether a key is stored for this provider: drives the placeholder and the check gate. */
  #keyIsSet = $state(false)
  /** The backend's opaque handle for the stored key. Changes whenever the key does, which
   *  is what makes the model cache miss after a key swap. */
  #keyFingerprint = $state('')
  #model = $state('')
  #baseUrl = $state('')

  #status = $state<ConnectionStatus>('idle')
  #error = $state<string | null>(null)
  #models = $state<string[]>([])
  #secretError = $state<SecretErrorMessage | null>(null)

  #apiKeySaveTimer: ReturnType<typeof setTimeout> | null = null
  #connectionCheckTimer: ReturnType<typeof setTimeout> | null = null
  /** Captured at schedule time, so switching provider mid-typing still flushes the
   *  trailing keystrokes against the keychain entry they were typed for. */
  #pendingApiKeySave: { providerId: string; value: string } | null = null

  constructor(options: ProviderSetupOptions) {
    this.#options = options
    this.#log = getAppLogger(options.logScope)
  }

  // ---- Reactive reads -------------------------------------------------------------

  get providerId(): string {
    return this.#providerId
  }
  get preset(): CloudProviderPreset | undefined {
    return getCloudProvider(this.#providerId)
  }
  get apiKey(): string {
    return this.#apiKey
  }
  get keyIsSet(): boolean {
    return this.#keyIsSet
  }
  get model(): string {
    return this.#model
  }
  get baseUrl(): string {
    return this.#baseUrl
  }
  get status(): ConnectionStatus {
    return this.#status
  }
  get error(): string | null {
    return this.#error
  }
  get models(): string[] {
    return this.#models
  }
  get secretError(): SecretErrorMessage | null {
    return this.#secretError
  }
  get isChecking(): boolean {
    return this.#status === 'checking'
  }
  /** True once the provider answered, with or without a model list. Ticks the key step. */
  get isConnected(): boolean {
    return this.#status === 'connected' || this.#status === 'connected-no-models'
  }

  /** The endpoint actually used: the user's for the two editable providers, the preset's otherwise. */
  get resolvedBaseUrl(): string {
    if (providerHasEditableEndpoint(this.#providerId)) return this.#baseUrl
    return this.preset?.baseUrl ?? ''
  }

  /** Whether there's enough config to be worth a round trip. */
  get hasCheckableConfig(): boolean {
    const requiresApiKey = this.preset?.requiresApiKey ?? false
    if (requiresApiKey && !this.#keyIsSet && this.#apiKey === '') return false
    return this.resolvedBaseUrl !== ''
  }

  // ---- Lifecycle ------------------------------------------------------------------

  /**
   * Points the controller at a provider: flushes anything typed for the previous one,
   * resets the connection state, reloads from the store, then reads the key status and
   * populates the model list.
   */
  setProvider(id: string): void {
    this.flushPendingApiKeySave()
    this.#providerId = id
    this.#resetConnectionState()
    this.#loadFromStore(id)
    void this.#loadKeyStatus(id).then(() => this.populateOnOpen())
  }

  /** Commits anything still in the save debounce. Idempotent; safe on teardown. */
  flushPendingApiKeySave(): void {
    if (!this.#apiKeySaveTimer || !this.#pendingApiKeySave) return
    clearTimeout(this.#apiKeySaveTimer)
    const pending = this.#pendingApiKeySave
    this.#apiKeySaveTimer = null
    this.#pendingApiKeySave = null
    void this.#persistApiKey(pending.providerId, pending.value)
  }

  /** Flushes pending typing and drops timers. Call from `onDestroy`. */
  destroy(): void {
    this.flushPendingApiKeySave()
    if (this.#connectionCheckTimer) {
      clearTimeout(this.#connectionCheckTimer)
      this.#connectionCheckTimer = null
    }
  }

  // ---- User actions ---------------------------------------------------------------

  handleApiKeyChange(value: string): void {
    this.#apiKey = value
    this.#setSecretError(null)
    this.#pendingApiKeySave = { providerId: this.#providerId, value }
    if (this.#apiKeySaveTimer) clearTimeout(this.#apiKeySaveTimer)
    this.#apiKeySaveTimer = setTimeout(() => {
      const pending = this.#pendingApiKeySave
      this.#apiKeySaveTimer = null
      this.#pendingApiKeySave = null
      if (pending) void this.#persistApiKey(pending.providerId, pending.value)
    }, API_KEY_SAVE_DEBOUNCE_MS)
  }

  saveModel(value: string): void {
    this.#model = value
    this.#writeProviderConfig({ model: value })
  }

  saveBaseUrl(value: string): void {
    this.#baseUrl = value
    this.#writeProviderConfig({ baseUrl: value })
    // Only the endpoint moves connectivity; a model change doesn't.
    this.#scheduleConnectionCheck()
  }

  /** Runs a check right now (the "Recheck" / "Test connection" button). */
  checkNow(): void {
    void this.#triggerConnectionCheck()
  }

  /**
   * Fills the model list when a surface opens: instantly from the session cache, otherwise
   * with one check. Auto-loading is the only request that fires without a user action, so
   * it's suppressed in automated E2E, which has no real provider to answer it. A cache hit
   * still serves everywhere, E2E included.
   */
  async populateOnOpen(): Promise<void> {
    if (!this.hasCheckableConfig) return
    const idAtStart = this.#providerId
    const fingerprint = await this.#cacheKey(idAtStart, this.resolvedBaseUrl, this.#keyFingerprint)
    if (idAtStart !== this.#providerId) return
    const cached = fingerprint === null ? undefined : getCachedModels(fingerprint)
    if (cached) {
      this.#models = cached
      this.#status = 'connected'
      return
    }
    if (isE2eRun()) return
    if (this.#connectionCheckTimer || this.#status === 'checking') return
    // Straight through, no debounce: nothing is being typed on open, and the user expects
    // "tell me whether my saved key still works" to answer right away.
    void this.#triggerConnectionCheck()
  }

  // ---- Internals ------------------------------------------------------------------

  #loadFromStore(id: string): void {
    const preset = getCloudProvider(id)
    const configs = getProviderConfigs(getSetting('ai.cloudProviderConfigs'))
    const providerConfig = configs[id]

    this.#model = providerConfig?.model ?? preset?.defaultModel ?? ''
    this.#baseUrl = providerHasEditableEndpoint(id)
      ? (providerConfig?.baseUrl ?? preset?.baseUrl ?? '')
      : (preset?.baseUrl ?? '')
    // Reset eagerly, so a stale key state from the previous provider can't flash while the
    // secret-store read is in flight.
    this.#apiKey = ''
    this.#keyIsSet = false
    this.#keyFingerprint = ''
    this.#setSecretError(null)
  }

  async #loadKeyStatus(id: string): Promise<void> {
    try {
      const status = await getAiApiKeyStatus(id)
      if (id !== this.#providerId) return
      this.#keyIsSet = status.isSet
      this.#keyFingerprint = status.fingerprint
    } catch (e) {
      if (id !== this.#providerId) return
      // "No key" is the right user-visible state when the read fails, so they can enter one
      // again; the failure itself is surfaced separately so the cause stays actionable.
      this.#keyIsSet = false
      this.#keyFingerprint = ''
      this.#setSecretError(describeSecretError(e, 'read'))
    }
  }

  async #persistApiKey(id: string, value: string): Promise<void> {
    try {
      await saveAiApiKey(id, value)
    } catch (e) {
      // Surface it and skip the check: an in-memory value would tell the user it worked.
      this.#setSecretError(describeSecretError(e, 'save'))
      this.#log.warn("Couldn't save the AI API key for provider {provider}: {error}", {
        provider: id,
        error: e,
      })
      return
    }
    if (id !== this.#providerId) return
    // Re-read the status so the fingerprint matches the key just stored: the check below
    // caches its model list under that fingerprint, and a stale one would serve the
    // previous key's models.
    await this.#loadKeyStatus(id)
    if (id !== this.#providerId) return
    this.#options.onKeyPersisted?.()
    this.#scheduleConnectionCheck()
  }

  #writeProviderConfig(patch: { model?: string; baseUrl?: string }): void {
    const configsJson = getSetting('ai.cloudProviderConfigs')
    const configs = getProviderConfigs(configsJson)
    const existing = configs[this.#providerId] ?? { model: this.#model }
    if (patch.model !== undefined) existing.model = patch.model
    if (patch.baseUrl !== undefined) existing.baseUrl = patch.baseUrl
    setSetting('ai.cloudProviderConfigs', setProviderConfig(configsJson, this.#providerId, existing))
  }

  #scheduleConnectionCheck(delayMs: number = CONNECTION_CHECK_DEBOUNCE_MS): void {
    if (this.#connectionCheckTimer) clearTimeout(this.#connectionCheckTimer)
    this.#connectionCheckTimer = setTimeout(() => {
      this.#connectionCheckTimer = null
      void this.#triggerConnectionCheck()
    }, delayMs)
  }

  async #triggerConnectionCheck(): Promise<void> {
    if (this.#connectionCheckTimer) {
      clearTimeout(this.#connectionCheckTimer)
      this.#connectionCheckTimer = null
    }
    if (!this.hasCheckableConfig) return

    // Captured so the result is cached under the config it was fetched for, even if the
    // user keeps typing while the request is in flight.
    const baseUrlAtStart = this.resolvedBaseUrl
    const fingerprintAtStart = this.#keyFingerprint
    const idAtStart = this.#providerId

    this.#status = 'checking'
    this.#error = null
    // The prior list stays put during a refetch: a suggestion list that blanks mid-check is
    // a regression we forbid.

    try {
      // The backend reads the stored key for this provider, so anything just typed has to
      // be committed first. That's why the check is scheduled from `#persistApiKey`.
      const result = await checkAiConnection(baseUrlAtStart, idAtStart)
      if (idAtStart !== this.#providerId) return
      if (result.authError) {
        this.#status = 'auth-error'
        this.#error = result.error
      } else if (!result.connected) {
        this.#status = 'connection-error'
        this.#error = result.error
      } else if (result.error) {
        this.#status = 'error'
        this.#error = result.error
      } else if (result.models.length > 0) {
        this.#status = 'connected'
        this.#models = result.models
        void this.#cacheModels(idAtStart, baseUrlAtStart, fingerprintAtStart, result.models)
      } else {
        this.#status = 'connected-no-models'
      }
    } catch (e) {
      if (idAtStart !== this.#providerId) return
      this.#status = 'error'
      this.#error = e instanceof Error ? e.message : tString('onboarding.cloudSetup.status.genericError')
    }
  }

  async #cacheModels(providerId: string, baseUrl: string, keyFingerprint: string, models: string[]): Promise<void> {
    const key = await this.#cacheKey(providerId, baseUrl, keyFingerprint)
    if (key !== null) setCachedModels(key, models)
  }

  /**
   * The model cache's key, or `null` when the digest can't be computed. The cache is an
   * optimization on top of a network call, so a runtime without Web Crypto (happy-dom in
   * the unit tests, a webview in a non-secure context) must degrade to "always refetch",
   * never to "never check".
   */
  async #cacheKey(providerId: string, baseUrl: string, keyFingerprint: string): Promise<string | null> {
    try {
      return await computeModelCacheKey(providerId, baseUrl, keyFingerprint)
    } catch (e) {
      this.#log.debug("Couldn't compute the model-cache key, so this check won't be cached: {error}", { error: e })
      return null
    }
  }

  #resetConnectionState(): void {
    this.#status = 'idle'
    this.#error = null
    this.#models = []
    if (this.#connectionCheckTimer) {
      clearTimeout(this.#connectionCheckTimer)
      this.#connectionCheckTimer = null
    }
  }

  #setSecretError(error: SecretErrorMessage | null): void {
    if (error === null && this.#secretError === null) return
    this.#secretError = error
    this.#options.onSecretErrorChange?.(error)
  }
}
