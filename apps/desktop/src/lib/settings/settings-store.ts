/**
 * Settings persistence layer - stores and loads settings from disk.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SettingId, SettingsValues } from './types'
import { SettingValidationError } from './types'
import { getDefaultValue, getSettingDefinition, settingsRegistry, validateSettingValue } from './settings-registry'
import { resolveStorePath } from './store-path'
import { SCHEMA_VERSION, migrateSettings } from './settings-migrations'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import type { RestrictedWindowPersistableSetting, SettingValue } from '$lib/ipc/bindings'
// Import from the specific submodule, not the `$lib/tauri-commands` barrel: the
// barrel re-exports the entire IPC surface (mtp, search, indexing, licensing, …),
// so pulling it in for three functions drags that whole graph into every
// settings-store consumer. The submodule keeps the dependency (and its transform
// graph) tight.
import {
  getRestrictedWindowSettings,
  persistRestrictedWindowSetting,
  recordSettingsDefaults,
} from '$lib/tauri-commands/settings'
import {
  getSettingLock,
  initManagedPolicy,
  settingLockIn,
  type ManagedPolicyChange,
} from '$lib/managed-policy/managed-policy.svelte'
import { lockAllowsWrite, lockedValue } from '$lib/managed-policy/overlay'

const log = getAppLogger('settings')

// Event name for cross-window setting changes
const SETTING_CHANGED_EVENT = 'settings:changed'

interface SettingChangedPayload {
  id: string
  value: unknown
  // Whether the change is an explicit set (true) or a reset back to the registry
  // default (false). Lets a receiving window keep its sparse explicit-set ledger
  // in sync, so its own later save neither drops a key another window just set
  // nor re-persists a key another window just reset.
  explicit: boolean
}

// ============================================================================
// Store Configuration
// ============================================================================

let storeInstance: Store | null = null
let saveTimeout: ReturnType<typeof setTimeout> | null = null
const SAVE_DEBOUNCE_MS = 500

// In-memory cache of settings for synchronous access. A `Map` (not a plain
// object) so unsetting a key on reset is a `.delete(id)` call, not the
// `delete obj[computed]` form the lint bans (`no-dynamic-delete`).
const settingsCache = new Map<string, unknown>()

// Ids an actor explicitly set: the Settings UI, the MCP `set_setting` tool, a
// migration, or a key found already persisted at load. This is the sparse-
// persistence ledger — `saveToStore` writes exactly these keys and nothing else.
// "Explicit" is a STRUCTURAL fact about which mutator ran, never a value
// comparison against the default: a deliberate choice that equals the default
// still belongs here, so it survives a future change to that default.
const explicitlySet = new Set<SettingId>()

let initialized = false
let crossWindowUnlisten: UnlistenFn | null = null

// True when this window runs without `tauri-plugin-store` capability (the
// viewer; see `src-tauri/capabilities/CLAUDE.md` § viewer). The store is never
// loaded: reads come from the `get_restricted_window_settings` snapshot plus
// cross-window `settings:changed` events, and writes persist through the
// `persist_restricted_window_setting` command (forwarded to the main window).
let restrictedWindowMode = false

/** The settings a restricted window may persist, mapped to the typed command
 *  enum. Must mirror `RestrictedWindowPersistableSetting` in
 *  `src-tauri/src/commands/settings.rs` — the backend enum is the enforced
 *  allowlist; this map only decides which `setSetting` calls are forwarded.
 *
 *  ❌ Shorter than the read snapshot below ON PURPOSE. A setting earns a place
 *  here only when a restricted window has a control that WRITES it (`W`, the
 *  binary-warning banner's button). One it merely reads — `viewer.showTextCursor`,
 *  every `appearance.*` — stays out, so the app's highest-risk webview never gets
 *  a write it has no use for. */
const RESTRICTED_PERSISTABLE_SETTINGS: Partial<Record<SettingId, RestrictedWindowPersistableSetting>> = {
  'viewer.wordWrap': 'viewerWordWrap',
  'fileViewer.suppressBinaryWarning': 'fileViewerSuppressBinaryWarning',
}

// ============================================================================
// Initialization
// ============================================================================

