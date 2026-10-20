/**
 * Round-trip persistence tests for the settings store's sparse model.
 *
 * `settings.json` holds ONLY keys an actor explicitly set (the Settings UI, the
 * MCP `set_setting` tool, a migration). Everything else resolves to the registry
 * default at read time. "Explicit" is a STRUCTURAL property of the write path
 * (which mutator ran), never a value comparison against the default: a deliberate
 * choice that happens to equal the default must persist so it survives a future
 * default change. These tests pin that contract:
 *
 *  (a) a fresh store with no file resolves to registry defaults and SAVES NOTHING,
 *  (b) an explicit set persists exactly that key,
 *  (c) a set to a value equal to the default STILL persists (structural),
 *  (d) reset DELETES the key (resolves back to default),
 *  (e) a legacy full `settings.json` keeps every present key (incl. equal-to-default),
 *  (f) pre-init reads return defaults and write nothing.
 *
 * Mirrors `shortcuts-store.test.ts`: a shared `disk` Map backs a fake
 * `@tauri-apps/plugin-store`, and `vi.resetModules()` + a fresh dynamic import
 * simulates a webview reload re-reading the file. The store's module-scoped
 * cache/explicit-set start empty on each import.
 */
import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest'
import { getDefaultValue } from './settings-registry'
import { SCHEMA_VERSION } from './settings-migrations'

// Shared backing store for the fake plugin-store, persisting across
// `vi.resetModules()` to stand in for the on-disk `settings.json`.
const disk = vi.hoisted(() => new Map<string, unknown>())

/**
 * Flipped by the `forceSave` tests: `saveFails` makes the fake disk reject every save, and
 * `setFailsOnceFor` makes one key's next `set` throw, partway through a save's write loop.
 */
const diskState = vi.hoisted(() => ({ saveFails: false, setFailsOnceFor: null as string | null }))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn((_path: string, opts?: { defaults?: Record<string, unknown> }) => {
    // Apply defaults only for keys not already present (matches plugin-store).
    for (const [k, v] of Object.entries(opts?.defaults ?? {})) {
      if (!disk.has(k)) disk.set(k, v)
    }
    return Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        if (diskState.setFailsOnceFor === key) {
          diskState.setFailsOnceFor = null
          return Promise.reject(new Error('store hiccup'))
        }
        disk.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(disk.delete(key)),
      has: (key: string) => Promise.resolve(disk.has(key)),
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => (diskState.saveFails ? Promise.reject(new Error('disk full')) : Promise.resolve()),
    })
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}))

vi.mock('./store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

// The backend-facing settings commands are irrelevant to on-disk persistence;
// stub them so init's fire-and-forget `record_settings_defaults` push and the
// restricted-window snapshot don't hit a real IPC layer.
vi.mock('#lib/tauri-commands/settings.js', () => ({
  recordSettingsDefaults: vi.fn(() => Promise.resolve()),
  getRestrictedWindowSettings: vi.fn(() => Promise.resolve({})),
  persistRestrictedWindowSetting: vi.fn(() => Promise.resolve({ status: 'ok' })),
}))

// Stateless and heavy (each pulls in the i18n runtime), so passed through to load
// once per file: see `docs/testing.md` § "Reloading a module per test". Init's
// managed-policy load imports the IPC barrel dynamically.
vi.mock('./settings-registry', async (importOriginal) => await importOriginal())
vi.mock('./settings-migrations', async (importOriginal) => await importOriginal())
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => await importOriginal())
// Warm Vite's transform cache: `settings-store` transitively pulls the whole
// intl catalog, a cold transform runs several seconds and would race the
// per-test timeout. A `beforeAll` gets the longer hook budget.
beforeAll(async () => {
  await import('./settings-store')
})

async function loadStore() {
  return await import('./settings-store')
}

// Keys the store writes that aren't settings themselves.
const META_KEYS = new Set(['_schemaVersion'])

