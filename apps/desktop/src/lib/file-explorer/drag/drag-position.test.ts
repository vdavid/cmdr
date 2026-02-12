import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { recalculateWebviewOffset, toViewportPosition } from './drag-position'

const mockOuterSize = vi.fn()

vi.mock('@tauri-apps/api/window', () => ({
    getCurrentWindow: () => ({
        outerSize: mockOuterSize,
    }),
}))

describe('drag-position', () => {
    let originalDevicePixelRatio: number
    let originalInnerWidth: number
    let originalInnerHeight: number

    beforeEach(() => {
        originalDevicePixelRatio = window.devicePixelRatio
        originalInnerWidth = window.innerWidth
        originalInnerHeight = window.innerHeight
    })

    afterEach(() => {
        Object.defineProperty(window, 'devicePixelRatio', { value: originalDevicePixelRatio, writable: true })
        Object.defineProperty(window, 'innerWidth', { value: originalInnerWidth, writable: true })
        Object.defineProperty(window, 'innerHeight', { value: originalInnerHeight, writable: true })
    })

    function setViewportSize(width: number, height: number, dpr: number) {
        Object.defineProperty(window, 'innerWidth', { value: width, writable: true })
        Object.defineProperty(window, 'innerHeight', { value: height, writable: true })
        Object.defineProperty(window, 'devicePixelRatio', { value: dpr, writable: true })
    }

    function mockWindowOuterSize(physWidth: number, physHeight: number) {
        mockOuterSize.mockResolvedValue({ width: physWidth, height: physHeight })
    }

    describe('toViewportPosition without recalculation', () => {
        it('passes coordinates through when offset has not been calculated', () => {
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 200 })
        })
    })

    describe('with DevTools closed (no offset)', () => {
        it('passes coordinates through on 2x Retina display', async () => {
            setViewportSize(1201, 828, 2)
            // outerSize matches viewport (no DevTools, overlay title bar)
            mockWindowOuterSize(2402, 1656)

            await recalculateWebviewOffset()

            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 200 })
        })
    })

    describe('with DevTools docked at bottom', () => {
        it('corrects y offset on 2x Retina display', async () => {
            // DevTools takes 275 logical px at the bottom (828 - 553)
            setViewportSize(1201, 553, 2)
            // outerSize stays at full window size
            mockWindowOuterSize(2402, 1656)

            await recalculateWebviewOffset()

            // y should be corrected by +275 (the DevTools height)
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 475 })
        })

        it('corrects y offset on 1x display', async () => {
            setViewportSize(1201, 553, 1)
            mockWindowOuterSize(1201, 828)

            await recalculateWebviewOffset()

            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 475 })
        })
    })

    describe('with DevTools docked at side', () => {
        it('corrects x offset when DevTools is docked on the right', async () => {
            setViewportSize(800, 828, 2)
            mockWindowOuterSize(2402, 1656)

            await recalculateWebviewOffset()

            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 501, y: 200 })
        })
    })

    describe('recalculateWebviewOffset', () => {
        it('updates offset when DevTools opens', async () => {
            setViewportSize(1201, 828, 2)
            mockWindowOuterSize(2402, 1656)
            await recalculateWebviewOffset()
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 200 })

            // DevTools opens — viewport shrinks
            setViewportSize(1201, 553, 2)
            await recalculateWebviewOffset()
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 475 })
        })

        it('resets offset when DevTools is closed', async () => {
            setViewportSize(1201, 553, 2)
            mockWindowOuterSize(2402, 1656)
            await recalculateWebviewOffset()
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 475 })

            // DevTools closed
            setViewportSize(1201, 828, 2)
            await recalculateWebviewOffset()
            expect(toViewportPosition({ x: 100, y: 200 })).toEqual({ x: 100, y: 200 })
        })
    })
})