async function getStore(): Promise<Store> {
  if (!storeInstance) {
    // Resolve the store path so isolated instances (dev, per-worktree dev, E2E)
    // don't read the real production `settings.json`. See `store-path.ts`.
    const storePath = await resolveStorePath('settings.json', (e) => {
      log.warn('Could not resolve isolated settings path, using default: {error}', { error: String(e) })
    })
    log.debug('Creating new store instance for {storeName}', { storeName: storePath })
    // Load WITHOUT seeding registry defaults. The store file must hold only the
    // keys an actor explicitly set, so no save can ever flush a registry default
    // into `settings.json` as if it were a user choice (the bug that pinned
    // `developer.mcpEnabled: false` and silently killed the dev MCP server).
    // Defaults resolve at read time via `getDefaultValue`, and a changed default
    // in a future release then reaches every user who never set that key.
    storeInstance = await load(storePath, { defaults: {}, autoSave: false })
    log.debug('Store instance created successfully')
  }
  return storeInstance
}

/**
 * Initialize the settings store. Must be called before using getSetting/setSetting.
 *
 * Pass `restrictedWindow: true` from windows whose capability file deliberately
 * has no `tauri-plugin-store` permission (the viewer). That path never touches
 * the store plugin: it seeds the cache from the backend's typed
 * `get_restricted_window_settings` snapshot and relies on cross-window
 * `settings:changed` events for live updates. Failures there degrade to
 * registry defaults with a warning — they're an expected capability boundary,
 * not an error (an error-level log would trigger an auto error report on
 * every viewer open, which is exactly the regression this mode fixes).
 */
export async function initializeSettings(options?: { restrictedWindow?: boolean }): Promise<void> {
  log.debug('initializeSettings() called, initialized={initialized}', { initialized })

  if (initialized) {
    log.debug('Settings already initialized, returning early')
    return
  }

  if (options?.restrictedWindow) {
    await initializeSettingsRestricted()
    return
  }

  log.debug('Starting settings initialization')

  try {
    const store = await getStore()
    log.debug('Got store instance')

    // Check schema version and migrate if needed
    const version = await store.get<number>('_schemaVersion')
    log.debug('Current schema version: {version}, expected: {expected}', { version, expected: SCHEMA_VERSION })

    if (version !== SCHEMA_VERSION) {
      log.info('Schema version mismatch, migrating from {from} to {to}', {
        from: version ?? 0,
        to: SCHEMA_VERSION,
      })
      await migrateSettings(store, version ?? 0)
    }

    // Load all settings into cache
    log.debug('Loading {count} {settingsNoun} from store into cache', {
      count: settingsRegistry.length,
      settingsNoun: pluralize(settingsRegistry.length, 'setting'),
    })
    let loadedCount = 0
    let defaultCount = 0

    for (const def of settingsRegistry) {
      const stored = await store.get<unknown>(def.id)
      if (stored !== null && stored !== undefined) {
        try {
          validateSettingValue(def.id, stored)
          settingsCache.set(def.id, stored)
          // Present on disk = an actor set it (we can't tell past-explicit from a
          // pre-fix leak, and dropping a deliberate choice is worse than keeping a
          // stale one — so every present, valid key is treated as explicit).
          explicitlySet.add(def.id)
          loadedCount++
        } catch {
          // Invalid stored value: drop it. Reads resolve to the default, and the
          // next save prunes it since it never enters the explicit-set ledger.
          log.warn('Invalid stored value for {id}, using default', { id: def.id })
          defaultCount++
        }
      } else {
        defaultCount++
      }
    }

    log.debug('Settings loaded: {loaded} from store, {defaults} using defaults', {
      loaded: loadedCount,
      defaults: defaultCount,
    })

    // Listen for cross-window setting changes
    await setupCrossWindowListener()

    // The organization's policy, BEFORE `initialized`: no reader ever sees a value the policy
    // overrides. Never throws (a failed fetch shows nothing as managed; the backend still refuses).
    await initManagedPolicy(applyPolicyChange)

    // Push the registry's default map to the backend so error-report manifests can
    // resolve `null`-shaped settings against the live registry instead of duplicating
    // defaults in Rust. Best-effort: a failure here only affects manifest resolution,
    // which has hardcoded fallbacks. Don't block init on it.
    void pushSettingsDefaultsToBackend()

    initialized = true
    log.debug('Settings initialization complete')
  } catch (error) {
    log.error('Failed to initialize settings: {error}', { error })
    throw error
  }
}