/** Setting ids currently persisted on disk (excludes bookkeeping keys). */
function persistedSettingKeys(): string[] {
  return [...disk.keys()].filter((k) => !META_KEYS.has(k))
}

beforeEach(() => {
  disk.clear()
  diskState.saveFails = false
  diskState.setFailsOnceFor = null
  vi.resetModules()
})

describe('sparse settings persistence', () => {
  it('(a) a fresh store with no file saves nothing', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    // Init must not materialize anything: no settings, no bookkeeping key.
    expect([...disk.keys()]).toEqual([])
    // Reads still resolve to the registry default.
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
  })

  it('(b) an explicit set persists exactly that key', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    await store.forceSave()

    expect(disk.get('developer.mcpEnabled')).toBe(true)
    expect(persistedSettingKeys()).toEqual(['developer.mcpEnabled'])
  })

  it('(c) a set to the default value still persists (structural, not value-compare)', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    const def = getDefaultValue('developer.mcpEnabled')
    expect(def).toBe(false) // guard: the whole point is persisting a default-equal choice
    store.setSetting('developer.mcpEnabled', def)
    await store.forceSave()

    // Present on disk even though it equals the registry default: a deliberate
    // choice must survive a future default flip.
    expect(disk.has('developer.mcpEnabled')).toBe(true)
    expect(disk.get('developer.mcpEnabled')).toBe(false)
  })

  it('(c2) isExplicitlySet tells "set to the default" from "never set", which isModified cannot', async () => {
    disk.set('developer.verboseLogging', getDefaultValue('developer.verboseLogging'))
    disk.set('_schemaVersion', 2)
    const store = await loadStore()
    await store.initializeSettings()

    // Present on load at its default: explicit, yet not "modified".
    expect(store.isExplicitlySet('developer.verboseLogging')).toBe(true)
    expect(store.isModified('developer.verboseLogging')).toBe(false)
    // Never touched: neither.
    expect(store.isExplicitlySet('developer.mcpEnabled')).toBe(false)

    store.setSetting('developer.mcpEnabled', getDefaultValue('developer.mcpEnabled'))
    expect(store.isExplicitlySet('developer.mcpEnabled')).toBe(true)

    store.resetSetting('developer.mcpEnabled')
    expect(store.isExplicitlySet('developer.mcpEnabled')).toBe(false)
    // Settle the debounced save now: left pending, it fires into a LATER test's disk.
    await store.forceSave()
  })

  it('(d) reset deletes the key so it resolves back to default', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    await store.forceSave()
    expect(disk.has('developer.mcpEnabled')).toBe(true)

    store.resetSetting('developer.mcpEnabled')
    await store.forceSave()

    expect(disk.has('developer.mcpEnabled')).toBe(false)
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
  })

  it('(e) a legacy full settings.json keeps every present key, incl. equal-to-default', async () => {
    // Seed a legacy file: a non-default choice plus a key explicitly stored at
    // its default value. Both are "explicit" because they're present on load.
    disk.set('developer.mcpEnabled', true)
    disk.set('developer.verboseLogging', getDefaultValue('developer.verboseLogging')) // equals default
    disk.set('_schemaVersion', 2)

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('developer.mcpEnabled')).toBe(true)
    expect(store.getSetting('developer.verboseLogging')).toBe(getDefaultValue('developer.verboseLogging'))

    // A save must not drop the equal-to-default key that was present on load.
    await store.forceSave()
    expect(disk.has('developer.mcpEnabled')).toBe(true)
    expect(disk.has('developer.verboseLogging')).toBe(true)
  })

  it('(g) a stored below-floor local context window reads as the 32,768 floor', async () => {
    // A tester who picked 4,096 in an earlier build had a chat that could not complete one
    // message: the prefix alone costs more than 60% of that window. 4,096 is no longer an
    // option, so it fails validation on load and the read resolves to the default, which IS
    // the floor (`agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS`). They get a working chat
    // without touching a thing.
    disk.set('ai.localContextSize', '4096')
    disk.set('_schemaVersion', 3)

    const store = await loadStore()
    await store.initializeSettings()

    expect(getDefaultValue('ai.localContextSize')).toBe('32768')
    expect(store.getSetting('ai.localContextSize')).toBe('32768')
    // Dropped, not kept: the next save prunes it rather than leaving a value the UI can't show.
    await store.forceSave()
    expect(disk.has('ai.localContextSize')).toBe(false)
  })

  it('(h) forceSave answers false when the write cannot land', async () => {
    // The caller contract that matters: onboarding, the Full Disk Access answer, and the
    // terms acceptance all gate a `log.warn` on this boolean. A silent `true` here would
    // mean the next launch re-asks with nothing in the log to explain why.
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    diskState.saveFails = true
    expect(await store.forceSave()).toBe(false)

    diskState.saveFails = false
    expect(await store.forceSave()).toBe(true)
  })

  it('(h2) a write that throws partway through is retried in full, so a true means every key landed', async () => {
    // Pre-fix the retry only re-ran `store.save()`, which flushed the keys set before the
    // throw and reported `true` for the ones after it that never reached the store.
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    store.setSetting('developer.verboseLogging', true)
    diskState.setFailsOnceFor = 'developer.verboseLogging'

    expect(await store.forceSave()).toBe(true)
    expect(disk.get('developer.mcpEnabled')).toBe(true)
    expect(disk.get('developer.verboseLogging')).toBe(true)
  })

  it('(i) a hidden-files choice survives the default, an untouched store follows it', async () => {
    // This is what makes the "hidden files off by default" flip safe without a migration:
    // whoever deliberately turned dotfiles on has the key on disk and keeps them, and
    // whoever never touched the switch picks up the default. Both toggle paths (the
    // Settings switch and the native View menu, which arrives as `settings-changed`) go
    // through `setSetting`, so both land in the explicit ledger.
    disk.set('listing.showHiddenFiles', true)
    disk.set('_schemaVersion', 4)

    const chose = await loadStore()
    await chose.initializeSettings()
    expect(chose.getSetting('listing.showHiddenFiles')).toBe(true)
    await chose.forceSave()
    expect(disk.get('listing.showHiddenFiles')).toBe(true)

    // A store nobody wrote that key into resolves to the registry default.
    disk.clear()
    vi.resetModules()
    const untouched = await loadStore()
    await untouched.initializeSettings()
    expect(untouched.getSetting('listing.showHiddenFiles')).toBe(false)
  })

  it('(f) pre-init reads return defaults and write nothing', async () => {
    const store = await loadStore()

    // Read several settings before initializeSettings() resolves.
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
    expect(store.getSetting('developer.verboseLogging')).toBe(getDefaultValue('developer.verboseLogging'))

    await store.initializeSettings()

    // Those reads must not have leaked anything to disk.
    expect([...disk.keys()]).toEqual([])
  })
})

