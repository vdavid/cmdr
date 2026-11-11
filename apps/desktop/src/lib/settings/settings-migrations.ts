// Settings schema migrations: what `settings-store.ts` runs when `settings.json` carries an older
// `_schemaVersion` than the one below.

import type { Store } from '@tauri-apps/plugin-store'
import type { SettingId } from './types'
import { getDefaultValue } from './settings-registry'
import { getAppLogger } from '#lib/logging/logger.js'
import { pluralize } from '#lib/utils/pluralize.js'
import { ARCHIVE_ENTER_FORMATS, isEnterAction } from '#lib/file-explorer/pane/archive-enter-policy.js'
import { hasCapability } from '#lib/platform/index.js'

const log = getAppLogger('settings')

/**
 * One step per schema version, in order. Each returns whether it changed anything, and each MUST
 * be idempotent: on a fresh install nothing stamps `_schemaVersion` until the first real save, so
 * every step re-runs each launch until then.
 */
const MIGRATIONS: { to: number; run: (store: Store) => Promise<boolean> }[] = [
  // Version 1 was the original schema: nothing to do.
  { to: 2, run: renameDateColorsOffToNone },
  { to: 3, run: stateImageIndexScopeForExistingUsers },
  { to: 4, run: migrateOnboardingKeysIntoRegistry },
  { to: 5, run: migrateArchiveEnterBlobIntoPerFormatKeys },
  { to: 6, run: renameFdaOpenStateToUnanswered },
  { to: 7, run: dropOldHourlyUpdateCheckDefault },
  // TODO(2027-01-14): delete `dropLeftoverOpenaiApiKey`, keeping this slot as `run: () => Promise.resolve(false)`.
  { to: 8, run: dropLeftoverOpenaiApiKey },
  { to: 9, run: turnOffImageIndexingWithoutAnEngine },
]

/** The schema version `settings.json` is at once every migration has run. */
export const SCHEMA_VERSION = MIGRATIONS[MIGRATIONS.length - 1].to

/**
 * Migration 2: `appearance.dateColors` renamed its "no coloring" value from `off` to `none` to
 * match `appearance.sizeColors`.
 */
async function renameDateColorsOffToNone(store: Store): Promise<boolean> {
  if ((await store.get<string>('appearance.dateColors')) !== 'off') return false
  await store.set('appearance.dateColors', 'none')
  return true
}

/**
 * Migration 3: `mediaIndex.scope` split "which folders do we index?" out of the importance
 * slider, and its default is the narrow "only folders I choose". Anyone who already had image
 * indexing ON is running the automatic behavior, so state it for them rather than silently
 * narrowing what they've already indexed. Everyone else (the feature is off by default) takes the
 * new default. Only writes when the key is still absent. The Rust `gate::scope_from_settings`
 * applies the same rule at startup, so the launch before this runs behaves the same.
 */
async function stateImageIndexScopeForExistingUsers(store: Store): Promise<boolean> {
  const scope = await store.get<string>('mediaIndex.scope')
  const imageIndexOn = await store.get<boolean>('mediaIndex.enabled')
  if (scope !== undefined || imageIndexOn !== true) return false
  await store.set('mediaIndex.scope', 'importance')
  return true
}

/**
 * Migration 6: `onboarding.fullDiskAccessChoice` renamed its open state from `notAskedYet` to
 * `unanswered`. The old name claimed nobody had been asked, when the wizard opens on that step at
 * EVERY launch while it holds: the value really means the question is open. Rust reads the same
 * file at startup and keeps a serde alias for the old token, so a launch that hasn't run this
 * migration yet still gates correctly. Only rewrites the exact old value.
 */
async function renameFdaOpenStateToUnanswered(store: Store): Promise<boolean> {
  if ((await store.get<string>('onboarding.fullDiskAccessChoice')) !== 'notAskedYet') return false
  await store.set('onboarding.fullDiskAccessChoice', 'unanswered')
  return true
}

/**
 * Migration 7: `advanced.updateCheckInterval`'s default moved from one hour to three. Before the
 * store went sparse, a first launch wrote the whole registry-default map to disk, so a stored one
 * hour is almost always that old default rather than a choice, and it would keep those installs
 * checking hourly. Dropping the key lets the current default apply. Only the exact old value goes.
 */
async function dropOldHourlyUpdateCheckDefault(store: Store): Promise<boolean> {
  if ((await store.get<number>('advanced.updateCheckInterval')) !== 3_600_000) return false
  await store.delete('advanced.updateCheckInterval')
  return true
}

/**
 * Migration 8: releases before AI keys moved to the Keychain kept the OpenAI key in plain text
 * here, and an install that skipped every migrating release still has it, unused. Deleted blind:
 * ❌ never read the value.
 */
async function dropLeftoverOpenaiApiKey(store: Store): Promise<boolean> {
  return await store.delete('ai.openaiApiKey')
}

/**
 * Migration 9 (#1001): before #943, a Linux install could turn image indexing on, and the test
 * fake that was its backend filled `media.db` with text made up from file names. Without an image
 * engine the backend now purges those stores and starts nothing, and the Settings page is gone,
 * so a stored "on" would leave the file-list badges and the search grid pointing at nothing, with
 * no switch to turn them off. Dropping the key reads as the default, off.
 */
async function turnOffImageIndexingWithoutAnEngine(store: Store): Promise<boolean> {
  if (hasCapability('imageIndexing')) return false
  if ((await store.get<boolean>('mediaIndex.enabled')) !== true) return false
  await store.delete('mediaIndex.enabled')
  return true
}