/**
 * Restricted-window initialization: snapshot from the backend instead of the
 * store plugin. Non-throwing — on failure the window simply runs on registry
 * defaults, which is the designed degradation for the viewer.
 */
async function initializeSettingsRestricted(): Promise<void> {
  restrictedWindowMode = true
  try {
    const snapshot = await getRestrictedWindowSettings()
    // Mechanical mapping: each snapshot field name spells out its setting id.
    const mapped: Partial<Record<SettingId, unknown>> = {
      'viewer.wordWrap': snapshot.viewerWordWrap,
      'viewer.showTextCursor': snapshot.viewerShowTextCursor,
      'fileViewer.suppressBinaryWarning': snapshot.fileViewerSuppressBinaryWarning,
      'appearance.textSize': snapshot.appearanceTextSize,
      'appearance.appColor': snapshot.appearanceAppColor,
      // The operation queue window is restricted but renders `<Size>`; without this it
      // would show a different number than the copy dialog for the same bytes.
      'appearance.fileSizeFormat': snapshot.appearanceFileSizeFormat,
      // Every window resolves its own UI language (`initWindowLanguageSync`), reading
      // this setting. Without it a restricted window sees the `'system'` default, so a
      // user who pinned Hungarian on an English Mac would get an English viewer.
      'appearance.language': snapshot.appearanceLanguage,
    }
    for (const [id, value] of Object.entries(mapped)) {
      if (value == null) continue // not persisted: registry default applies
      try {
        validateSettingValue(id, value)
        settingsCache.set(id, value)
      } catch {
        log.warn('Invalid snapshot value for {id}, using default', { id })
      }
    }

    // Live updates (text size, app color, ...) still arrive from the main and
    // settings windows through the regular cross-window event.
    await setupCrossWindowListener()

    initialized = true
    log.debug('Settings initialized from restricted-window snapshot')
  } catch (error) {
    // warn, not error: the window stays usable on registry defaults, and an
    // error-level log would fire an auto error report on every viewer open.
    log.warn('Restricted-window settings snapshot failed, using defaults: {error}', { error })
  }
}

/**
 * Send the registry's default values to the backend's `record_settings_defaults`
 * command. The backend uses this map in `ResolvedSettings::from_settings` to keep
 * manifest defaults in sync with the registry. Silently swallows errors; the Rust
 * side has hardcoded fallbacks for every field it reads.
 */
async function pushSettingsDefaultsToBackend(): Promise<void> {
  try {
    const defaults: Record<string, SettingValue> = {}
    for (const def of settingsRegistry) {
      const value = def.default
      // SettingValue is untagged on the wire: boolean | number | string.
      // Values of other types (arrays, objects) are silently skipped; the Rust
      // side has hardcoded fallbacks and the lookup_* helpers only support these three.
      if (typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') {
        defaults[def.id] = value
      }
    }
    await recordSettingsDefaults(defaults)
  } catch (err) {
    log.warn('Failed to push settings defaults to backend: {err}', { err })
  }
}

/**
 * Set up listener for setting changes from other windows.
 */
async function setupCrossWindowListener(): Promise<void> {
  if (crossWindowUnlisten) {
    return // Already listening
  }

  log.debug('Setting up cross-window settings listener')

  crossWindowUnlisten = await listen<SettingChangedPayload>(SETTING_CHANGED_EVENT, (event) => {
    const { id, value, explicit } = event.payload
    log.debug('Received cross-window setting change: {id}', { id })

    // Update our cache and mirror the sparse ledger without re-emitting (to avoid
    // loops). `explicit === false` means another window reset the key: forget the
    // value so this window also resolves to the registry default, and drop it from
    // the ledger so our own next save doesn't re-persist it. Any other shape (an
    // explicit set, or a legacy payload without the flag) marks it explicit.
    const before = effectiveValue(id as SettingId)
    if (!explicit) {
      settingsCache.delete(id)
      explicitlySet.delete(id as SettingId)
    } else {
      settingsCache.set(id, value)
      explicitlySet.add(id as SettingId)
    }

    // Notify local listeners with what the setting now READS as. Under a managed lock that can
    // be unchanged whatever the other window stored, and then there's nothing to tell.
    const after = effectiveValue(id as SettingId)
    if (getSettingLock(id) !== undefined && isUnchanged(before, after)) return
    notifyListeners(id as SettingId, after)
  })

  log.debug('Cross-window settings listener ready')
}

