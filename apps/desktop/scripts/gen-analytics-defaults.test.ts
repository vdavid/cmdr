/**
 * Tests for the analytics settings-defaults generator.
 *
 * The parser is fed a fake `SourceTree` rather than the real repo, so a case says exactly which
 * registry shape it's about and the suite doesn't move every time a setting's default changes.
 */

import { describe, expect, it } from 'vitest'
import {
  buildSnapshot,
  compareVersionsAsc,
  parseCategoricalKeys,
  promote,
  resolveEntry,
  snapshotsEqual,
  type DefaultsManifest,
  type DefaultsSnapshot,
  type SourceTree,
} from './gen-analytics-defaults-lib.ts'

const DEFINITIONS_DIR = 'apps/desktop/src/lib/settings/definitions'
const REGISTRY_FILE = 'apps/desktop/src/lib/settings/settings-registry.ts'
const CONFIG_SHAPE_FILE = 'apps/desktop/src-tauri/src/analytics/config_shape.rs'

/** A `config_shape.rs` carrying just the allowlist, which is all the parser reads from it. */
function configShapeRs(keys: string[]): string {
  return [
    '/// The categorical enum-string settings worth keeping.',
    'const CATEGORICAL_STRING_KEYS: &[&str] = &[',
    ...keys.map((key) => `    "${key}",`),
    '];',
    '',
    'pub fn build_config_shape() {}',
  ].join('\n')
}

/** A tree holding one definitions file plus an allowlist, the shape every current release has. */
function treeWith(definitions: string, categorical: string[] = []): SourceTree {
  const files: Record<string, string> = {
    [`${DEFINITIONS_DIR}/appearance.ts`]: definitions,
    [CONFIG_SHAPE_FILE]: configShapeRs(categorical),
  }
  return {
    list: (dir) => (dir === DEFINITIONS_DIR ? ['appearance.ts'] : []),
    read: (path) => files[path] ?? null,
  }
}

describe('buildSnapshot', () => {
  it('keeps the bool and number defaults, which always reach the config shape', () => {
    const { snapshot } = buildSnapshot(
      treeWith(`
        export const s = [
          { id: 'listing.showHiddenFiles', type: 'boolean', default: false },
          { id: 'appearance.textSize', type: 'number', default: 100 },
          { id: 'advanced.updateCheckInterval', type: 'duration', default: 3600000 },
        ]
      `),
    )
    expect(snapshot).toEqual({
      'advanced.updateCheckInterval': 3600000,
      'appearance.textSize': 100,
      'listing.showHiddenFiles': false,
    })
  })

  it('keeps a string default only when the Rust allowlist would ship it', () => {
    const definitions = `
      export const s = [
        { id: 'theme.mode', type: 'enum', default: 'system' },
        { id: 'appearance.customDateTimeFormat', type: 'string', default: 'YYYY-MM-DD' },
      ]
    `
    const { snapshot } = buildSnapshot(treeWith(definitions, ['theme.mode']))
    // The free-text one is dropped by `include_key` on the backend, so a default for it would
    // describe a key that can never be observed.
    expect(snapshot).toEqual({ 'theme.mode': 'system' })
  })

  it('drops arrays and objects, which the config shape never carries', () => {
    const { snapshot, unresolved } = buildSnapshot(
      treeWith(`
        export const s = [
          { id: 'indexing.includedPaths', type: 'string-array', default: [] },
          { id: 'ai.cloudProviderConfigs', type: 'string', default: {} },
        ]
      `),
    )
    expect(snapshot).toEqual({})
    expect(unresolved).toEqual([])
  })

  it('resolves `import.meta.env.DEV` to false, because the manifest describes shipped builds', () => {
    const { snapshot } = buildSnapshot(
      treeWith(`
        export const s = [{ id: 'advanced.logLlmCalls', type: 'boolean', default: import.meta.env.DEV }]
      `),
    )
    expect(snapshot).toEqual({ 'advanced.logLlmCalls': false })
  })

  it('reports a default it cannot read on a setting that would reach the config shape', () => {
    // Silently skipping would leave that key absent from every entry, and the dashboard would then
    // read "this setting did not exist" for a setting that very much does.
    const { snapshot, unresolved } = buildSnapshot(
      treeWith(`
        export const s = [{ id: 'advanced.mountTimeout', type: 'number', default: DEFAULT_TIMEOUT }]
      `),
    )
    expect(snapshot).toEqual({})
    expect(unresolved).toEqual([
      { id: 'advanced.mountTimeout', file: `${DEFINITIONS_DIR}/appearance.ts`, expression: 'DEFAULT_TIMEOUT' },
    ])
  })

  it('reads the older single-file registry layout', () => {
    // Releases before the definitions split still have to parse, or the backfill can't reach them.
    const tree: SourceTree = {
      list: () => [],
      read: (path) =>
        path === REGISTRY_FILE
          ? `const registry = [{ id: 'network.enabled', type: 'boolean', default: true }]`
          : path === CONFIG_SHAPE_FILE
            ? configShapeRs([])
            : null,
    }
    expect(buildSnapshot(tree).snapshot).toEqual({ 'network.enabled': true })
  })

  it('flags a revision whose heartbeat sent no config shape at all', () => {
    const tree: SourceTree = {
      list: () => [],
      read: (path) => (path === REGISTRY_FILE ? `const r = [{ id: 'a.b', type: 'boolean', default: true }]` : null),
    }
    const result = buildSnapshot(tree)
    expect(result.configShapeShipped).toBe(false)
    expect(result.snapshot).toEqual({})
  })

  it('ignores enum options, which carry a value but no default', () => {
    const { snapshot } = buildSnapshot(
      treeWith(
        `
        export const s = [{
          id: 'theme.mode',
          type: 'enum',
          default: 'system',
          constraints: { options: [{ value: 'light', labelKey: 'a' }, { value: 'dark', labelKey: 'b' }] },
        }]
      `,
        ['theme.mode'],
      ),
    )
    expect(snapshot).toEqual({ 'theme.mode': 'system' })
  })
})

