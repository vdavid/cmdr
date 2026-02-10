import { describe, it, expect } from 'vitest'
import { truncateMiddle, formatFileCount, buildDisplayLines, getEntryEmoji } from './drag-image-renderer'

describe('getEntryEmoji', () => {
    it('returns folder emoji for directories', () => {
        expect(getEntryEmoji('Documents', true)).toBe('\uD83D\uDCC1 ')
    })

    it('returns file emoji for files', () => {
        expect(getEntryEmoji('readme.txt', false)).toBe('\uD83D\uDCC4 ')
    })
})

describe('truncateMiddle', () => {
    it('returns the name unchanged if shorter than max', () => {
        expect(truncateMiddle('short.txt', 36)).toBe('short.txt')
    })

    it('returns the name unchanged if exactly max length', () => {
        const name = 'a'.repeat(30) + '.json'
        expect(truncateMiddle(name, 35)).toBe(name)
    })

    it('truncates middle of long name preserving extension', () => {
        const name = 'this-is-a-very-long-filename-that-exceeds.txt'
        const result = truncateMiddle(name, 20)
        expect(result).toContain('\u2026')
        expect(result).toMatch(/\.txt$/)
        expect(result.length).toBeLessThanOrEqual(20)
    })

    it('handles names without extension', () => {
        const name = 'a-very-long-name-without-extension'
        const result = truncateMiddle(name, 15)
        expect(result).toContain('\u2026')
        expect(result.length).toBeLessThanOrEqual(15)
    })

    it('handles names with dot at start (hidden files)', () => {
        const name = '.very-long-hidden-configuration-file'
        const result = truncateMiddle(name, 15)
        expect(result.length).toBeLessThanOrEqual(15)
    })

    it('handles very short max length gracefully', () => {
        const name = 'longname.verylongext'
        const result = truncateMiddle(name, 5)
        expect(result.length).toBeLessThanOrEqual(5)
        expect(result).toContain('\u2026')
    })
})

describe('formatFileCount', () => {
    it('formats singular file count', () => {
        expect(formatFileCount(1)).toBe('1 file')
    })

    it('formats plural file count', () => {
        expect(formatFileCount(3)).toBe('3 files')
    })

    it('formats zero files', () => {
        expect(formatFileCount(0)).toBe('0 files')
    })

    it('formats large count', () => {
        expect(formatFileCount(1234)).toBe('1234 files')
    })
})

describe('buildDisplayLines', () => {
    it('shows all names when 12 or fewer', () => {
        const names = ['a.txt', 'b.txt', 'c.txt']
        const flags = [false, false, false]
        const lines = buildDisplayLines(names, flags)

        // 3 name lines + 1 count line
        expect(lines).toHaveLength(4)
        expect(lines[0].text).toContain('a.txt')
        expect(lines[0].isMuted).toBe(false)
        expect(lines[3].text).toBe('3 files')
        expect(lines[3].isMuted).toBe(true)
    })

    it('shows all 12 names when exactly 12', () => {
        const names = Array.from({ length: 12 }, (_, i) => `file${String(i)}.txt`)
        const flags = names.map(() => false)
        const lines = buildDisplayLines(names, flags)

        // 12 name lines + 1 count line
        expect(lines).toHaveLength(13)
        expect(lines[11].text).toContain('file11.txt')
        expect(lines[12].text).toBe('12 files')
    })

    it('truncates to first 8 + "and N more" for more than 12', () => {
        const names = Array.from({ length: 20 }, (_, i) => `file${String(i)}.txt`)
        const flags = names.map(() => false)
        const lines = buildDisplayLines(names, flags)

        // 8 name lines + "and 12 more" + count line
        expect(lines).toHaveLength(10)
        expect(lines[7].text).toContain('file7.txt')
        expect(lines[8].text).toBe('and 12 more')
        expect(lines[8].isMuted).toBe(true)
        expect(lines[9].text).toBe('20 files')
    })

    it('uses folder emoji for directories', () => {
        const names = ['Documents', 'readme.txt']
        const flags = [true, false]
        const lines = buildDisplayLines(names, flags)

        expect(lines[0].text).toMatch(/^\uD83D\uDCC1/)
        expect(lines[1].text).toMatch(/^\uD83D\uDCC4/)
    })

    it('handles single file', () => {
        const lines = buildDisplayLines(['hello.txt'], [false])

        expect(lines).toHaveLength(2)
        expect(lines[0].text).toContain('hello.txt')
        expect(lines[1].text).toBe('1 file')
    })
})
