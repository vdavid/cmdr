import { describe, expect, it } from 'vitest'
import { resolveSettingsSection } from './settings-section-lookup'

describe('resolveSettingsSection', () => {
  it('finds a top-level section and a subsection by their sidebar names', () => {
    expect(resolveSettingsSection('Appearance')).toEqual(['Appearance'])
    expect(resolveSettingsSection('Ask Cmdr')).toEqual(['AI', 'Ask Cmdr'])
  })

  it('finds a page the registry does not drive', () => {
    expect(resolveSettingsSection('Keyboard shortcuts')).toEqual(['Keyboard shortcuts'])
    expect(resolveSettingsSection('License')).toEqual(['License'])
  })

  it('ignores case, since a model rarely copies it', () => {
    expect(resolveSettingsSection('ask cmdr')).toEqual(['AI', 'Ask Cmdr'])
  })

  it('takes part of a name when only one section has it', () => {
    expect(resolveSettingsSection('shortcuts')).toEqual(['Keyboard shortcuts'])
  })

  it('answers undefined for a name no section has, which used to open a blank page', () => {
    expect(resolveSettingsSection('file operations')).toBeUndefined()
    expect(resolveSettingsSection('')).toBeUndefined()
  })

  it('answers undefined when part of a name fits several sections, rather than picking one', () => {
    expect(resolveSettingsSection('e')).toBeUndefined()
  })
})
