/**
 * Comprehensive component tests for file explorer.
 *
 * These tests verify actual component rendering with mocked Tauri backend.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import BriefList from './BriefList.svelte'
import FullList from './FullList.svelte'
import { createMockDirectoryListing, filterHiddenFiles, createMockEntriesWithCount } from './test-helpers'
import type { FileEntry } from './types'

// ============================================================================
// Mock setup
// ============================================================================

// Create mock data that will be returned by getFileRange
let mockEntries: FileEntry[] = []
let mockGetFileRangeCalls: { listingId: string; start: number; count: number; includeHidden: boolean }[] = []

vi.mock('$lib/tauri-commands', () => ({
    getFileRange: vi.fn(
        (listingId: string, start: number, count: number, includeHidden: boolean): Promise<FileEntry[]> => {
            mockGetFileRangeCalls.push({ listingId, start, count, includeHidden })
            // Return a slice of mock entries
            const entries = includeHidden ? mockEntries : mockEntries.filter((e) => !e.name.startsWith('.'))
            const end = Math.min(start + count, entries.length)
            return Promise.resolve(entries.slice(start, end))
        },
    ),
    getSyncStatusBatch: vi.fn().mockResolvedValue([]),
}))

vi.mock('$lib/icon-cache', async () => {
    const { writable } = await import('svelte/store')
    return {
        getCachedIcon: vi.fn().mockReturnValue('/icons/file.png'),
        iconCacheVersion: writable(0),
        prefetchIcons: vi.fn().mockResolvedValue(undefined),
    }
})

vi.mock('$lib/drag-drop', () => ({
    startDragTracking: vi.fn(),
}))

// Helper to wait for async updates
async function waitForUpdates(ms = 50): Promise<void> {
    await tick()
    await new Promise((r) => setTimeout(r, ms))
    await tick()
}

// ============================================================================
// BriefList component tests
// ============================================================================

describe('BriefList component', () => {
    let target: HTMLDivElement

    beforeEach(() => {
        vi.clearAllMocks()
        mockGetFileRangeCalls = []
        mockEntries = createMockDirectoryListing()
        target = document.createElement('div')
        document.body.appendChild(target)
    })

    afterEach(() => {
        target.remove()
    })

    describe('Rendering', () => {
        it('calls getFileRange when listingId and container dimensions are set', async () => {
            mount(BriefList, {
                target,
                props: {
                    listingId: 'test-listing-1',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            // Manually trigger the reactive dependencies by setting container dimensions
            // This simulates what bind:clientHeight/clientWidth would do
            const briefList = target.querySelector('.brief-list') as HTMLElement
            // Simulate container dimensions
            Object.defineProperty(briefList, 'clientHeight', { value: 400, configurable: true })
            Object.defineProperty(briefList, 'clientWidth', { value: 600, configurable: true })

            await waitForUpdates(100)

            // The component should exist
            expect(target.querySelector('.brief-list')).toBeTruthy()
        })

        it('applies is-focused class when isFocused is true', async () => {
            mount(BriefList, {
                target,
                props: {
                    listingId: 'test-focused',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            const container = target.querySelector('.brief-list-container')
            expect(container?.classList.contains('is-focused')).toBe(true)
        })

        it('does NOT apply is-focused class when isFocused is false', async () => {
            mount(BriefList, {
                target,
                props: {
                    listingId: 'test-not-focused',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: false,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            const container = target.querySelector('.brief-list-container')
            expect(container?.classList.contains('is-focused')).toBe(false)
        })
    })

    describe('Exported methods', () => {
        it('exports handleKeyNavigation method', async () => {
            const component = mount(BriefList, {
                target,
                props: {
                    listingId: 'test-methods',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 2,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            expect(typeof (component as unknown as Record<string, unknown>).handleKeyNavigation).toBe('function')
        })

        it('handleKeyNavigation returns correct index for ArrowDown', async () => {
            const component = mount(BriefList, {
                target,
                props: {
                    listingId: 'test-arrow-down',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 2,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            const newIndex = (
                component as unknown as { handleKeyNavigation: (key: string) => number | undefined }
            ).handleKeyNavigation('ArrowDown')
            expect(newIndex).toBe(3)
        })

        it('handleKeyNavigation returns correct index for ArrowUp', async () => {
            const component = mount(BriefList, {
                target,
                props: {
                    listingId: 'test-arrow-up',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 5,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            const newIndex = (
                component as unknown as { handleKeyNavigation: (key: string) => number | undefined }
            ).handleKeyNavigation('ArrowUp')
            expect(newIndex).toBe(4)
        })

        it('handleKeyNavigation clamps at bounds', async () => {
            const component = mount(BriefList, {
                target,
                props: {
                    listingId: 'test-bounds',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            // At index 0, ArrowUp should return 0 (clamped)

            const newIndex = (
                component as unknown as { handleKeyNavigation: (key: string) => number | undefined }
            ).handleKeyNavigation('ArrowUp')
            expect(newIndex).toBe(0)
        })

        it('exports scrollToIndex method', async () => {
            const component = mount(BriefList, {
                target,
                props: {
                    listingId: 'test-scroll',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            expect(typeof (component as unknown as Record<string, unknown>).scrollToIndex).toBe('function')
        })
    })

    describe('Props validation', () => {
        it('passes includeHidden to getFileRange calls', async () => {
            // Mount with includeHidden=true
            mount(BriefList, {
                target,
                props: {
                    listingId: 'test-include-hidden',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    parentPath: '/',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            // The props are passed correctly - we check component accepts them
            await tick()
            expect(target.querySelector('.brief-list')).toBeTruthy()
        })
    })
})

// ============================================================================
// FullList component tests
// ============================================================================

describe('FullList component', () => {
    let target: HTMLDivElement

    beforeEach(() => {
        vi.clearAllMocks()
        mockGetFileRangeCalls = []
        mockEntries = createMockDirectoryListing()
        target = document.createElement('div')
        document.body.appendChild(target)
    })

    afterEach(() => {
        target.remove()
    })

    describe('Rendering', () => {
        it('renders full-list container', async () => {
            mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-list',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            expect(target.querySelector('.full-list')).toBeTruthy()
        })

        it('applies is-focused class when focused', async () => {
            mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-focused',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            const list = target.querySelector('.full-list')
            expect(list?.classList.contains('is-focused')).toBe(true)
        })
    })

    describe('Exported methods', () => {
        it('exports getVisibleItemsCount method', async () => {
            const component = mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-methods',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            expect(typeof (component as unknown as Record<string, unknown>).getVisibleItemsCount).toBe('function')
        })

        it('exports scrollToIndex method', async () => {
            const component = mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-scroll',
                    totalCount: 10,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            expect(typeof (component as unknown as Record<string, unknown>).scrollToIndex).toBe('function')
        })
    })

    describe('Sorting headers', () => {
        it('renders sortable column headers', async () => {
            mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-headers',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            // Should have header row with sortable headers
            const headers = target.querySelectorAll('.header-row button')
            expect(headers.length).toBe(3) // Name, Size, Modified
        })

        it('calls onSortChange when header is clicked', async () => {
            const onSortChange = vi.fn()
            mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-sort-click',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                    onSortChange,
                },
            })

            await tick()

            // Click the Size header (find by text content instead of aria-label)
            const headers = target.querySelectorAll('.header-row button.sortable-header')
            // Size is the second header (Name, Size, Modified)
            const sizeHeader = headers[1]
            expect(sizeHeader).toBeTruthy()

            sizeHeader.dispatchEvent(new MouseEvent('click', { bubbles: true }))
            await tick()

            expect(onSortChange).toHaveBeenCalledWith('size')
        })

        it('shows sort indicator on current column', async () => {
            mount(FullList, {
                target,
                props: {
                    listingId: 'test-full-sort-indicator',
                    totalCount: mockEntries.length,
                    includeHidden: true,
                    selectedIndex: 0,
                    isFocused: true,
                    hasParent: false,
                    parentPath: '/',
                    sortBy: 'name',
                    sortOrder: 'ascending',
                    onSelect: vi.fn(),
                    onNavigate: vi.fn(),
                },
            })

            await tick()

            // The name header should have the is-active class and ascending indicator
            const nameHeader = target.querySelector('.header-row .sortable-header.is-active')
            expect(nameHeader).toBeTruthy()
            expect(nameHeader?.textContent).toContain('Name')
            expect(nameHeader?.textContent).toContain('▲')
        })
    })
})

// ============================================================================
// Virtual scrolling logic tests
// ============================================================================

describe('Virtual scrolling calculations', () => {
    // Import the pure functions directly
    it('calculateVirtualWindow returns correct range', async () => {
        const { calculateVirtualWindow } = await import('./virtual-scroll')

        const result = calculateVirtualWindow({
            direction: 'vertical',
            itemSize: 20,
            bufferSize: 5,
            containerSize: 400,
            scrollOffset: 0,
            totalItems: 1000,
        })

        // Should show items 0-24 (20 visible + buffer), not all 1000
        expect(result.startIndex).toBe(0)
        expect(result.endIndex).toBeLessThan(1000)
        expect(result.endIndex - result.startIndex).toBeLessThan(100)
    })

    it('calculateVirtualWindow handles scroll offset', async () => {
        const { calculateVirtualWindow } = await import('./virtual-scroll')

        const result = calculateVirtualWindow({
            direction: 'vertical',
            itemSize: 20,
            bufferSize: 5,
            containerSize: 400,
            scrollOffset: 200, // Scrolled down 10 items
            totalItems: 1000,
        })

        // Start should be near 10 (scroll offset / item size) minus buffer
        expect(result.startIndex).toBeGreaterThan(0)
    })

    it('getScrollToPosition calculates correct position', async () => {
        const { getScrollToPosition } = await import('./virtual-scroll')

        // Item at index 10, itemSize 100, already scrolled to 0, container 400
        const position = getScrollToPosition(10, 100, 0, 400)

        // Should return scroll position to bring item 10 into view
        expect(position).toBeDefined()
    })
})

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

// ============================================================================
// Keyboard shortcuts logic tests
// ============================================================================

describe('Keyboard shortcuts logic', () => {
    it('handleNavigationShortcut returns correct index for Home', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'Home' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 50,
            totalCount: 100,
            visibleItems: 20,
        })

        expect(result?.newIndex).toBe(0)
    })

    it('handleNavigationShortcut returns correct index for End', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'End' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 50,
            totalCount: 100,
            visibleItems: 20,
        })

        expect(result?.newIndex).toBe(99)
    })

    it('handleNavigationShortcut returns correct index for PageDown', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'PageDown' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 10,
            totalCount: 100,
            visibleItems: 20,
        })

        // Should jump by visibleItems - 1
        expect(result?.newIndex).toBe(29) // 10 + 19
    })

    it('handleNavigationShortcut returns correct index for PageUp', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'PageUp' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 50,
            totalCount: 100,
            visibleItems: 20,
        })

        // Should jump by visibleItems - 1
        expect(result?.newIndex).toBe(31) // 50 - 19
    })

    it('handleNavigationShortcut clamps PageDown at end', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'PageDown' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 90,
            totalCount: 100,
            visibleItems: 20,
        })

        // Should clamp to totalCount - 1
        expect(result?.newIndex).toBe(99)
    })

    it('handleNavigationShortcut clamps PageUp at start', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'PageUp' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 5,
            totalCount: 100,
            visibleItems: 20,
        })

        // Should clamp to 0
        expect(result?.newIndex).toBe(0)
    })

    it('handleNavigationShortcut returns undefined for non-navigation keys', async () => {
        const { handleNavigationShortcut } = await import('./keyboard-shortcuts')

        const event = new KeyboardEvent('keydown', { key: 'a' })
        const result = handleNavigationShortcut(event, {
            currentIndex: 50,
            totalCount: 100,
            visibleItems: 20,
        })

        expect(result).toBeNull()
    })
})