/**
 * Migration 5 unpacks the one `behavior.archiveEnterBehavior` JSON blob into one
 * setting per archive format. The blob was the user's own choice about what Enter does,
 * so dropping it silently changes the behavior of the key they press most; and because
 * the blob sits outside the registry, only an explicit delete gets it off disk (the
 * sparse save can't prune what it doesn't know about).
 */
describe('migration 5: the archive Enter blob unpacks into one setting per format', () => {
  /** A pre-migration file holding `stored` as the blob, stamped at the previous schema. */
  function seedBlob(stored: unknown): void {
    disk.set('behavior.archiveEnterBehavior', stored)
    disk.set('_schemaVersion', 4)
  }

  it('carries every format the user chose across to its own key', async () => {
    seedBlob('{"zip":"ask","bundle":"open"}')

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('behavior.archiveEnter.zip')).toBe('ask')
    expect(store.getSetting('behavior.archiveEnter.bundle')).toBe('open')
    // Never in the blob (the format wasn't configurable), so it takes its registry default.
    expect(store.getSetting('behavior.archiveEnter.ooxml')).toBe('open')
    expect(disk.has('behavior.archiveEnterBehavior')).toBe(false)
    expect(disk.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })

  it('leaves a format the blob never mentioned on its default, with nothing on disk', async () => {
    seedBlob('{"zip":"browse"}')

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('behavior.archiveEnter.zip')).toBe('browse')
    expect(store.getSetting('behavior.archiveEnter.bundle')).toBe(getDefaultValue('behavior.archiveEnter.bundle'))
    // Sparse persistence: an untouched format must NOT gain a key, or a future
    // default change would skip everyone this migration ran for.
    await store.forceSave()
    expect(disk.has('behavior.archiveEnter.bundle')).toBe(false)
  })

  it('writes nothing when the install never had the blob', async () => {
    disk.set('developer.mcpEnabled', true) // an unrelated key, so the file exists
    disk.set('_schemaVersion', 4)

    const store = await loadStore()
    await store.initializeSettings()
    await store.forceSave()

    expect(persistedSettingKeys()).toEqual(['developer.mcpEnabled'])
  })

  it('drops a blob it cannot read rather than leaving it on disk forever', async () => {
    // Anything that isn't a JSON object of known formats and valid actions: a
    // hand-edited file, a half-written value, a format that no longer exists.
    // Every format falls to its default, which is the same answer the resolver
    // gave for an unreadable blob before the split.
    seedBlob('{"zip":"nope","rar":"browse"')

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('behavior.archiveEnter.zip')).toBe(getDefaultValue('behavior.archiveEnter.zip'))
    expect(disk.has('behavior.archiveEnterBehavior')).toBe(false)
    await store.forceSave()
    expect(persistedSettingKeys()).toEqual([])
  })

  it('ignores an unknown format key and an invalid action inside a readable blob', async () => {
    seedBlob('{"zip":"nope","rar":"browse","bundle":"ask"}')

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('behavior.archiveEnter.bundle')).toBe('ask')
    expect(store.getSetting('behavior.archiveEnter.zip')).toBe(getDefaultValue('behavior.archiveEnter.zip'))
    await store.forceSave()
    expect(persistedSettingKeys()).toEqual(['behavior.archiveEnter.bundle'])
  })

  it('survives a re-run: the second launch finds no blob and changes nothing', async () => {
    // The stamp only lands once something is written, so on some installs this
    // re-runs on the next launch. Deleting the blob is what makes that a no-op.
    seedBlob('{"zip":"browse","bundle":"open"}')
    const first = await loadStore()
    await first.initializeSettings()
    const afterFirst = new Map(disk)

    vi.resetModules()
    const second = await loadStore()
    await second.initializeSettings()

    expect([...disk.entries()].sort()).toEqual([...afterFirst.entries()].sort())
    expect(second.getSetting('behavior.archiveEnter.zip')).toBe('browse')
  })
})