// ============================================================================
// Core API
// ============================================================================

/** Ids already warned about for a pre-init read, so a tight pre-init read loop can't spam the log. */
const warnedUninitializedReads = new Set<string>()

/**
 * Get a setting value. Returns the default if not set.
 * Must call initializeSettings() first.
 */
export function getSetting<K extends SettingId>(id: K): SettingsValues[K] {
  if (!initialized) {
    // Reading before initializeSettings() completes returns the REGISTRY DEFAULT.
    // Under sparse persistence a read never writes anything (only an explicit
    // setSetting/reset touches the file), so a stray pre-init read can no longer
    // leak a default into `settings.json`. It can still feed a wrong value to a
    // backend hot-apply push, though (this is how a pre-init read of `ai.provider`
    // could quietly configure AI as "off"). Warn — once per id — so an accidental
    // pre-init read surfaces in the logs instead of masquerading as a real value.
    // We warn rather than throw: a stray early read must not crash the UI.
    if (!warnedUninitializedReads.has(id)) {
      warnedUninitializedReads.add(id)
      log.warn('getSetting({id}) called before settings were initialized; returning the registry default', { id })
    }
    return getDefaultValue(id)
  }

  return effectiveValue(id)
}

/**
 * What `id` reads as: the organization's policy over what the store holds. The policy overlays, it
 * never replaces (nothing here writes), so removing the profile brings the person's value back.
 */
function effectiveValue<K extends SettingId>(id: K): SettingsValues[K] {
  return lockedValue(getSettingLock(id), storedValue(id)) as SettingsValues[K]
}

/**
 * Whether the organization's policy changed what `id` reads: `getSetting` returns a value the person
 * (or the default) doesn't hold. A flow that preselects from a read and writes the answer back must
 * not write such a value unless the person picked it, or the policy's display becomes their choice.
 */
export function isOverriddenByPolicy(id: SettingId): boolean {
  return !isUnchanged(effectiveValue(id), storedValue(id))
}

/** What the store holds for `id`, or its registry default: the value before any managed lock. */
function storedValue<K extends SettingId>(id: K): SettingsValues[K] {
  const cached = settingsCache.get(id)
  return cached !== undefined ? (cached as SettingsValues[K]) : getDefaultValue(id)
}

/**
 * A policy change: tell each listener whose setting now READS differently, once, with the new
 * value, so hot-apply (`settings-applier.ts`) re-pushes it. Nothing is persisted, and nothing goes
 * cross-window: every window gets the same backend event and applies it itself.
 */
function applyPolicyChange({ previous, next }: ManagedPolicyChange): void {
  const ids = new Set([...previous.lockedSettings, ...next.lockedSettings].map((locked) => locked.id))
  for (const id of ids) {
    if (getSettingDefinition(id) === undefined) {
      log.warn('The managed policy locks {id}, which this build has no setting for', { id })
      continue
    }
    const stored = storedValue(id as SettingId)
    const before = lockedValue(settingLockIn(previous, id), stored)
    const after = lockedValue(settingLockIn(next, id), stored)
    if (!isUnchanged(before, after)) {
      notifyListeners(id as SettingId, after as SettingsValues[SettingId])
    }
  }
}

/**
 * Set a setting value. Validates against constraints before storing.
 * Throws SettingValidationError if invalid.
 *
 * Idempotent: when `value` strictly equals the currently-cached value, the
 * call returns after validation without scheduling a save, notifying
 * listeners, or emitting the cross-window event. This avoids redundant work
 * for the (common) case of writing the same value twice — e.g. a settings UI
 * onChange that fires for any click, or test setup/teardown that resets a
 * setting back to its already-current value. The cascade for `network.enabled`
 * alone fires 14 `network-host-lost` events through the FE event loop on a
 * real toggle, so the redundant call used to be heavy enough to occasionally
 * starve a concurrent `mcp_round_trip` waiting on `mcp-response`.
 *
 * `===` carries the primitive settings (`boolean | number | string`). It cannot
 * carry the four `string[]` ones, which never arrive by the same reference
 * twice: every writer builds a fresh array, and an MCP `set_setting` gets one
 * out of JSON. `isUnchanged` compares those element-wise so the guard covers
 * them too. If you add a setting whose shape neither branch settles, narrow the
 * comparison further rather than dropping the guard.
 */
