import { describe, expect, it } from 'vitest'
import { pluralize } from './pluralize'

describe('pluralize', () => {
  it('returns singular when count is 1', () => {
    expect(pluralize(1, 'file')).toBe('file')
    expect(pluralize(1, 'entry', 'entries')).toBe('entry')
  })

  it('defaults to appending "s" for the plural', () => {
    expect(pluralize(0, 'file')).toBe('files')
    expect(pluralize(2, 'byte')).toBe('bytes')
    expect(pluralize(42, 'volume')).toBe('volumes')
  })

  it('uses the explicit plural form when provided', () => {
    expect(pluralize(0, 'entry', 'entries')).toBe('entries')
    expect(pluralize(12, 'branch', 'branches')).toBe('branches')
    expect(pluralize(5, 'directory', 'directories')).toBe('directories')
  })

  it('treats negative counts as plural', () => {
    expect(pluralize(-1, 'file')).toBe('files')
  })
})
