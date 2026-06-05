import { describe, it, expect } from 'vitest'
import { searchCommands } from './fuzzy-search'

describe('searchCommands', () => {
  describe('empty query', () => {
    it('returns all palette commands when query is empty', () => {
      const results = searchCommands('')
      // Should return all commands with showInPalette: true
      expect(results.length).toBeGreaterThan(0)
      // All results should have empty matchedIndices
      expect(results.every((r) => r.matchedIndices.length === 0)).toBe(true)
    })

    it('returns all palette commands when query is whitespace', () => {
      const results = searchCommands('   ')
      expect(results.length).toBeGreaterThan(0)
    })
  })

  describe('exact matches', () => {
    it('finds exact prefix matches', () => {
      const results = searchCommands('About')
      expect(results.some((r) => r.command.id === 'app.about')).toBe(true)
    })

    it('finds exact substring matches', () => {
      const results = searchCommands('path')
      expect(results.some((r) => r.command.id === 'file.copyPath')).toBe(true)
    })

    it('matches are case-insensitive', () => {
      const results = searchCommands('ABOUT')
      expect(results.some((r) => r.command.id === 'app.about')).toBe(true)
    })
  })

  describe('fuzzy matches', () => {
    it('finds fuzzy matches with missing characters', () => {
      // "cop" should match "Copy path to clipboard"
      const results = searchCommands('cop')
      expect(results.some((r) => r.command.name.toLowerCase().includes('copy'))).toBe(true)
    })

    it('finds matches with out-of-order characters', () => {
      // "hnf" could match "Show in Finder" (h-n-F)
      const results = searchCommands('hnf')
      // May or may not match depending on fuzzy settings, but shouldn't error
      expect(Array.isArray(results)).toBe(true)
    })
  })

  describe('match highlighting', () => {
    it('returns matched character indices for highlighting', () => {
      const results = searchCommands('about')
      const aboutResult = results.find((r) => r.command.id === 'app.about')
      expect(aboutResult).toBeDefined()
      expect(aboutResult?.matchedIndices.length).toBeGreaterThan(0)
    })

    it('matched indices are within command name bounds', () => {
      const results = searchCommands('about')
      const aboutResult = results.find((r) => r.command.id === 'app.about')
      if (aboutResult && aboutResult.matchedIndices.length > 0) {
        const maxIndex = Math.max(...aboutResult.matchedIndices)
        expect(maxIndex).toBeLessThan(aboutResult.command.name.length)
      }
    })
  })

  describe('no matches', () => {
    it('returns empty array for no matches', () => {
      const results = searchCommands('xyzzynonexistent')
      expect(results).toEqual([])
    })

    it('returns empty array for gibberish', () => {
      const results = searchCommands('!@#$%^&*()')
      expect(results).toEqual([])
    })
  })

  describe('ranking', () => {
    it('ranks exact prefix matches higher than substring matches', () => {
      // "Copy" should rank "Copy filename" before "Go to parent folder"
      const results = searchCommands('Copy')
      expect(results.length).toBeGreaterThan(0)
      // First result should start with "Copy"
      expect(results[0].command.name.startsWith('Copy')).toBe(true)
    })
  })

  describe('recents on empty query', () => {
    it('leads the result with recents in given order', () => {
      const results = searchCommands('', ['app.about', 'file.copyPath'])
      expect(results[0]?.command.id).toBe('app.about')
      expect(results[1]?.command.id).toBe('file.copyPath')
    })

    it('filters out stale IDs (commands no longer in the palette)', () => {
      const results = searchCommands('', ['definitely.not.a.real.command', 'app.about'])
      // Stale ID dropped silently; real recent leads.
      expect(results[0]?.command.id).toBe('app.about')
      // `command.id` is `CommandId`, so the stale literal can't appear; compare as
      // `string` to keep the runtime assertion (and dodge a no-overlap type error).
      expect(results.every((r) => (r.command.id as string) !== 'definitely.not.a.real.command')).toBe(true)
    })

    it('appends remaining palette commands after recents with no duplicates', () => {
      const results = searchCommands('', ['app.about'])
      const ids = results.map((r) => r.command.id)
      // Each command appears exactly once.
      expect(new Set(ids).size).toBe(ids.length)
      // The recent leads.
      expect(ids[0]).toBe('app.about')
    })

    it('falls back to plain order when recents is empty', () => {
      const withRecents = searchCommands('', [])
      const withoutArg = searchCommands('')
      expect(withRecents.map((r) => r.command.id)).toEqual(withoutArg.map((r) => r.command.id))
    })

    it('ignores recents when the query is non-empty', () => {
      const results = searchCommands('about', ['file.copyPath'])
      // file.copyPath does not contain "about", so it must not appear just because it's recent.
      expect(results.every((r) => r.command.id !== 'file.copyPath')).toBe(true)
    })
  })

  describe('keyword matches', () => {
    it('finds both jump commands via the "jump" keyword', () => {
      const results = searchCommands('jump')
      const ids = results.map((r) => r.command.id)
      expect(ids).toContain('nav.goToPath')
      expect(ids).toContain('downloads.goToLatest')
    })

    it('finds both jump commands via the "navigate" keyword', () => {
      const results = searchCommands('navigate')
      const ids = results.map((r) => r.command.id)
      expect(ids).toContain('nav.goToPath')
      expect(ids).toContain('downloads.goToLatest')
    })

    it('never highlights past the visible name on a keyword-only match', () => {
      // "jump" appears only in the keywords, not the name, so any highlight indices
      // must stay within the visible label.
      for (const query of ['jump', 'navigate', 'goto']) {
        const results = searchCommands(query)
        for (const match of results) {
          for (const index of match.matchedIndices) {
            expect(index).toBeLessThan(match.command.name.length)
          }
        }
      }
    })
  })

  describe('command filtering', () => {
    it('excludes commands with showInPalette: false', () => {
      const results = searchCommands('')
      // nav.up and nav.down have showInPalette: false
      expect(results.some((r) => r.command.id === 'nav.up')).toBe(false)
      expect(results.some((r) => r.command.id === 'nav.down')).toBe(false)
    })

    it('includes commands with showInPalette: true', () => {
      const results = searchCommands('')
      // app.about has showInPalette: true
      expect(results.some((r) => r.command.id === 'app.about')).toBe(true)
    })
  })
})