function isUnchanged(cached: unknown, value: unknown): boolean {
  if (cached === value) return true
  // Order-sensitive on purpose: these lists render in the order they're stored,
  // so a reorder is a real change.
  if (Array.isArray(cached) && Array.isArray(value)) {
    return cached.length === value.length && cached.every((entry, at) => entry === value[at])
  }
  return false
}

export function setSetting<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  log.debug('setSetting({id}, {value})', { id, value })

  // Validate the value
  validateSettingValue(id, value)

  // The organization's policy rules this value out. Its row is disabled and MCP refuses before
  // getting here (`managed_policy::refuses_write`), so this is the backstop, not the gate.
  if (!lockAllowsWrite(getSettingLock(id), value)) {
    log.info('setSetting({id}): refused, the organization manages this setting', { id })
    return
  }

  // Idempotency: skip the cascade when nothing actually changed.
  if (isUnchanged(settingsCache.get(id), value)) {
    log.debug('setSetting({id}): unchanged, skipping notify+save+emit', { id })
    return
  }

  // Update cache immediately for synchronous access, and record the explicit
  // choice so the sparse save persists exactly this key (structural, not a
  // value compare — persists even when `value` equals the registry default).
  settingsCache.set(id, value)
  explicitlySet.add(id)

  if (restrictedWindowMode) {
    // No store in this window: persistence goes through the typed backend
    // command, which forwards to the main window's restricted-settings bridge.
    const persistable = RESTRICTED_PERSISTABLE_SETTINGS[id]
    if (persistable !== undefined && typeof value === 'boolean') {
      void persistRestrictedWindowSetting(persistable, value)
        .then((result) => {
          if (result.status === 'error') {
            log.warn('Failed to persist {id} from restricted window: {error}', { id, error: result.error })
          }
        })
        .catch((error: unknown) => {
          log.warn('Failed to persist {id} from restricted window: {error}', { id, error: String(error) })
        })
    } else {
      log.debug('Restricted window: {id} change is session-only (not in the persist allowlist)', { id })
    }
  } else {
    // Debounced save to disk
    scheduleSave()
  }

  // Notify local listeners
  notifyListeners(id, value)

  // Emit cross-window event so other windows get the update (and mirror the
  // explicit choice into their ledger).
  void emit(SETTING_CHANGED_EVENT, { id, value, explicit: true } satisfies SettingChangedPayload)
  log.debug('Emitted cross-window setting change event for {id}', { id })
}

/**
 * Persists a value on behalf of a restricted window. Called by the main
 * window's restricted-settings bridge (see `restricted-settings-bridge.ts`)
 * after it allowlist-checks the forwarded change.
 *
 * Deliberately NOT `setSetting`: the restricted window's own cross-window
 * `settings:changed` emit has usually already synced this window's cache (and
 * notified listeners) by the time the persist request arrives, so `setSetting`
 * would hit the idempotency guard and skip the save. This writes the cache and
 * schedules the save unconditionally, and skips notify/emit to avoid echoing
 * the change back out a second time.
 */
export function persistSettingFromRestrictedWindow<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  validateSettingValue(id, value)
  settingsCache.set(id, value)
  explicitlySet.add(id)
  scheduleSave()
}

/**
 * E2E-only seed: writes the cache and schedules a save WITHOUT emitting the
 * cross-window `settings:changed` event. The whats-new E2E spec seeds an old
 * `lastSeenVersion` and then lets the trigger stamp the current version; a
 * `setSetting` seed's self-echo (the emit loops back to this same window) could
 * land after the stamp and revert it. Skipping the emit avoids that race and
 * matches production, where the seed is read from disk at boot, never emitted.
 * Not for product code: real writes go through `setSetting` so other windows stay
 * in sync.
 */
export function seedSettingForE2E<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  validateSettingValue(id, value)
  settingsCache.set(id, value)
  explicitlySet.add(id)
  scheduleSave()
}

