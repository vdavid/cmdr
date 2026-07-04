import { describe, it, expect, vi } from 'vitest'
import type { TabManager } from '../tabs/tab-state-manager.svelte'
import type { FilePaneAPI } from './types'
import type { SwapState } from './types'
import { createSwapPanes, type SwapPanesDeps } from './swap-panes'

/** A TabManager holding a single active tab with the nav-state swap touches. */
function makeTabMgr(id: string, fields: Record<string, unknown>): TabManager {
    const tab = { id, ...fields }
    return { tabs: [tab], activeTabId: id } as unknown as TabManager
}

function makePaneRef(overrides: Partial<FilePaneAPI> = {}): FilePaneAPI {
    return {
        isLoading: vi.fn(() => false),
        getSwapState: vi.fn(() => ({ tag: 'swap' }) as unknown as SwapState),
        adoptListing: vi.fn(),
        ...overrides,
    } as unknown as FilePaneAPI
}

function setup(opts: {
    leftRef?: FilePaneAPI | undefined
    rightRef?: FilePaneAPI | undefined
    isAnyTransferDialogOpen?: boolean
    leftMgr?: TabManager
    rightMgr?: TabManager
}) {
    const focusContainer = vi.fn()
    const leftMgr = opts.leftMgr ?? makeTabMgr('L', { path: '/l', volumeId: 'vl' })
    const rightMgr = opts.rightMgr ?? makeTabMgr('R', { path: '/r', volumeId: 'vr' })
    const deps: SwapPanesDeps = {
        getPaneRef: (p) => (p === 'left' ? opts.leftRef : opts.rightRef),
        getLeftTabMgr: () => leftMgr,
        getRightTabMgr: () => rightMgr,
        isAnyTransferDialogOpen: () => opts.isAnyTransferDialogOpen ?? false,
        focusContainer,
    }
    return { deps, focusContainer, leftMgr, rightMgr }
}

describe('createSwapPanes', () => {
    it('swaps nav-state, adopts the other pane listing, and refocuses', () => {
        const leftRef = makePaneRef({ getSwapState: vi.fn(() => ({ id: 'left-listing' }) as unknown as SwapState) })
        const rightRef = makePaneRef({ getSwapState: vi.fn(() => ({ id: 'right-listing' }) as unknown as SwapState) })
        const leftMgr = makeTabMgr('L', {
            path: '/l',
            volumeId: 'vl',
            history: 'hl',
            viewMode: 'full',
            sortBy: 'name',
            sortOrder: 'ascending',
        })
        const rightMgr = makeTabMgr('R', {
            path: '/r',
            volumeId: 'vr',
            history: 'hr',
            viewMode: 'brief',
            sortBy: 'size',
            sortOrder: 'descending',
        })
        const { deps, focusContainer } = setup({ leftRef, rightRef, leftMgr, rightMgr })

        createSwapPanes(deps).swapPanes()

        const leftTab = leftMgr.tabs[0] as unknown as Record<string, unknown>
        const rightTab = rightMgr.tabs[0] as unknown as Record<string, unknown>
        expect(leftTab.path).toBe('/r')
        expect(rightTab.path).toBe('/l')
        expect(leftTab.volumeId).toBe('vr')
        expect(leftTab.viewMode).toBe('brief')
        expect(rightTab.sortBy).toBe('name')

        // Each pane adopts the OTHER pane's snapshot.
        expect(leftRef.adoptListing).toHaveBeenCalledWith({ id: 'right-listing' })
        expect(rightRef.adoptListing).toHaveBeenCalledWith({ id: 'left-listing' })
        expect(focusContainer).toHaveBeenCalled()
    })

    it('is a no-op when a pane ref is missing', () => {
        const rightRef = makePaneRef()
        const { deps, focusContainer } = setup({ leftRef: undefined, rightRef })

        createSwapPanes(deps).swapPanes()

        expect(rightRef.adoptListing).not.toHaveBeenCalled()
        expect(focusContainer).not.toHaveBeenCalled()
    })

    it('is a no-op while a pane is loading', () => {
        const leftRef = makePaneRef({ isLoading: vi.fn(() => true) })
        const rightRef = makePaneRef()
        const { deps } = setup({ leftRef, rightRef })

        createSwapPanes(deps).swapPanes()

        expect(leftRef.adoptListing).not.toHaveBeenCalled()
        expect(rightRef.adoptListing).not.toHaveBeenCalled()
    })

    it('is a no-op while a transfer dialog is open', () => {
        const leftRef = makePaneRef()
        const rightRef = makePaneRef()
        const { deps } = setup({ leftRef, rightRef, isAnyTransferDialogOpen: true })

        createSwapPanes(deps).swapPanes()

        expect(leftRef.adoptListing).not.toHaveBeenCalled()
    })
})