/**
 * Migration 4 lifted onboarding's four keys out of a hand-rolled second store over the
 * same `settings.json` and into the registry. Getting this wrong is expensive in a way a
 * preference migration isn't: a dropped value re-runs the onboarding wizard or re-asks
 * for Full Disk Access on someone who already answered.
 */
describe('migration 4: the onboarding keys move into the registry', () => {
  /** A pre-migration file: legacy top-level keys, stamped at the previous schema. */
  function seedLegacyStore(overrides: Record<string, unknown> = {}): void {
    disk.set('isOnboarded', true)
    disk.set('fullDiskAccessChoice', 'deny')
    disk.set('termsAcceptedVersion', '2026-07-01')
    disk.set('termsAcceptedAt', '2026-07-02T09:00:00.000Z')
    disk.set('_schemaVersion', 3)
    for (const [k, v] of Object.entries(overrides)) disk.set(k, v)
  }

  it('carries every value across under its new id', async () => {
    seedLegacyStore()

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('onboarding.completed')).toBe(true)
    expect(store.getSetting('onboarding.fullDiskAccessChoice')).toBe('deny')
    expect(store.getSetting('onboarding.termsAcceptedVersion')).toBe('2026-07-01')
    expect(store.getSetting('onboarding.termsAcceptedAt')).toBe('2026-07-02T09:00:00.000Z')
  })

  it('drops the legacy keys, which the sparse save would otherwise leave forever', async () => {
    seedLegacyStore()

    const store = await loadStore()
    await store.initializeSettings()

    for (const legacy of ['isOnboarded', 'fullDiskAccessChoice', 'termsAcceptedVersion', 'termsAcceptedAt']) {
      expect(disk.has(legacy)).toBe(false)
    }
    // A run stamps the CURRENT schema version, not the one whose migration did the
    // work, so this number moves with every bump.
    expect(disk.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })

  it('survives a re-run: the second launch finds nothing to move and changes nothing', async () => {
    seedLegacyStore()
    const first = await loadStore()
    await first.initializeSettings()
    const afterFirst = new Map(disk)

    vi.resetModules()
    const second = await loadStore()
    await second.initializeSettings()

    expect([...disk.entries()].sort()).toEqual([...afterFirst.entries()].sort())
    expect(second.getSetting('onboarding.completed')).toBe(true)
  })

  it('reads a legacy null terms acceptance as "never accepted", not as a stored null', async () => {
    // `null` was the legacy marker for "never accepted"; the registry default is `''`.
    // Storing the null instead would fail validation on the next load and read as garbage.
    seedLegacyStore({ termsAcceptedVersion: null, termsAcceptedAt: null })

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('onboarding.termsAcceptedVersion')).toBe('')
    expect(disk.has('onboarding.termsAcceptedVersion')).toBe(false)
  })

  it('leaves a fresh install alone: nothing to move, nothing written', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    expect([...disk.keys()]).toEqual([])
    expect(store.getSetting('onboarding.completed')).toBe(false)
  })
})

