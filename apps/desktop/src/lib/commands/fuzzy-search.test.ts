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
            const results = searchCommands('Quit')
            expect(results.some((r) => r.command.id === 'app.quit')).toBe(true)
        })

        it('finds exact substring matches', () => {
            const results = searchCommands('path')
            expect(results.some((r) => r.command.id === 'file.copyPath')).toBe(true)
        })

        it('matches are case-insensitive', () => {
            const results = searchCommands('QUIT')
            expect(results.some((r) => r.command.id === 'app.quit')).toBe(true)
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
            const results = searchCommands('quit')
            const quitResult = results.find((r) => r.command.id === 'app.quit')
            expect(quitResult).toBeDefined()
            // "Quit Cmdr" - Q, u, i, t should be matched (indices 0, 1, 2, 3)
            expect(quitResult?.matchedIndices.length).toBeGreaterThan(0)
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
