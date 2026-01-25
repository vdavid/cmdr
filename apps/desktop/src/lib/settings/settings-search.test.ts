import { describe, it, expect, beforeEach } from 'vitest'
import {
    searchSettings,
    searchAdvancedSettings,
    getMatchingSections,
    sectionHasMatches,
    highlightMatches,
    clearSearchIndex,
} from './settings-search'

describe('searchSettings', () => {
    beforeEach(() => {
        clearSearchIndex()
    })

    it('should return all settings when query is empty', () => {
        const results = searchSettings('')
        expect(results.length).toBeGreaterThan(0)
    })

    it('should return all settings when query is whitespace', () => {
        const results = searchSettings('   ')
        expect(results.length).toBeGreaterThan(0)
    })

    it('should find settings by label', () => {
        const results = searchSettings('density')
        expect(results.length).toBeGreaterThan(0)
        expect(results.some((r) => r.setting.id === 'appearance.uiDensity')).toBe(true)
    })

    it('should find settings by section name', () => {
        const results = searchSettings('general')
        expect(results.length).toBeGreaterThan(0)
        // At least one result should be in the General section
        const hasGeneral = results.some((r) => r.setting.section[0] === 'General')
        expect(hasGeneral).toBe(true)
    })

    it('should return empty array when nothing matches', () => {
        const results = searchSettings('xyznonexistent123')
        expect(results).toEqual([])
    })

    it('should include matched indices for highlighting', () => {
        const results = searchSettings('density')
        expect(results.length).toBeGreaterThan(0)
        // Matched indices should be numbers
        for (const result of results) {
            expect(Array.isArray(result.matchedIndices)).toBe(true)
        }
    })
})

describe('searchAdvancedSettings', () => {
    it('should return all advanced settings when query is empty', () => {
        const results = searchAdvancedSettings('')
        expect(results.length).toBeGreaterThan(0)
        for (const result of results) {
            expect(result.setting.showInAdvanced).toBe(true)
        }
    })

    it('should find advanced settings by label', () => {
        const results = searchAdvancedSettings('drag')
        // Should find dragThreshold
        const hasDragThreshold = results.some((r) => r.setting.id.includes('dragThreshold'))
        expect(hasDragThreshold).toBe(true)
    })
})

describe('getMatchingSections', () => {
    it('should return sections containing matching settings', () => {
        const sections = getMatchingSections('density')
        expect(sections.size).toBeGreaterThan(0)
        // Should include the parent section path 'General' or 'General/Appearance'
        const hasGeneral = sections.has('General') || sections.has('General/Appearance')
        expect(hasGeneral).toBe(true)
    })

    it('should return empty set when nothing matches', () => {
        const sections = getMatchingSections('xyznonexistent123')
        expect(sections.size).toBe(0)
    })
})

describe('sectionHasMatches', () => {
    it('should return true for sections with matching settings', () => {
        const matchingSections = getMatchingSections('density')
        // The function uses path.join('/') to check
        expect(sectionHasMatches(['General'], matchingSections)).toBe(true)
    })

    it('should return false for sections without matches', () => {
        const matchingSections = getMatchingSections('density')
        expect(sectionHasMatches(['NonExistent'], matchingSections)).toBe(false)
    })
})

describe('highlightMatches', () => {
    it('should return single segment when no matches', () => {
        const segments = highlightMatches('hello world', [])
        expect(segments).toEqual([{ text: 'hello world', matched: false }])
    })

    it('should highlight matched characters', () => {
        const segments = highlightMatches('hello', [0, 1])
        expect(segments).toEqual([
            { text: 'he', matched: true },
            { text: 'llo', matched: false },
        ])
    })

    it('should handle non-contiguous matches', () => {
        const segments = highlightMatches('abcde', [0, 2, 4])
        expect(segments.length).toBeGreaterThan(1)
        // Check that matched characters are marked
        expect(segments.some((s) => s.matched && s.text === 'a')).toBe(true)
    })
})
