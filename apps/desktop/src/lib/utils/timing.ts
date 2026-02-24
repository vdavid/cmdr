/**
 * Debounce: delays execution until `delayMs` after the last call.
 * Only the final call in a burst fires. Good for "I only care about the end state."
 */
export function createDebounce(fn: () => void, delayMs: number) {
    let timer: ReturnType<typeof setTimeout> | null = null

    function call() {
        if (timer !== null) clearTimeout(timer)
        timer = setTimeout(() => {
            timer = null
            fn()
        }, delayMs)
    }

    function cancel() {
        if (timer !== null) {
            clearTimeout(timer)
            timer = null
        }
    }

    /** Cancel pending timer and fire immediately. */
    function flush() {
        if (timer !== null) {
            clearTimeout(timer)
            timer = null
            fn()
        }
    }

    return { call, cancel, flush }
}

/**
 * Throttle: fires immediately on first call, then at most once per `delayMs`.
 * Trailing call guaranteed (last call always fires). Good for "show live progress at a steady cadence."
 */
export function createThrottle(fn: () => void, delayMs: number) {
    let timer: ReturnType<typeof setTimeout> | null = null
    let lastFireTime = 0

    function call() {
        const now = Date.now()
        const elapsed = now - lastFireTime

        if (elapsed >= delayMs) {
            lastFireTime = now
            if (timer !== null) {
                clearTimeout(timer)
                timer = null
            }
            fn()
        } else if (timer === null) {
            timer = setTimeout(() => {
                timer = null
                lastFireTime = Date.now()
                fn()
            }, delayMs - elapsed)
        }
    }

    function cancel() {
        if (timer !== null) {
            clearTimeout(timer)
            timer = null
        }
    }

    return { call, cancel }
}