/**
 * Reset a setting to its default value by UNSETTING it: the key is dropped from
 * the ledger and pruned from `settings.json` on the next save, so the value
 * resolves to the registry default (and tracks a future change to that default).
 *
 * Deliberately NOT `setSetting(id, default)` — writing the default back would
 * re-persist it as an explicit choice, which is exactly what sparse persistence
 * avoids. Notifies listeners and emits with the default value (so hot-apply and
 * other windows revert), tagged `explicit: false` so receivers unset it too.
 */
export function resetSetting(id: SettingId): void {
  // A pinned setting keeps the person's own stored choice for when the policy goes away.
  if (getSettingLock(id)?.kind === 'fixed') {
    log.info('resetSetting({id}): refused, the organization manages this setting', { id })
    return
  }

  const wasExplicit = explicitlySet.delete(id)
  const hadCachedValue = settingsCache.has(id)
  if (!wasExplicit && !hadCachedValue) {
    // Already at the registry default; nothing to unset, nothing to broadcast.
    return
  }

  settingsCache.delete(id)
  // What the setting now reads as: the default, or a narrowed setting's fallback if the default
  // is a value the policy rules out.
  const defaultValue = effectiveValue(id)

  if (restrictedWindowMode) {
    // The viewer has no reset affordance; keep the unset session-only rather than
    // forwarding (the persist command only carries a value, not a delete).
    log.debug('Restricted window: reset of {id} is session-only (not forwarded)', { id })
  } else {
    scheduleSave()
  }

  notifyListeners(id, defaultValue)
  void emit(SETTING_CHANGED_EVENT, { id, value: defaultValue, explicit: false } satisfies SettingChangedPayload)
}

/**
 * Whether an actor explicitly set this setting (it's in the sparse-persistence ledger), even to
 * a value equal to its default. Tells "never set" from "set to the default", which
 * {@link isModified} can't: a one-time migration uses it to leave the person's own answer alone.
 */
export function isExplicitlySet(id: SettingId): boolean {
  return explicitlySet.has(id)
}

/**
 * Check if a setting has been modified from its default value. Compares what the STORE holds, so a
 * managed lock never makes a setting look modified (nobody changed it).
 */
export function isModified(id: SettingId): boolean {
  return storedValue(id) !== getDefaultValue(id)
}

// ============================================================================
// Persistence
// ============================================================================

function scheduleSave(): void {
  if (saveTimeout) {
    clearTimeout(saveTimeout)
  }

  saveTimeout = setTimeout(() => {
    void saveToStore().finally(() => {
      saveTimeout = null
    })
  }, SAVE_DEBOUNCE_MS)
}

/**
 * Writes the sparse explicit set to disk. Returns whether the write landed, so
 * `forceSave()` can report it; the debounced path ignores the result.
 *
 * A failed attempt is retried once, and the retry re-runs the WHOLE write: an
 * attempt can throw partway through the `set` loop, so re-flushing alone would
 * report keys that never reached the store.
 */
async function saveToStore(): Promise<boolean> {
  log.debug('saveToStore() called')

  try {
    await writeExplicitSettings()
    return true
  } catch (error) {
    log.error('Failed to save settings: {error}', { error })
    try {
      log.debug('Retrying save...')
      await writeExplicitSettings()
      log.info('Retry save succeeded')
      return true
    } catch (retryError) {
      log.error('Retry save failed: {error}', { error: retryError })
      return false
    }
  }
}

async function writeExplicitSettings(): Promise<void> {
  const store = await getStore()
  const existingKeys = new Set(await store.keys())

  // Sparse write: persist exactly the explicitly-set keys (structural — driven
  // by which mutator ran, NEVER a value compare against the default), and drop
  // any registry key that's persisted but no longer explicit (e.g. after a
  // reset) so it resolves back to the registry default. Non-registry keys are
  // left untouched: this loop only knows registered ids, so a key that left the
  // registry needs a `migrateSettings()` case that deletes it.
  let savedCount = 0
  let removedCount = 0

  for (const def of settingsRegistry) {
    const id = def.id
    if (explicitlySet.has(id)) {
      await store.set(id, settingsCache.get(id))
      savedCount++
    } else if (existingKeys.has(id)) {
      await store.delete(id)
      removedCount++
    }
  }

  await store.set('_schemaVersion', SCHEMA_VERSION)
  await store.save()
  log.info('Settings saved: {saved} explicit values, {removed} reset to default', {
    saved: savedCount,
    removed: removedCount,
  })
}

