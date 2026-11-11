import { IntlMessageFormat } from 'intl-messageformat'
import { describe, expect, it } from 'vitest'
import { parsePlatformSelect, pickPlatformBranch } from './platform-select'

// allowed-pluralize-noun: `} windows` is a platform block's branch keyword, not a count and a noun
const FULL = '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Show in file manager}}'

describe('parsePlatformSelect', () => {
  it('splits a block into its branches', () => {
    expect(parsePlatformSelect(FULL)).toEqual({
      macos: 'Show in Finder',
      windows: 'Show in File Explorer',
      other: 'Show in file manager',
    })
  })

  it('answers null for a plain value', () => {
    expect(parsePlatformSelect('Show in Finder')).toBeNull()
    expect(parsePlatformSelect('Moved {count, plural, one {# file} other {# files}}')).toBeNull()
    expect(parsePlatformSelect('')).toBeNull()
  })

  it('answers null for a block inside a sentence: the block is the whole value or nothing', () => {
    expect(parsePlatformSelect('Open {platform, select, macos {Finder} other {files}}')).toBeNull()
    expect(parsePlatformSelect('{platform, select, macos {Finder} other {files}} now')).toBeNull()
  })

  it('leaves out a branch the block doesn’t have', () => {
    expect(parsePlatformSelect('{platform, select, windows {Recycle Bin} other {Trash}}')).toEqual({
      windows: 'Recycle Bin',
      other: 'Trash',
    })
  })

  it('keeps a nested ICU plural in a branch whole', () => {
    const value =
      '{platform, select, macos {Moved {count, plural, one {# file} other {# files}} to the Trash} other {Deleted {count, plural, one {# file} other {# files}}}}'
    expect(parsePlatformSelect(value)).toEqual({
      macos: 'Moved {count, plural, one {# file} other {# files}} to the Trash',
      other: 'Deleted {count, plural, one {# file} other {# files}}',
    })
  })

  it('keeps raw tokens and markup in a branch as text', () => {
    const value =
      '{platform, select, macos {Open {system_settings} > <b>Privacy</b>} other {Check your system’s settings}}'
    expect(parsePlatformSelect(value)?.macos).toBe('Open {system_settings} > <b>Privacy</b>')
  })

  it('reads ICU apostrophe quoting, so a quoted brace doesn’t end a branch', () => {
    const value = "{platform, select, macos {Type '{' to start} other {Type '}' or '<b>{x}</b>' to end}}"
    expect(parsePlatformSelect(value)).toEqual({
      macos: "Type '{' to start",
      other: "Type '}' or '<b>{x}</b>' to end",
    })
    // ICU reads the same branches, so the splitter and the engine agree.
    expect(new IntlMessageFormat(value, 'en').format({ platform: 'macos' })).toBe('Type { to start')
    expect(new IntlMessageFormat(value, 'en').format({ platform: 'linux' })).toBe('Type } or <b>{x}</b> to end')
  })

  it('reads a doubled apostrophe as a literal one, which doesn’t start a quote', () => {
    expect(parsePlatformSelect("{platform, select, macos {It''s {name}} other {It's {name}}}")).toEqual({
      macos: "It''s {name}",
      other: "It's {name}",
    })
  })

  it('rejects a block whose braces don’t balance', () => {
    expect(parsePlatformSelect('{platform, select, macos {Open {system_settings} other {Settings}}')).toBeNull()
    expect(parsePlatformSelect('{platform, select, macos {Finder}} other {Files}}')).toBeNull()
    expect(parsePlatformSelect('{platform, select, macos {Finder} other {Files}')).toBeNull()
  })

  it('rejects a block without `other`, with an unknown branch, or with a branch twice', () => {
    // allowed-pluralize-noun: `} windows` is a platform block's branch keyword, not a count and a noun
    expect(parsePlatformSelect('{platform, select, macos {Finder} windows {File Explorer}}')).toBeNull()
    expect(parsePlatformSelect('{platform, select, macos {Finder} linux {Files} other {Files}}')).toBeNull()
    // allowed-pluralize-noun: `} macos` is a platform block's branch keyword, not a count and a noun
    expect(parsePlatformSelect('{platform, select, macos {Finder} macos {Finder} other {Files}}')).toBeNull()
  })
})

describe('pickPlatformBranch', () => {
  it('picks each platform’s branch', () => {
    expect(pickPlatformBranch(FULL, 'macos')).toBe('Show in Finder')
    expect(pickPlatformBranch(FULL, 'windows')).toBe('Show in File Explorer')
  })

  it('gives Linux the `other` branch', () => {
    expect(pickPlatformBranch(FULL, 'linux')).toBe('Show in file manager')
  })

  it('falls back to `other` for a platform the block leaves out', () => {
    const value = '{platform, select, macos {the Trash} other {the trash}}'
    expect(pickPlatformBranch(value, 'windows')).toBe('the trash')
    expect(pickPlatformBranch('{platform, select, windows {Recycle Bin} other {Trash}}', 'macos')).toBe('Trash')
  })

  it('returns a plain value unchanged', () => {
    expect(pickPlatformBranch('Open {system_settings}', 'windows')).toBe('Open {system_settings}')
  })
})
