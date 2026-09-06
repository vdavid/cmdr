import { describe, expect, it } from 'vitest'
import {
  ARCHIVE_ENTER_FORMATS,
  enterBehaviorFromSettings,
  resolveEnterPolicy,
  type ArchiveEnterSettingId,
  type EnterAction,
  type EnterBehaviorByFormat,
  type EnterCandidate,
} from './archive-enter-policy'
import { getSettingDefinition, settingsRegistry } from '$lib/settings/settings-registry'

/** A plain file entry with the fields the resolver reads. */
function file(name: string, isArchive = false): EnterCandidate {
  return { name, isDirectory: false, isArchive }
}

/** A directory entry (bundles are directories). */
function dir(name: string): EnterCandidate {
  return { name, isDirectory: true, isArchive: false }
}

describe('resolveEnterPolicy classification', () => {
  it('classifies a zip file as the zip format (default ask)', () => {
    expect(resolveEnterPolicy(file('foo.zip', true), {})).toBe('ask')
  })

  it('classifies OOXML and app packages as documents (default open)', () => {
    for (const name of ['report.docx', 'sheet.xlsx', 'deck.pptx', 'lib.jar', 'app.apk']) {
      expect(resolveEnterPolicy(file(name), {})).toBe('open')
    }
  })

  it('classifies macOS bundle directories (default ask)', () => {
    for (const name of ['Safari.app', 'Some.bundle', 'Foundation.framework']) {
      expect(resolveEnterPolicy(dir(name), {})).toBe('ask')
    }
  })

  it('returns null for entries that are neither archives, documents, nor bundles', () => {
    expect(resolveEnterPolicy(file('notes.txt'), {})).toBeNull()
    expect(resolveEnterPolicy(dir('Documents'), {})).toBeNull()
  })

  it('does not treat a directory named like an archive as an archive', () => {
    // `is_archive` is never set on a directory backend-side; a folder literally
    // named `foo.zip` is browsed as itself, so the resolver must not prompt.
    expect(resolveEnterPolicy(dir('foo.zip'), {})).toBeNull()
  })

  it('does not treat a bundle extension on a regular file as a bundle', () => {
    // `.app`/`.framework` are bundle markers only on directories.
    expect(resolveEnterPolicy(file('weird.app'), {})).toBeNull()
  })

  it('is case-insensitive on the extension', () => {
    expect(resolveEnterPolicy(file('FOO.ZIP', true), {})).toBe('ask')
    expect(resolveEnterPolicy(dir('Safari.APP'), {})).toBe('ask')
  })

  it('keeps an OOXML file on the ooxml format even once the backend flags it as an archive', () => {
    // ❗ Order guard, not a style preference. An OOXML file IS a zip, so the day the
    // backend's archive suffix table learns to browse into `.docx`, `isArchive` goes
    // true on one — and the zip matcher is exactly `isArchive === true`. Classification
    // is first-match-wins, so a registry ordered zip-before-ooxml would swallow every
    // Office document into the zip row and the Office documents setting would silently
    // stop doing anything. Specific formats MUST precede general ones.
    const flaggedDocx = file('report.docx', true)
    expect(resolveEnterPolicy(flaggedDocx, {})).toBe('open')
    expect(resolveEnterPolicy(flaggedDocx, { zip: 'browse', ooxml: 'ask' })).toBe('ask')
  })
})

describe('resolveEnterPolicy per-format actions', () => {
  it('applies the action chosen for that format', () => {
    const behavior: EnterBehaviorByFormat = { zip: 'browse', bundle: 'open' }
    expect(resolveEnterPolicy(file('foo.zip', true), behavior)).toBe('browse')
    expect(resolveEnterPolicy(dir('Safari.app'), behavior)).toBe('open')
  })

  it('falls back to the format default when the caller supplied no action for it', () => {
    const behavior: EnterBehaviorByFormat = { zip: 'browse' }
    expect(resolveEnterPolicy(dir('Safari.app'), behavior)).toBe('ask')
  })
})

describe('enterBehaviorFromSettings', () => {
  it('reads one setting per format, so a new format needs no change here', () => {
    const stored: Record<string, EnterAction> = {
      'behavior.archiveEnter.zip': 'browse',
      'behavior.archiveEnter.ooxml': 'ask',
      'behavior.archiveEnter.bundle': 'open',
    }
    const read = (id: ArchiveEnterSettingId): EnterAction => stored[id]

    const behavior = enterBehaviorFromSettings(read)

    expect(behavior).toEqual({ zip: 'browse', ooxml: 'ask', bundle: 'open' })
    // Every format is covered, so the resolver never falls back to a default here.
    expect(Object.keys(behavior).sort()).toEqual(ARCHIVE_ENTER_FORMATS.map((f) => f.key).sort())
  })
})

describe('ARCHIVE_ENTER_FORMATS registry', () => {
  it('exposes each format with its default', () => {
    const byKey = Object.fromEntries(ARCHIVE_ENTER_FORMATS.map((f) => [f.key, f]))
    expect(byKey.zip.defaultAction).toBe('ask')
    expect(byKey.bundle.defaultAction).toBe('ask')
    // Office documents and app packages open in their app rather than browsing.
    expect(byKey.ooxml.defaultAction).toBe('open')
  })
})

/**
 * The seam this whole module rests on: a format is user-configurable because it names
 * a settings id, and nothing else. Both directions matter, so both are asserted —
 * a format with no setting is a row a user can never change, and a
 * `behavior.archiveEnter.*` setting with no format is a control that writes a value
 * nothing reads. Either one is silent in production and instant here.
 */
describe('ARCHIVE_ENTER_FORMATS ↔ settings registry parity', () => {
  const ARCHIVE_ENTER_PREFIX = 'behavior.archiveEnter.'

  it('every format names a registry setting whose default matches the format default', () => {
    for (const format of ARCHIVE_ENTER_FORMATS) {
      const definition = getSettingDefinition(format.settingId)
      expect(definition, `no registry entry for ${format.settingId}`).toBeDefined()
      // The registry default IS the format default: `resolveEnterPolicy` falls back to
      // the format's, `getSetting` falls back to the registry's, and a user who never
      // touched the row must get the same answer down either path.
      expect(definition?.default, `${format.settingId} default`).toBe(format.defaultAction)
      // Every action the resolver understands has to be offerable, or a row would be
      // missing a choice the policy can still resolve to.
      const values = definition?.constraints?.options?.map((o) => o.value)
      expect(values, `options of ${format.settingId}`).toEqual(['browse', 'open', 'ask'])
    }
  })

  it('every archive-enter setting in the registry belongs to a format', () => {
    const declared = new Set<string>(ARCHIVE_ENTER_FORMATS.map((f) => f.settingId))
    const registered = settingsRegistry.map((d) => d.id).filter((id) => id.startsWith(ARCHIVE_ENTER_PREFIX))

    expect(registered.length).toBeGreaterThan(0) // guard against the prefix silently changing
    expect([...registered].sort()).toEqual([...declared].sort())
  })
})