// ============================================================================
// Change Listeners
// ============================================================================

/**
 * A change to setting `id`, carrying its new `value`. Unlike
 * `onSpecificSettingChange` (which drops the id since every call site already
 * knows it), this all-settings listener genuinely needs both: a
 * `(id, value) => void` shape let a listener declare just the one parameter it
 * cared about — TypeScript allows a callback to take fewer parameters than the
 * type offers — and silently bind the id into what looked like the value slot.
 * A single object payload makes dropping or swapping a field a compile error.
 */
export interface SettingChangePayload<K extends SettingId = SettingId> {
  id: K
  value: SettingsValues[K]
}

type SettingChangeListener<K extends SettingId = SettingId> = (change: SettingChangePayload<K>) => void

// The specific-listener registry drops the id `onSpecificSettingChange` echoes back from its
// listener signature (see the export's doc comment for why), so it can't share `SettingChangeListener`
// with the all-settings registry above. `SettingsValues[K]` isn't expressible as a single type
// parameterized over every `K` in one `Set`, so the map is keyed by the (erased) value type and the
// one dispatch site below casts back to the call's own `K` — the cast is narrow and unavoidable, not
// a way to paper over a real mismatch.
type SpecificSettingChangeListener = (value: never) => void

const listeners = new Set<SettingChangeListener>()
const specificListeners = new Map<SettingId, Set<SpecificSettingChangeListener>>()

/**
 * Subscribe to all setting changes.
 */
export function onSettingChange(listener: SettingChangeListener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/**
 * Subscribe to changes for a specific setting.
 *
 * The listener takes only the value, not `(id, value)`: since every call site already knows
 * `id` (it just passed it in), a `(id, value) => void` signature let a listener declare just
 * one parameter — TypeScript allows a callback to take fewer parameters than the type offers —
 * and silently bind the ECHOED ID into what looked like the value slot. That shipped as a real
 * bug (an email field rendering the literal id `analytics.email`). Dropping the parameter makes
 * that class of bug unrepresentable here.
 */
export function onSpecificSettingChange<K extends SettingId>(
  id: K,
  listener: (value: SettingsValues[K]) => void,
): () => void {
  let set = specificListeners.get(id)
  if (!set) {
    set = new Set()
    specificListeners.set(id, set)
  }
  set.add(listener)
  return () => set.delete(listener)
}

function notifyListeners<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  // Notify global listeners
  for (const listener of listeners) {
    try {
      listener({ id, value })
    } catch (error) {
      log.error('Setting change listener error: {error}', { error })
    }
  }

  // Notify specific listeners
  const specific = specificListeners.get(id)
  if (specific) {
    for (const listener of specific) {
      try {
        listener(value as never)
      } catch (error) {
        log.error('Setting change listener error: {error}', { error })
      }
    }
  }
}

// ============================================================================
// Utility: immediate, awaited persistence
// ============================================================================

/**
 * Persists every explicitly-set value NOW, and reports whether the write landed.
 *
 * `setSetting` is fire-and-forget behind a 500 ms debounce, which is right for a
 * preference: the UI already reflects it, and a dropped write costs a re-toggle.
 * It's wrong for the settings that RECORD SOMETHING THAT HAPPENED — onboarding
 * finishing, the Full Disk Access answer, a terms acceptance. A lost write there
 * re-runs onboarding or re-prompts for a permission the user already answered,
 * with no trail. Those callers write, then await this, and log a warning on
 * `false` so the next launch's odd behavior has an explanation in the log. The
 * settings window also calls it on close, to beat the debounce.
 *
 * Cancels any pending debounce (its work is subsumed) and always writes, even
 * with nothing pending: a caller asking "is it on disk?" wants the disk checked,
 * not the timer inspected. A restricted window has no store, so this reports
 * `false` without touching it — going through `saveToStore` would `log.error`,
 * which auto-reports on every viewer open.
 */
export async function forceSave(): Promise<boolean> {
  if (restrictedWindowMode) {
    log.warn('forceSave() in a restricted window: no store here, nothing was written')
    return false
  }
  if (saveTimeout) {
    clearTimeout(saveTimeout)
    saveTimeout = null
  }
  return saveToStore()
}

// ============================================================================
// Export validation error for external use
// ============================================================================

export { SettingValidationError }
