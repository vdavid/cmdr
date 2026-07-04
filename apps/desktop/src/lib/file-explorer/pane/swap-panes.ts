/**
 * Full left/right pane swap (paths, volumes, history, sort, view mode, and
 * listing ownership) with zero backend calls — the frontend just trades listing
 * state. Lifted out of `DualPaneExplorer`; the component keeps the one-line
 * `export function swapPanes` delegate.
 *
 * Swapping active-tab nav-state is what drives persistence: each pane's
 * active-tab fields change, so the single `persistence-subscriber` re-runs both
 * per-pane effects (A5). No `saveAppStatus` call here.
 */

import { getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { FilePaneAPI } from './types'

export interface SwapPanesDeps {
    getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
    getLeftTabMgr: () => TabManager
    getRightTabMgr: () => TabManager
    /** True while any transfer dialog is open — a swap mid-transfer is unsafe. */
    isAnyTransferDialogOpen: () => boolean
    /** Re-anchor DOM focus on the explorer container after the swap. */
    focusContainer: () => void
}

export interface SwapPanes {
    swapPanes: () => void
}

export function createSwapPanes(deps: SwapPanesDeps): SwapPanes {
    /** True if pane swap is safe (both panes ready, none loading, no transfer dialog). */
    function canSwapPanes(): boolean {
        const leftRef = deps.getPaneRef('left')
        const rightRef = deps.getPaneRef('right')
        if (!leftRef || !rightRef) return false
        if (leftRef.isLoading() || rightRef.isLoading()) return false
        return !deps.isAnyTransferDialogOpen()
    }

    /** Swaps all active-tab nav-state between the left and right panes. */
    function swapDualPaneState(): void {
        const leftTab = getActiveTab(deps.getLeftTabMgr())
        const rightTab = getActiveTab(deps.getRightTabMgr())

        ;[leftTab.path, rightTab.path] = [rightTab.path, leftTab.path]
        ;[leftTab.volumeId, rightTab.volumeId] = [rightTab.volumeId, leftTab.volumeId]
        ;[leftTab.history, rightTab.history] = [rightTab.history, leftTab.history]
        ;[leftTab.viewMode, rightTab.viewMode] = [rightTab.viewMode, leftTab.viewMode]
        ;[leftTab.sortBy, rightTab.sortBy] = [rightTab.sortBy, leftTab.sortBy]
        ;[leftTab.sortOrder, rightTab.sortOrder] = [rightTab.sortOrder, leftTab.sortOrder]
    }

    function swapPanes(): void {
        if (!canSwapPanes()) return

        const leftRef = deps.getPaneRef('left')
        const rightRef = deps.getPaneRef('right')
        if (!leftRef || !rightRef) return

        // 1. Snapshot both panes' listing state
        const leftSwap = leftRef.getSwapState()
        const rightSwap = rightRef.getSwapState()

        // 2. Swap the active-tab nav-state
        swapDualPaneState()

        // 3. Each pane adopts the other's listing (no backend calls)
        leftRef.adoptListing(rightSwap)
        rightRef.adoptListing(leftSwap)

        // 4. Persistence (both panes' app-status fields + tab sets) fires from the
        // single subscriber: the swap mutates each pane's active-tab nav-state, so
        // both per-pane effects re-run.
        deps.focusContainer()
    }

    return { swapPanes }
}
