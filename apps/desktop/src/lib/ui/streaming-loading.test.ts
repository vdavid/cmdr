/**
 * Tests for streaming directory loading functionality.
 *
 * Tests LoadingIcon props, streaming types, and cancellation behavior.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import LoadingIcon from './LoadingIcon.svelte'

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