/**
 * Migration 6 renames the Full Disk Access choice's open state. The old token claimed
 * nobody had been asked; the wizard actually opens on that step at every launch while it
 * holds, so the value means the question is open. The name misled a real triage into
 * reading "onboarding never ran" off a report from someone who had seen the step for days.
 */
describe('migration 6: the open FDA state renames to `unanswered`', () => {
  it('rewrites the old token', async () => {
    disk.set('onboarding.fullDiskAccessChoice', 'notAskedYet')
    disk.set('_schemaVersion', 5)

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('onboarding.fullDiskAccessChoice')).toBe('unanswered')
    expect(disk.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })

  it('leaves a real answer alone', async () => {
    disk.set('onboarding.fullDiskAccessChoice', 'deny')
    disk.set('_schemaVersion', 5)

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('onboarding.fullDiskAccessChoice')).toBe('deny')
  })

  // Sparse persistence: a fresh install has no file, and a rename must not create one.
  it('writes nothing on a fresh install', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    expect([...disk.keys()]).toEqual([])
    expect(store.getSetting('onboarding.fullDiskAccessChoice')).toBe('unanswered')
  })
})

/**
 * Migration 7 lets the old one-hour update-check default go. Until the store went sparse, every
 * install's first launch wrote the whole registry-default map to disk, so a stored 60 minutes is
 * almost always that old default rather than a choice, and it would pin those installs to hourly
 * checks after the default moved to three hours.
 */
describe('migration 7: a stored one-hour update check interval takes the new default', () => {
  it('drops a stored 60 minutes, so the three-hour default applies', async () => {
    disk.set('advanced.updateCheckInterval', 3_600_000)
    disk.set('_schemaVersion', 6)

    const store = await loadStore()
    await store.initializeSettings()

    expect(disk.has('advanced.updateCheckInterval')).toBe(false)
    expect(store.getSetting('advanced.updateCheckInterval')).toBe(10_800_000)
    expect(disk.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })

  it('keeps any other interval, which somebody picked', async () => {
    disk.set('advanced.updateCheckInterval', 1_800_000)
    disk.set('_schemaVersion', 6)

    const store = await loadStore()
    await store.initializeSettings()

    expect(disk.get('advanced.updateCheckInterval')).toBe(1_800_000)
    expect(store.getSetting('advanced.updateCheckInterval')).toBe(1_800_000)
  })

  it('writes nothing on a fresh install', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    expect([...disk.keys()]).toEqual([])
  })
})

