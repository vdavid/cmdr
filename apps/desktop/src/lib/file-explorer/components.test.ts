/**
 * Test helpers and pure function tests for file explorer components.
 *
 * Note: Component mounting tests are memory-intensive. The heavy component tests
 * have been moved to integration.test.ts which uses isolation mode.
 *
 * This file focuses on testing:
 * 1. Mock data helpers (test infrastructure)
 * 2. Pure functions that don't require component mounting
 */
import { describe, it, expect } from 'vitest'
import { createMockDirectoryListing, filterHiddenFiles, createMockEntriesWithCount } from './test-helpers'

// ============================================================================
// Mock data helper tests
// ============================================================================

describe('Mock data helpers', () => {
    it('createMockDirectoryListing includes hidden and visible files', () => {
        const listing = createMockDirectoryListing()

        const hidden = listing.filter((f) => f.name.startsWith('.'))
        const visible = listing.filter((f) => !f.name.startsWith('.'))

        expect(hidden.length).toBeGreaterThan(0)
        expect(visible.length).toBeGreaterThan(0)
    })

    it('filterHiddenFiles correctly filters', () => {
        const listing = createMockDirectoryListing()

        const withHidden = filterHiddenFiles(listing, true)
        const withoutHidden = filterHiddenFiles(listing, false)

        expect(withHidden.length).toBe(listing.length)
        expect(withoutHidden.length).toBeLessThan(listing.length)
        expect(withoutHidden.every((f) => !f.name.startsWith('.') || f.name === '..')).toBe(true)
    })

    it('createMockEntriesWithCount creates correct count', () => {
        const entries = createMockEntriesWithCount(500)
        expect(entries.length).toBe(500)
    })

    it('createMockEntriesWithCount sorts directories first', () => {
        const entries = createMockEntriesWithCount(100)

        const dirs = entries.filter((e) => e.isDirectory)
        const files = entries.filter((e) => !e.isDirectory)

        if (dirs.length > 0 && files.length > 0) {
            const lastDirIndex = entries.findIndex((e) => e === dirs[dirs.length - 1])
            const firstFileIndex = entries.findIndex((e) => e === files[0])
            expect(lastDirIndex).toBeLessThan(firstFileIndex)
        }
    })
})

// Note: Keyboard shortcuts logic tests are in keyboard-shortcuts.test.ts
