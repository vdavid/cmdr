/**
 * TDD for the message-key codegen (`gen-message-keys-lib.ts`): catalog → the
 * `MessageKey` union, plus the missing-key (build failure) and dead-key
 * (warning) reports.
 *
 * The codegen reads `messages/en/*.json` and scans source for `t()`/`<Trans>`/
 * `getMessage()` key usages. These tests drive both pure pieces (the catalog
 * parse + the source scan + the diff) against in-memory inputs, so no real
 * catalog or source tree is touched.
 */
import { describe, it, expect } from 'vitest'
import {
  collectCatalogKeys,
  stripMetadataKeys,
  extractUsedKeys,
  findCatalogKeyMentions,
  diffKeys,
  emitKeysModule,
} from './gen-message-keys-lib.ts'

describe('stripMetadataKeys', () => {
  it('keeps message keys and drops ARB-style @key metadata entries', () => {
    const raw = {
      'transfer.trash': 'Moved {countText} to trash',
      '@transfer.trash': { description: 'meta', screenshot: 'x.png' },
      'transfer.delete': 'Deleted',
    }
    expect(stripMetadataKeys(raw)).toEqual(['transfer.trash', 'transfer.delete'])
  })
})

describe('collectCatalogKeys', () => {
  it('merges keys across files, sorts, and dedupes', () => {
    const files = {
      'common.json': { 'common.cancel': 'Cancel', '@common.cancel': { description: 'm' } },
      'transfer.json': { 'transfer.trash': 'x', 'transfer.delete': 'y' },
    }
    expect(collectCatalogKeys(files)).toEqual(['common.cancel', 'transfer.delete', 'transfer.trash'])
  })
})

describe('extractUsedKeys', () => {
  it("finds t('key'), tString('key'), getMessage('key'), and <Trans key=\"...\">", () => {
    const src = [
      `const a = t('transfer.trash', { count: 1 })`,
      `const b = tString("transfer.delete")`,
      `const c = getMessage('common.cancel')`,
      `<Trans key="common.downloadsFdaHint" />`,
      "<Trans key='settings.title' params={p} />",
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(
      new Set(['transfer.trash', 'transfer.delete', 'common.cancel', 'common.downloadsFdaHint', 'settings.title']),
    )
  })

  it('finds keys passed indirectly via a `*Key`/`*Keys` property (the settings registry pattern)', () => {
    // The settings registry stores message KEYS in `labelKey`/`descriptionKey`
    // fields and resolves them through `t(variable)` (not a literal), so the
    // scan must recognize the key at its literal definition site.
    const src = [
      `labelKey: 'settings.theme.mode.label',`,
      `descriptionKey: "settings.theme.mode.description",`,
      `{ value: 'light', labelKey: 'settings.theme.mode.opt.light' },`,
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(
      new Set(['settings.theme.mode.label', 'settings.theme.mode.description', 'settings.theme.mode.opt.light']),
    )
  })

  it('ignores non-message calls and dynamic (non-literal) keys', () => {
    const src = [
      `translate(key)`, // dynamic
      `t(dynamicKey)`, // dynamic, not extractable
      `something('not.a.key')`, // not a message accessor
      `t(\`transfer.\${x}\`)`, // template with expression, skipped
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(new Set())
  })
})

describe('findCatalogKeyMentions', () => {
  it('finds catalog keys mentioned via a Record map (dead-key suppression of indirection)', () => {
    const src = `const M = { Appearance: 'settings.section.appearance', red: 'settings.tint.red' }`
    const mentioned = findCatalogKeyMentions(src, [
      'settings.section.appearance',
      'settings.tint.red',
      'settings.tint.blue',
    ])
    expect(mentioned.has('settings.section.appearance')).toBe(true)
    expect(mentioned.has('settings.tint.red')).toBe(true)
    expect(mentioned.has('settings.tint.blue')).toBe(false) // not mentioned
  })

  it('is robust to apostrophes in nearby backtick strings (substring scan, not quote parsing)', () => {
    const src = "const e = `Invalid '${x}'`; const M = { red: 'settings.tint.red' }"
    expect(findCatalogKeyMentions(src, ['settings.tint.red']).has('settings.tint.red')).toBe(true)
  })
})

describe('diffKeys', () => {
  it('suppresses the dead warning for a catalog key reached only via a literal (indirection)', () => {
    // The key is never directly referenced (no t('x')), but its literal appears
    // in a Record map, so it must NOT be reported dead.
    const result = diffKeys({
      catalogKeys: ['settings.section.appearance'],
      usedKeys: new Set(),
      literalKeys: new Set(['settings.section.appearance']),
    })
    expect(result.dead).toEqual([])
    expect(result.missing).toEqual([])
  })

  it('a literal that is not a direct reference never becomes a missing key', () => {
    // `literalKeys` must not feed missing detection: a non-catalog dotted
    // literal (e.g. a setting id) stays out of `missing`.
    const result = diffKeys({
      catalogKeys: ['settings.theme.mode.label'],
      usedKeys: new Set(['settings.theme.mode.label']),
      literalKeys: new Set(['developer.mcpPort', 'settings.theme.mode.label']),
    })
    expect(result.missing).toEqual([])
    expect(result.dead).toEqual([])
  })

  it('reports keys used in code but missing from the catalog', () => {
    const result = diffKeys({
      catalogKeys: ['transfer.trash'],
      usedKeys: new Set(['transfer.trash', 'transfer.missing']),
    })
    expect(result.missing).toEqual(['transfer.missing'])
    expect(result.dead).toEqual([])
  })

  it('reports catalog keys never used in code (dead)', () => {
    const result = diffKeys({
      catalogKeys: ['transfer.trash', 'transfer.unused'],
      usedKeys: new Set(['transfer.trash']),
    })
    expect(result.missing).toEqual([])
    expect(result.dead).toEqual(['transfer.unused'])
  })

  it('returns both lists sorted and empty when in sync', () => {
    const result = diffKeys({
      catalogKeys: ['a.one', 'a.two'],
      usedKeys: new Set(['a.two', 'a.one']),
    })
    expect(result.missing).toEqual([])
    expect(result.dead).toEqual([])
  })
})

describe('emitKeysModule', () => {
  it('emits a string-literal union (in the order given) with the do-not-edit header', () => {
    // The caller (collectCatalogKeys) sorts; emit reflects the given order.
    const out = emitKeysModule(['common.cancel', 'transfer.trash'])
    expect(out).toContain('AUTO-GENERATED')
    expect(out).toContain('Do not edit by hand')
    expect(out).toContain('export type MessageKey =')
    expect(out).toContain("  | 'common.cancel'")
    expect(out).toContain("  | 'transfer.trash'")
    // Sorted: common before transfer.
    expect(out.indexOf("'common.cancel'")).toBeLessThan(out.indexOf("'transfer.trash'"))
  })

  it('emits `never` for an empty catalog', () => {
    expect(emitKeysModule([])).toContain('  never')
  })
})
