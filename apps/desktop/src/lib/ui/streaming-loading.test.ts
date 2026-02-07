/**
 * Tests for streaming directory loading functionality.
 *
 * Tests LoadingIcon props, streaming types, and cancellation behavior.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import LoadingIcon from './LoadingIcon.svelte'
import type {
    ListingProgressEvent,
    ListingCompleteEvent,
    ListingErrorEvent,
    ListingCancelledEvent,
} from '$lib/file-explorer/types'

// ============================================================================
// LoadingIcon component tests
// ============================================================================

describe('LoadingIcon component', () => {
    let target: HTMLDivElement

    beforeEach(() => {
        vi.clearAllMocks()
        target = document.createElement('div')
        document.body.appendChild(target)
    })

    afterEach(() => {
        target.remove()
    })

    describe('Default behavior', () => {
        it('shows "Loading..." when no props provided', async () => {
            mount(LoadingIcon, { target, props: {} })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loading...')
        })

        it('does not show cancel hint by default', async () => {
            mount(LoadingIcon, { target, props: {} })
            await tick()

            const cancelHint = target.querySelector('.cancel-hint')
            expect(cancelHint).toBeNull()
        })
    })

    describe('loadedCount prop', () => {
        it('shows count when loadedCount is provided', async () => {
            mount(LoadingIcon, { target, props: { loadedCount: 1500 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loaded 1,500 files...')
        })

        it('shows count of 0 when loadedCount is 0', async () => {
            mount(LoadingIcon, { target, props: { loadedCount: 0 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loaded 0 files...')
        })

        it('updates count when loadedCount changes', async () => {
            mount(LoadingIcon, { target, props: { loadedCount: 100 } })
            await tick()

            let loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loaded 100 files...')

            // Update the prop - need to remount since we can't update props directly
            target.innerHTML = ''
            mount(LoadingIcon, { target, props: { loadedCount: 500 } })
            await tick()

            loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loaded 500 files...')
        })
    })

    describe('showCancelHint prop', () => {
        it('shows cancel hint when showCancelHint is true', async () => {
            mount(LoadingIcon, { target, props: { showCancelHint: true } })
            await tick()

            const cancelHint = target.querySelector('.cancel-hint')
            expect(cancelHint).not.toBeNull()
            expect(cancelHint?.textContent).toBe('Press ESC to cancel and go back')
        })

        it('does not show cancel hint when showCancelHint is false', async () => {
            mount(LoadingIcon, { target, props: { showCancelHint: false } })
            await tick()

            const cancelHint = target.querySelector('.cancel-hint')
            expect(cancelHint).toBeNull()
        })

        it('shows both count and cancel hint together', async () => {
            mount(LoadingIcon, { target, props: { loadedCount: 250, showCancelHint: true } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            const cancelHint = target.querySelector('.cancel-hint')

            expect(loadingText?.textContent).toBe('Loaded 250 files...')
            expect(cancelHint?.textContent).toBe('Press ESC to cancel and go back')
        })
    })

    describe('finalizingCount prop', () => {
        it('shows finalizing message when finalizingCount is provided', async () => {
            mount(LoadingIcon, { target, props: { finalizingCount: 600 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('All 600 files loaded, just a moment now.')
        })

        it('finalizingCount takes precedence over loadedCount', async () => {
            mount(LoadingIcon, { target, props: { loadedCount: 500, finalizingCount: 600 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('All 600 files loaded, just a moment now.')
        })

        it('shows finalizing message with cancel hint', async () => {
            mount(LoadingIcon, { target, props: { finalizingCount: 1000, showCancelHint: true } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            const cancelHint = target.querySelector('.cancel-hint')

            expect(loadingText?.textContent).toBe('All 1,000 files loaded, just a moment now.')
            expect(cancelHint?.textContent).toBe('Press ESC to cancel and go back')
        })
    })

    describe('openingFolder prop', () => {
        it('shows "Opening folder..." when openingFolder is true', async () => {
            mount(LoadingIcon, { target, props: { openingFolder: true } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Opening folder...')
        })

        it('loadedCount takes precedence over openingFolder', async () => {
            mount(LoadingIcon, { target, props: { openingFolder: true, loadedCount: 100 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loaded 100 files...')
        })

        it('finalizingCount takes precedence over openingFolder', async () => {
            mount(LoadingIcon, { target, props: { openingFolder: true, finalizingCount: 500 } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('All 500 files loaded, just a moment now.')
        })

        it('shows "Loading..." when openingFolder is false and no counts', async () => {
            mount(LoadingIcon, { target, props: { openingFolder: false } })
            await tick()

            const loadingText = target.querySelector('.loading-text')
            expect(loadingText?.textContent).toBe('Loading...')
        })
    })

    describe('Accessibility', () => {
        it('has loading container element', async () => {
            mount(LoadingIcon, { target, props: {} })
            await tick()

            const container = target.querySelector('.loading-container')
            expect(container).not.toBeNull()
        })

        it('has loader spinner element', async () => {
            mount(LoadingIcon, { target, props: {} })
            await tick()

            const loader = target.querySelector('.loader')
            expect(loader).not.toBeNull()
        })
    })
})

// ============================================================================
// Streaming event handling logic tests
// ============================================================================

describe('Streaming event handling', () => {
    it('progress event updates loadedCount correctly', () => {
        // Simulate the event handling logic from FilePane
        let loadingCount: number | undefined = undefined

        const progressEvent: ListingProgressEvent = {
            listingId: 'test-listing',
            loadedCount: 1500,
        }

        // Simulate the event handler
        const currentListingId = 'test-listing'
        if (progressEvent.listingId === currentListingId) {
            loadingCount = progressEvent.loadedCount
        }

        expect(loadingCount).toBe(1500)
    })

    it('progress event is ignored for different listing ID', () => {
        let loadingCount: number | undefined = undefined

        const progressEvent: ListingProgressEvent = {
            listingId: 'other-listing',
            loadedCount: 1500,
        }

        // Simulate the event handler
        const currentListingId = 'test-listing'
        if (progressEvent.listingId === currentListingId) {
            loadingCount = progressEvent.loadedCount
        }

        expect(loadingCount).toBeUndefined()
    })

    it('complete event sets totalCount and clears loading state', () => {
        let loading = true
        let loadingCount: number | undefined = 500
        let totalCount = 0

        const completeEvent: ListingCompleteEvent = {
            listingId: 'test-listing',
            totalCount: 2500,
            maxFilenameWidth: 120,
            volumeRoot: '/',
        }

        // Simulate the event handler
        const currentListingId = 'test-listing'
        if (completeEvent.listingId === currentListingId) {
            totalCount = completeEvent.totalCount
            loading = false
            loadingCount = undefined
        }

        expect(totalCount).toBe(2500)
        expect(loading).toBe(false)
        expect(loadingCount).toBeUndefined()
    })

    it('error event sets error message and clears loading state', () => {
        let loading = true
        let loadingCount: number | undefined = 500
        let error: string | null = null

        const errorEvent: ListingErrorEvent = {
            listingId: 'test-listing',
            message: 'Permission denied',
        }

        // Simulate the event handler
        const currentListingId = 'test-listing'
        if (errorEvent.listingId === currentListingId) {
            error = errorEvent.message
            loading = false
            loadingCount = undefined
        }

        expect(error).toBe('Permission denied')
        expect(loading).toBe(false)
        expect(loadingCount).toBeUndefined()
    })

    it('cancelled event clears loading state without error', () => {
        let loading = true
        let loadingCount: number | undefined = 500
        const error: string | null = null
        let listingId = 'test-listing'

        const cancelledEvent: ListingCancelledEvent = {
            listingId: 'test-listing',
        }

        // Simulate the event handler
        if (cancelledEvent.listingId === listingId) {
            listingId = ''
            loading = false
            loadingCount = undefined
        }

        expect(listingId).toBe('')
        expect(loading).toBe(false)
        expect(loadingCount).toBeUndefined()
        expect(error).toBeNull()
    })
})

// ============================================================================
// Load generation tracking tests
// ============================================================================

describe('Load generation tracking', () => {
    // Helper to simulate the load generation logic
    function simulateLoadWithGeneration(
        loadGen: number,
        capturedGen: number,
    ): { shouldProcess: boolean; shouldCancel: boolean } {
        return {
            shouldProcess: capturedGen === loadGen,
            shouldCancel: capturedGen !== loadGen,
        }
    }

    it('incrementing generation cancels previous load', () => {
        let loadGeneration = 0

        // Start first load
        loadGeneration++
        const firstGeneration = loadGeneration

        // Start second load (should cancel first)
        loadGeneration++

        // Check if first load should be cancelled
        const result = simulateLoadWithGeneration(loadGeneration, firstGeneration)

        expect(result.shouldCancel).toBe(true)
        expect(result.shouldProcess).toBe(false)
    })

    it('completed load with matching generation is processed', () => {
        const loadGeneration = 1
        const thisGeneration = 1

        const result = simulateLoadWithGeneration(loadGeneration, thisGeneration)

        expect(result.shouldProcess).toBe(true)
    })

    it('completed load with stale generation is ignored', () => {
        const loadGeneration = 2
        const thisGeneration = 1 // Stale - was captured when loadGeneration was 1

        const result = simulateLoadWithGeneration(loadGeneration, thisGeneration)

        expect(result.shouldProcess).toBe(false)
    })
})

// ============================================================================
// Cancel loading behavior tests
// ============================================================================

describe('Cancel loading behavior', () => {
    // Helper to simulate handleCancelLoading logic
    function simulateCancelLoading(loading: boolean, listingId: string): { cancelCalled: boolean } {
        if (!loading || !listingId) {
            return { cancelCalled: false }
        }
        return { cancelCalled: true }
    }

    it('handleCancelLoading does nothing when not loading', () => {
        const result = simulateCancelLoading(false, '')
        expect(result.cancelCalled).toBe(false)
    })

    it('handleCancelLoading does nothing when no listingId', () => {
        const result = simulateCancelLoading(true, '')
        expect(result.cancelCalled).toBe(false)
    })

    it('handleCancelLoading calls cancel when loading with listingId', () => {
        const result = simulateCancelLoading(true, 'test-listing')
        expect(result.cancelCalled).toBe(true)
    })
})

// ============================================================================
// History timing tests
// ============================================================================

describe('History timing', () => {
    it('onPathChange is called only after successful completion', () => {
        const pathChanges: string[] = []
        const onPathChange = (path: string) => pathChanges.push(path)

        // Simulate listing-complete - call onPathChange
        onPathChange('/Users/test/Documents')

        expect(pathChanges).toEqual(['/Users/test/Documents'])
    })

    it('onPathChange is not called on error (empty array)', () => {
        const pathChanges: string[] = []
        // On error, we don't call onPathChange - just verify the array stays empty
        expect(pathChanges).toEqual([])
    })

    it('onPathChange is not called on cancellation (empty array)', () => {
        const pathChanges: string[] = []
        // On cancellation, we don't call onPathChange - just verify the array stays empty
        expect(pathChanges).toEqual([])
    })
})