/**
 * Migration 8 drops the plaintext OpenAI key that releases before the Keychain move left in
 * `settings.json`. Nothing reads it, and the delete never does either.
 */
describe('migration 8: a leftover plaintext OpenAI key is deleted', () => {
  it('deletes the key, so no plaintext secret lingers on disk', async () => {
    disk.set('ai.openaiApiKey', 'sk-leftover')
    disk.set('_schemaVersion', 7)

    const store = await loadStore()
    await store.initializeSettings()

    expect(disk.has('ai.openaiApiKey')).toBe(false)
    expect(disk.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })
})

/**
 * Two windows over one backing store (`tauri-plugin-store` shares one in-memory store per
 * path across windows). Settings saves 500 ms after a change, and a window opened inside
 * that half second loads the store without the new value and starts listening only after
 * loading, so it never hears about it. Each `loadStore()` after `vi.resetModules()` is
 * another window's module state; `emit` is mocked, which is exactly the "never heard" case.
 */
describe('a second window that never heard a change', () => {
  async function openWindow() {
    vi.resetModules()
    const window = await loadStore()
    await window.initializeSettings()
    return window
  }

  it('gives each window its own store module state', async () => {
    // The premise of every test here. Guards the pass-through mocks at the top
    // of the file: caching the store itself would merge the windows silently.
    const settings = await openWindow()
    const main = await openWindow()
    expect(main).not.toBe(settings)
    main.setSetting('viewer.wordWrap', !getDefaultValue('viewer.wordWrap'))
    expect(settings.getSetting('viewer.wordWrap')).toBe(getDefaultValue('viewer.wordWrap'))
  })

  it('keeps a setting changed just before it opened when it closes', async () => {
    const main = await openWindow()
    main.setSetting('developer.mcpEnabled', true) // inside the 500 ms debounce
    const settings = await openWindow()
    expect(await main.forceSave()).toBe(true)

    await settings.forceSave() // the Settings window closing

    expect(disk.get('developer.mcpEnabled')).toBe(true)
  })

  it('keeps a newer value of a key it loaded when it saves its own change', async () => {
    disk.set('viewer.wordWrap', false)
    disk.set('_schemaVersion', SCHEMA_VERSION)
    const main = await openWindow()
    main.setSetting('viewer.wordWrap', true)
    const settings = await openWindow() // still reads `false`
    expect(await main.forceSave()).toBe(true)

    settings.setSetting('viewer.showTextCursor', true)
    expect(await settings.forceSave()).toBe(true)

    expect(disk.get('viewer.wordWrap')).toBe(true)
    expect(disk.get('viewer.showTextCursor')).toBe(true)
  })

  it('writes nothing on close when it changed nothing', async () => {
    const settings = await openWindow()

    expect(await settings.flushSettingsChanges()).toBe(true)

    // Not even the version stamp: no save ran.
    expect([...disk.keys()]).toEqual([])
  })

  it('writes its own pending change on close', async () => {
    const settings = await openWindow()
    settings.setSetting('viewer.wordWrap', true)

    expect(await settings.flushSettingsChanges()).toBe(true)

    expect(disk.get('viewer.wordWrap')).toBe(true)
  })

  it('still deletes a key it reset itself', async () => {
    disk.set('viewer.wordWrap', true)
    disk.set('_schemaVersion', SCHEMA_VERSION)
    const settings = await openWindow()

    settings.resetSetting('viewer.wordWrap')
    expect(await settings.forceSave()).toBe(true)

    expect(disk.has('viewer.wordWrap')).toBe(false)
  })
})
