import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { createHistory } from '../navigation/navigation-history'
import type { TabState } from './tab-types'
import {
    createTabManager,
    getActiveTab,
    addTab,
    closeTab,
    closeOtherTabs,
    switchTab,
    pinTab,
    unpinTab,
    cycleTab,
    resetCycleDebounce,
    getAllTabs,
    getTabCount,
    MAX_TABS_PER_PANE,
} from './tab-state-manager.svelte'

function makeTab(overrides: Partial<TabState> = {}): TabState {
    return {
        id: crypto.randomUUID(),
        path: '/Users/test',
        volumeId: 'root',
        history: createHistory('root', '/Users/test'),
        sortBy: 'name',
        sortOrder: 'ascending',
        viewMode: 'full',
        pinned: false,
        cursorFilename: null,
        ...overrides,
    }
}

describe('tab-state-manager', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        resetCycleDebounce()
    })

    afterEach(() => {
        resetCycleDebounce()
        vi.useRealTimers()
    })

    describe('createTabManager', () => {
        it('creates manager with one tab', () => {
            const tab = makeTab()
            const mgr = createTabManager(tab)

            expect(getTabCount(mgr)).toBe(1)
            expect(mgr.activeTabId).toBe(tab.id)
            expect(getActiveTab(mgr)).toStrictEqual(tab)
        })
    })

    describe('getActiveTab', () => {
        it('falls back to first tab when activeTabId is stale', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)

            // Simulate a stale activeTabId
            mgr.activeTabId = 'nonexistent-id'

            const result = getActiveTab(mgr)

            expect(result.id).toBe('tab-2') // First tab in the array
            expect(mgr.activeTabId).toBe('tab-2') // activeTabId corrected
        })
    })

    describe('addTab', () => {
        it('inserts tab before the active tab without changing activeTabId', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)

            const tab2 = makeTab({ id: 'tab-2' })
            const result = addTab(mgr, 'tab-1', tab2)

            expect(result).toBe(true)
            expect(getTabCount(mgr)).toBe(2)
            // tab2 should be to the left of tab1
            expect(getAllTabs(mgr)[0].id).toBe('tab-2')
            expect(getAllTabs(mgr)[1].id).toBe('tab-1')
            // activeTabId stays on the original tab (clone trick)
            expect(mgr.activeTabId).toBe('tab-1')
        })

        it('returns false at cap (10 tabs)', () => {
            const firstTab = makeTab({ id: 'tab-0' })
            const mgr = createTabManager(firstTab)

            for (let i = 1; i < MAX_TABS_PER_PANE; i++) {
                const tab = makeTab({ id: `tab-${String(i)}` })
                addTab(mgr, mgr.activeTabId, tab)
            }

            expect(getTabCount(mgr)).toBe(MAX_TABS_PER_PANE)

            const extraTab = makeTab({ id: 'tab-extra' })
            const result = addTab(mgr, mgr.activeTabId, extraTab)

            expect(result).toBe(false)
            expect(getTabCount(mgr)).toBe(MAX_TABS_PER_PANE)
        })

        it('appends at end when beforeTabId is not found', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)

            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'nonexistent', tab2)

            expect(getAllTabs(mgr)[1].id).toBe('tab-2')
        })
    })

    describe('closeTab', () => {
        it('activates right neighbor when closing active tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3' })
            addTab(mgr, 'tab-1', tab2)
            addTab(mgr, 'tab-1', tab3)
            // Order: tab-2, tab-3, tab-1. Active is still tab-1.
            // Set active to tab-3 for this test
            mgr.activeTabId = 'tab-3'

            const result = closeTab(mgr, 'tab-3')

            expect(result).toEqual({ closed: true, newActiveTabId: 'tab-1' })
            expect(getTabCount(mgr)).toBe(2)
        })

        it('activates left neighbor when closing rightmost tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)
            // Order: tab-2, tab-1. Make tab-1 active (rightmost)
            mgr.activeTabId = 'tab-1'

            const result = closeTab(mgr, 'tab-1')

            expect(result).toEqual({ closed: true, newActiveTabId: 'tab-2' })
        })

        it('returns closed false when it is the last tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)

            const result = closeTab(mgr, 'tab-1')

            expect(result).toEqual({ closed: false })
            expect(getTabCount(mgr)).toBe(1)
        })

        it('does not change active when closing inactive tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)
            // Active is tab-2, close tab-1
            mgr.activeTabId = 'tab-2'

            const result = closeTab(mgr, 'tab-1')

            expect(result).toEqual({ closed: true, newActiveTabId: 'tab-2' })
            expect(mgr.activeTabId).toBe('tab-2')
        })
    })

    describe('closeOtherTabs', () => {
        it('closes all unpinned tabs except the given one', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3' })
            addTab(mgr, 'tab-1', tab2)
            addTab(mgr, 'tab-1', tab3)

            closeOtherTabs(mgr, 'tab-1')

            expect(getTabCount(mgr)).toBe(1)
            expect(getAllTabs(mgr)[0].id).toBe('tab-1')
            expect(mgr.activeTabId).toBe('tab-1')
        })

        it('keeps pinned tabs', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3', pinned: true })
            addTab(mgr, 'tab-1', tab2)
            addTab(mgr, 'tab-1', tab3)

            closeOtherTabs(mgr, 'tab-1')

            expect(getTabCount(mgr)).toBe(2)
            const ids = getAllTabs(mgr).map((t) => t.id)
            expect(ids).toContain('tab-1')
            expect(ids).toContain('tab-3')
            expect(mgr.activeTabId).toBe('tab-1')
        })
    })

    describe('switchTab', () => {
        it('stores cursor filename on old active tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)
            mgr.activeTabId = 'tab-1'

            const result = switchTab(mgr, 'tab-2', 'document.txt')

            expect(result).toBe(true)
            expect(mgr.activeTabId).toBe('tab-2')
            const oldTab = getAllTabs(mgr).find((t) => t.id === 'tab-1')
            expect(oldTab?.cursorFilename).toBe('document.txt')
        })

        it('activates the new tab', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)
            mgr.activeTabId = 'tab-1'

            const result = switchTab(mgr, 'tab-2', null)

            expect(result).toBe(true)
            expect(mgr.activeTabId).toBe('tab-2')
        })

        it('returns false and does not switch for nonexistent tab ID', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            mgr.activeTabId = 'tab-1'

            const result = switchTab(mgr, 'nonexistent-id', null)

            expect(result).toBe(false)
            expect(mgr.activeTabId).toBe('tab-1')
        })
    })

    describe('pinTab / unpinTab', () => {
        it('pins a tab', () => {
            const tab1 = makeTab({ id: 'tab-1', pinned: false })
            const mgr = createTabManager(tab1)

            pinTab(mgr, 'tab-1')

            expect(getAllTabs(mgr)[0].pinned).toBe(true)
        })

        it('unpins a tab', () => {
            const tab1 = makeTab({ id: 'tab-1', pinned: true })
            const mgr = createTabManager(tab1)

            unpinTab(mgr, 'tab-1')

            expect(getAllTabs(mgr)[0].pinned).toBe(false)
        })
    })

    describe('cycleTab', () => {
        it('cycles to the next tab wrapping around', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3' })
            addTab(mgr, 'tab-1', tab2)
            addTab(mgr, 'tab-1', tab3)
            // Order: tab-2, tab-3, tab-1
            mgr.activeTabId = 'tab-1'

            // tab-1 is at index 2, next wraps to index 0 = tab-2
            const result = cycleTab(mgr, 'next', null)

            expect(result).toBe('tab-2')
            expect(mgr.activeTabId).toBe('tab-2')
        })

        it('cycles to the previous tab wrapping around', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3' })
            addTab(mgr, 'tab-1', tab2)
            addTab(mgr, 'tab-1', tab3)
            // Order: tab-2, tab-3, tab-1
            mgr.activeTabId = 'tab-2'

            // tab-2 is at index 0, prev wraps to index 2 = tab-1
            const result = cycleTab(mgr, 'prev', null)

            expect(result).toBe('tab-1')
            expect(mgr.activeTabId).toBe('tab-1')
        })

        it('debounces rapid cycles so the final tab wins', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            const tab3 = makeTab({ id: 'tab-3' })
            const tab4 = makeTab({ id: 'tab-4' })
            addTab(mgr, 'tab-1', tab2) // Order: tab-2, tab-1
            addTab(mgr, 'tab-1', tab3) // Order: tab-2, tab-3, tab-1
            addTab(mgr, 'tab-1', tab4) // Order: tab-2, tab-3, tab-4, tab-1
            // Active is still tab-1. Set to tab-2 for this test.
            mgr.activeTabId = 'tab-2'

            // First call fires immediately (leading edge)
            cycleTab(mgr, 'next', null)
            expect(mgr.activeTabId).toBe('tab-3')

            // Rapid subsequent calls within debounce window just store the target
            cycleTab(mgr, 'next', null)
            cycleTab(mgr, 'next', null)

            // After the debounce timeout, the final stored tab should be committed
            vi.advanceTimersByTime(50)

            // The final cycle from tab-3's position: next would be tab-4, next would be tab-1
            // But cycleTab calculates from the current activeTabId at call time
            // Second call: active is tab-3, next = tab-4 (stored but not applied)
            // Third call: active is still tab-3, next = tab-4 (stored again, same result)
            // After timeout: commits tab-4
            expect(mgr.activeTabId).toBe('tab-4')
        })

        it('stores cursor filename when cycling', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)
            mgr.activeTabId = 'tab-1'

            cycleTab(mgr, 'next', 'myfile.txt')

            const oldTab = getAllTabs(mgr).find((t) => t.id === 'tab-1')
            expect(oldTab?.cursorFilename).toBe('myfile.txt')
        })
    })

    describe('getAllTabs / getTabCount', () => {
        it('returns all tabs', () => {
            const tab1 = makeTab({ id: 'tab-1' })
            const mgr = createTabManager(tab1)
            const tab2 = makeTab({ id: 'tab-2' })
            addTab(mgr, 'tab-1', tab2)

            expect(getAllTabs(mgr)).toHaveLength(2)
            expect(getTabCount(mgr)).toBe(2)
        })
    })
})
