/**
 * Vitest test setup file.
 * Polyfills browser APIs not available in jsdom.
 */

import { vi } from 'vitest'

// ResizeObserver is not available in jsdom
// This mock allows components that use ResizeObserver to run in tests
class ResizeObserverMock {
    callback: ResizeObserverCallback

    constructor(callback: ResizeObserverCallback) {
        this.callback = callback
    }

    observe() {
        // No-op in tests
    }

    unobserve() {
        // No-op in tests
    }

    disconnect() {
        // No-op in tests
    }
}

vi.stubGlobal('ResizeObserver', ResizeObserverMock)

// Mock Tauri event API to handle both static and dynamic imports
vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(() => Promise.resolve(() => {})),
    emit: vi.fn(() => Promise.resolve()),
}))
