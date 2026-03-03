import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { withTimeout, createDebounce, createThrottle } from './timing'

beforeEach(() => {
    vi.useFakeTimers()
})
afterEach(() => {
    vi.useRealTimers()
})

describe('withTimeout', () => {
    it('returns the promise result if it resolves before timeout', async () => {
        const promise = Promise.resolve('resolved-value')
        const result = await withTimeout(promise, 500, 'fallback')
        expect(result).toBe('resolved-value')
    })

    it('returns the fallback if the promise does not resolve in time', async () => {
        vi.useFakeTimers()
        const neverResolves = new Promise<string>(() => {})
        const resultPromise = withTimeout(neverResolves, 500, 'fallback')

        await vi.advanceTimersByTimeAsync(500)

        const result = await resultPromise
        expect(result).toBe('fallback')
        vi.useRealTimers()
    })

    it('returns the fallback value with correct type', async () => {
        vi.useFakeTimers()
        const neverResolves = new Promise<number | null>(() => {})
        const resultPromise = withTimeout(neverResolves, 100, null)

        await vi.advanceTimersByTimeAsync(100)

        const result = await resultPromise
        expect(result).toBeNull()
        vi.useRealTimers()
    })
})

describe('createDebounce', () => {
    it('fires after delay when called once', () => {
        const fn = vi.fn()
        const debounced = createDebounce(fn, 100)

        debounced.call()
        expect(fn).not.toHaveBeenCalled()

        vi.advanceTimersByTime(100)
        expect(fn).toHaveBeenCalledOnce()
    })

    it('resets timer on repeated calls — only the last one fires', () => {
        const fn = vi.fn()
        const debounced = createDebounce(fn, 100)

        debounced.call()
        vi.advanceTimersByTime(50)
        debounced.call()
        vi.advanceTimersByTime(50)
        debounced.call()
        vi.advanceTimersByTime(50)

        expect(fn).not.toHaveBeenCalled()

        vi.advanceTimersByTime(50)
        expect(fn).toHaveBeenCalledOnce()
    })

    it('cancel prevents pending call', () => {
        const fn = vi.fn()
        const debounced = createDebounce(fn, 100)

        debounced.call()
        vi.advanceTimersByTime(50)
        debounced.cancel()

        vi.advanceTimersByTime(200)
        expect(fn).not.toHaveBeenCalled()
    })

    it('flush fires immediately and clears timer', () => {
        const fn = vi.fn()
        const debounced = createDebounce(fn, 100)

        debounced.call()
        debounced.flush()
        expect(fn).toHaveBeenCalledOnce()

        // No double-fire after the original delay
        vi.advanceTimersByTime(200)
        expect(fn).toHaveBeenCalledOnce()
    })

    it('flush is a no-op when nothing is pending', () => {
        const fn = vi.fn()
        const debounced = createDebounce(fn, 100)

        debounced.flush()
        expect(fn).not.toHaveBeenCalled()
    })
})

describe('createThrottle', () => {
    it('fires immediately on first call', () => {
        const fn = vi.fn()
        const throttled = createThrottle(fn, 100)

        throttled.call()
        expect(fn).toHaveBeenCalledOnce()
    })

    it('suppresses calls within the delay window, fires trailing', () => {
        const fn = vi.fn()
        const throttled = createThrottle(fn, 100)

        throttled.call() // fires immediately
        throttled.call() // suppressed, schedules trailing
        throttled.call() // suppressed, trailing already scheduled

        expect(fn).toHaveBeenCalledOnce()

        vi.advanceTimersByTime(100)
        expect(fn).toHaveBeenCalledTimes(2) // trailing fires
    })

    it('allows another immediate call after delay has passed', () => {
        const fn = vi.fn()
        const throttled = createThrottle(fn, 100)

        throttled.call() // immediate
        vi.advanceTimersByTime(100)

        throttled.call() // immediate again (enough time passed)
        expect(fn).toHaveBeenCalledTimes(2)
    })

    it('cancel prevents the trailing call', () => {
        const fn = vi.fn()
        const throttled = createThrottle(fn, 100)

        throttled.call() // immediate
        throttled.call() // schedules trailing
        throttled.cancel()

        vi.advanceTimersByTime(200)
        expect(fn).toHaveBeenCalledOnce() // only the immediate one
    })

    it('handles rapid bursts with correct cadence', () => {
        const fn = vi.fn()
        const throttled = createThrottle(fn, 100)

        // Simulate 10 calls at 20ms intervals (burst over 200ms)
        for (let i = 0; i < 10; i++) {
            throttled.call()
            vi.advanceTimersByTime(20)
        }

        // Flush any remaining trailing
        vi.advanceTimersByTime(100)

        // Should fire at most ~3-4 times (immediate + trailing calls), not 10
        expect(fn.mock.calls.length).toBeGreaterThanOrEqual(2)
        expect(fn.mock.calls.length).toBeLessThanOrEqual(5)
    })
})
