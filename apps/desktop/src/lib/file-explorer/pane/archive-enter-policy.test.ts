import { describe, expect, it } from 'vitest'
import {
  ARCHIVE_ENTER_FORMATS,
  parseEnterBehaviorOverrides,
  resolveEnterPolicy,
  type EnterBehaviorOverrides,
  type EnterCandidate,
} from './archive-enter-policy'

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
})

describe('resolveEnterPolicy overrides', () => {
  it('applies a per-format override over the default', () => {
    const overrides: EnterBehaviorOverrides = { zip: 'browse', bundle: 'open' }
    expect(resolveEnterPolicy(file('foo.zip', true), overrides)).toBe('browse')
    expect(resolveEnterPolicy(dir('Safari.app'), overrides)).toBe('open')
  })

  it('falls back to the format default when no override is set for that format', () => {
    const overrides: EnterBehaviorOverrides = { zip: 'browse' }
    expect(resolveEnterPolicy(dir('Safari.app'), overrides)).toBe('ask')
  })
})

describe('parseEnterBehaviorOverrides', () => {
  it('parses a stored JSON object, keeping only known formats and actions', () => {
    const parsed = parseEnterBehaviorOverrides('{"zip":"browse","bundle":"open"}')
    expect(parsed).toEqual({ zip: 'browse', bundle: 'open' })
  })

  it('drops unknown format keys and invalid actions', () => {
    const parsed = parseEnterBehaviorOverrides('{"zip":"nope","rar":"browse","bundle":"ask"}')
    expect(parsed).toEqual({ bundle: 'ask' })
  })

  it('returns an empty object for malformed or empty input', () => {
    expect(parseEnterBehaviorOverrides('')).toEqual({})
    expect(parseEnterBehaviorOverrides('not json')).toEqual({})
    expect(parseEnterBehaviorOverrides('[]')).toEqual({})
    expect(parseEnterBehaviorOverrides('null')).toEqual({})
  })
})

describe('ARCHIVE_ENTER_FORMATS registry', () => {
  it('exposes the configurable formats with their defaults', () => {
    const byKey = Object.fromEntries(ARCHIVE_ENTER_FORMATS.map((f) => [f.key, f]))
    expect(byKey.zip.defaultAction).toBe('ask')
    expect(byKey.zip.configurable).toBe(true)
    expect(byKey.bundle.defaultAction).toBe('ask')
    expect(byKey.bundle.configurable).toBe(true)
    // OOXML/app packages resolve to open but aren't user-configurable yet
    // (browse-into isn't supported for them in this phase).
    expect(byKey.ooxml.defaultAction).toBe('open')
    expect(byKey.ooxml.configurable).toBe(false)
  })
})