describe('parseCategoricalKeys', () => {
  it('returns null when no allowlist is in the revision, so the caller can tell it apart from empty', () => {
    expect(parseCategoricalKeys({ list: () => [], read: () => null })).toBeNull()
  })

  it('reads the keys out of the Rust const', () => {
    const tree: SourceTree = { list: () => [], read: () => configShapeRs(['theme.mode', 'ai.provider']) }
    expect(parseCategoricalKeys(tree)).toEqual(new Set(['theme.mode', 'ai.provider']))
  })
})

describe('resolveEntry', () => {
  const versions: Record<string, DefaultsSnapshot> = {
    '0.25.0': { 'indexing.enabled': true },
    '0.34.0': { 'indexing.enabled': true, 'mediaIndex.enabled': false },
  }

  it('takes the newest entry at or below the version', () => {
    expect(resolveEntry(versions, '0.30.0')).toEqual(versions['0.25.0'])
    expect(resolveEntry(versions, '0.34.0')).toEqual(versions['0.34.0'])
    expect(resolveEntry(versions, '0.40.0')).toEqual(versions['0.34.0'])
  })

  it('answers null below the earliest entry, rather than guessing with the oldest one', () => {
    expect(resolveEntry(versions, '0.24.0')).toBeNull()
  })
})

describe('compareVersionsAsc', () => {
  it('orders numerically, not as text', () => {
    expect(['0.9.0', '0.40.0', '0.10.0'].sort(compareVersionsAsc)).toEqual(['0.9.0', '0.10.0', '0.40.0'])
  })
})

describe('promote', () => {
  const base: DefaultsManifest = { note: [], versions: { '0.39.0': { 'indexing.enabled': true } }, next: {} }

  it('writes no entry when the release changed no default, keeping the manifest sparse', () => {
    const promoted = promote(base, '0.40.0', { 'indexing.enabled': true })
    expect(Object.keys(promoted.versions)).toEqual(['0.39.0'])
    expect(promoted.next).toEqual({ 'indexing.enabled': true })
  })

  it('writes an entry when a default moved', () => {
    const promoted = promote(base, '0.40.0', { 'indexing.enabled': false })
    expect(promoted.versions['0.40.0']).toEqual({ 'indexing.enabled': false })
  })

  it('writes an entry when a release only ADDS a setting', () => {
    // The added key is the whole point: without an entry, 0.40.0 installs would resolve against
    // 0.39.0, where the key is absent, and the new setting would look like it never existed.
    const promoted = promote(base, '0.40.0', { 'indexing.enabled': true, 'askCmdr.proactive': true })
    expect(promoted.versions['0.40.0']).toEqual({ 'askCmdr.proactive': true, 'indexing.enabled': true })
  })

  it('is idempotent, so re-running a release is harmless', () => {
    const once = promote(base, '0.40.0', { 'indexing.enabled': false })
    const twice = promote(once, '0.40.0', { 'indexing.enabled': false })
    expect(twice.versions).toEqual(once.versions)
  })
})

describe('snapshotsEqual', () => {
  it('ignores key order', () => {
    expect(snapshotsEqual({ b: 1, a: 2 }, { a: 2, b: 1 })).toBe(true)
  })

  it('separates a missing key from a differing value', () => {
    expect(snapshotsEqual({ a: 1 }, { a: 1, b: 2 })).toBe(false)
    expect(snapshotsEqual({ a: 1 }, { a: 2 })).toBe(false)
  })
})