/**
 * Migrate settings from older schema versions: runs every step newer than `fromVersion`, then
 * stamps the version.
 */
export async function migrateSettings(store: Store, fromVersion: number): Promise<void> {
  let changed = false
  for (const migration of MIGRATIONS) {
    if (fromVersion < migration.to && (await migration.run(store))) {
      changed = true
    }
  }

  // Persist the version stamp only when this launch has something to write: a
  // value the migration changed, or an already-populated file. A brand-new
  // install with no file writes nothing — the sparse invariant that
  // `settings.json` doesn't exist until an actor sets something. Consequence:
  // on a fresh install the migration re-runs each launch until the first real
  // save stamps `_schemaVersion`, so every migration step MUST be idempotent.
  const fileHasKeys = (await store.keys()).length > 0
  if (changed || fileHasKeys) {
    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()
  }
}

/**
 * Migration 4: onboarding's four keys move out of a hand-rolled second store over the
 * same `settings.json` and into the registry, gaining dot-notation ids. Returns whether
 * anything changed.
 *
 * The values are carried across, not defaulted: dropping them would re-run the onboarding
 * wizard and re-ask for Full Disk Access. The old keys are then deleted, because they sit
 * outside the registry and the sparse save can't prune them.
 *
 * Idempotent: it only acts on a legacy key that's still present, and the delete is what
 * makes the second run a no-op. Only the store is written; the cache load right after this
 * migration reads every registry key back out of it, so writing the cache here too would
 * just be a way for the two to drift.
 *
 * `null` was the legacy "never accepted" marker on the two terms keys; the registry
 * default is `''`, so a null maps to "don't write" rather than to a stored null (which
 * would fail validation on the next load).
 */
const LEGACY_ONBOARDING_KEYS: { from: string; to: SettingId }[] = [
  { from: 'isOnboarded', to: 'onboarding.completed' },
  { from: 'fullDiskAccessChoice', to: 'onboarding.fullDiskAccessChoice' },
  { from: 'termsAcceptedVersion', to: 'onboarding.termsAcceptedVersion' },
  { from: 'termsAcceptedAt', to: 'onboarding.termsAcceptedAt' },
]

async function migrateOnboardingKeysIntoRegistry(store: Store): Promise<boolean> {
  const movedKeys: string[] = []
  for (const { from, to } of LEGACY_ONBOARDING_KEYS) {
    const legacyValue = await store.get<unknown>(from)
    if (legacyValue === undefined) continue
    movedKeys.push(from)
    if (legacyValue === null) continue
    if (typeof legacyValue !== typeof getDefaultValue(to)) {
      log.warn('Migration 4: dropping {from} with unexpected type {type}', { from, type: typeof legacyValue })
      continue
    }
    await store.set(to, legacyValue)
  }
  if (movedKeys.length === 0) return false
  for (const key of movedKeys) {
    await store.delete(key)
  }
  log.info('Migration 4: moved {count} onboarding {keysNoun} into the settings registry', {
    count: movedKeys.length,
    keysNoun: pluralize(movedKeys.length, 'key'),
  })
  return true
}

/**
 * Migration 5: the one `behavior.archiveEnterBehavior` JSON blob (`{ zip: 'ask',
 * bundle: 'open' }`) unpacks into one registry setting per archive format. Returns
 * whether anything changed.
 *
 * A format the blob never named keeps NO key, so it resolves to its registry default
 * and stays open to a future default change — the sparse-persistence contract. A blob
 * we can't read (hand-edited, half-written, naming a format that's gone) writes
 * nothing: every format falls to its default, which is the same answer the resolver
 * gave for an unreadable blob before the split, so nobody's Enter key changes meaning
 * because a value was garbled.
 *
 * Idempotent: the legacy key is deleted either way, and that delete is what makes the
 * second run a no-op. It has to be explicit — the key sits outside the registry, so
 * the sparse save can't prune it.
 */
const LEGACY_ARCHIVE_ENTER_BLOB_KEY = 'behavior.archiveEnterBehavior'

async function migrateArchiveEnterBlobIntoPerFormatKeys(store: Store): Promise<boolean> {
  const stored = await store.get<unknown>(LEGACY_ARCHIVE_ENTER_BLOB_KEY)
  if (stored === undefined) return false

  const byFormatKey = typeof stored === 'string' ? parseJsonObject(stored) : undefined
  const moved: string[] = []
  for (const format of ARCHIVE_ENTER_FORMATS) {
    // Iterating the formats (rather than the blob's own keys) is what drops a format
    // that no longer exists; `isEnterAction` drops a value that never did.
    const action = byFormatKey?.[format.key]
    if (!isEnterAction(action)) continue
    await store.set(format.settingId, action)
    moved.push(format.key)
  }

  await store.delete(LEGACY_ARCHIVE_ENTER_BLOB_KEY)
  log.info('Migration 5: unpacked the archive Enter blob into {count} per-format {keysNoun}', {
    count: moved.length,
    keysNoun: pluralize(moved.length, 'key'),
  })
  return true
}

/** The parsed object at the top level of `json`, or `undefined` for anything else. */
function parseJsonObject(json: string): Record<string, unknown> | undefined {
  let raw: unknown
  try {
    raw = JSON.parse(json)
  } catch {
    return undefined
  }
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return undefined
  return raw as Record<string, unknown>
}
