/** Races a promise against a timeout, returning the fallback if it doesn't resolve in time. */
export function withTimeout<T>(promise: Promise<T>, ms: number, fallback: T): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((resolve) =>
      setTimeout(() => {
        resolve(fallback)
      }, ms),
    ),
  ])
}

/**
 * Resolves `'painted'` once the webview has presented a frame (two nested
 * `requestAnimationFrame`s, so the browser has committed layout and painted),
 * or `'timeout'` after `timeoutMs` as a fallback.
 *
 * The timeout is load-bearing, not just defensive: `requestAnimationFrame` can
 * be throttled or paused while a window is hidden, and Cmdr's main window
 * launches `visible: false`. A naive frame wait could hang forever, so callers
 * gate on the return value and proceed regardless when it is `'timeout'`.
 */
export function waitForNextPaint(timeoutMs: number): Promise<'painted' | 'timeout'> {
  const painted = new Promise<'painted'>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        resolve('painted')
      })
    })
  })
  return withTimeout(painted, timeoutMs, 'timeout')
}

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
