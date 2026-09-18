import { describe, it, expect } from 'vitest'
import { effectiveScopePath, scopeSummaryFor, type ScopeSummaryInput } from './scope-summary'

function input(over: Partial<ScopeSummaryInput> = {}): ScopeSummaryInput {
  return { scope: '', defaultScopePath: '/Users/j/Downloads', defaultScopeLabel: 'Current folder', ...over }
}

describe('effectiveScopePath', () => {
  it('is the default path while the box is empty', () => {
    expect(effectiveScopePath(input())).toBe('/Users/j/Downloads')
    expect(effectiveScopePath(input({ scope: '   ' }))).toBe('/Users/j/Downloads')
  })

  it('is what the user typed once they type something', () => {
    expect(effectiveScopePath(input({ scope: '/Users/j/Projects' }))).toBe('/Users/j/Projects')
  })
})

describe('scopeSummaryFor', () => {
  it('names the folder rather than restating the setting', () => {
    // "Downloads" tells the reader what went wrong; "Current folder" only repeats the chip.
    expect(scopeSummaryFor(input())).toBe('Downloads')
  })

  it('shows a typed scope verbatim, list and exclusions included', () => {
    expect(scopeSummaryFor(input({ scope: '~/a, ~/b, !node_modules' }))).toBe('~/a, ~/b, !node_modules')
  })

  it('falls back to the label at a volume root, which has no last segment', () => {
    expect(scopeSummaryFor(input({ defaultScopePath: '/', defaultScopeLabel: 'This volume' }))).toBe('This volume')
  })

  it('falls back to the label for the bare home tilde, which would read as punctuation', () => {
    // A tab sitting in the home folder reports `~` as its path, so this is a real input.
    expect(scopeSummaryFor(input({ defaultScopePath: '~' }))).toBe('Current folder')
  })

  it('tolerates a trailing slash on a mount root', () => {
    expect(scopeSummaryFor(input({ defaultScopePath: '/Volumes/naspi/photos/' }))).toBe('photos')
  })

  it('names a folder inside a remote volume, but not the remote root itself', () => {
    expect(scopeSummaryFor(input({ defaultScopePath: 'smb://nas.local/share/photos' }))).toBe('photos')
    expect(scopeSummaryFor(input({ defaultScopePath: 'smb://nas.local', defaultScopeLabel: 'This volume' }))).toBe(
      'This volume',
    )
  })
})
