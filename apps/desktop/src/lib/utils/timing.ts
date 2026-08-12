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
 * Coalesce: keeps at most ONE run of `run` in flight, and remembers only the newest
 * request that arrived while it was busy. Good for "fetch the state of what's on screen
 * now", where a superseded request has nothing left to contribute.
 *
 * **Why this and not a debounce.** A debounce bounds how often you START work; it does
 * nothing about work that takes longer than the delay. When each call outlasts the
 * debounce window, calls simply stack, every one of them holding whatever the far side
 * holds. That's how the image-index badge fetch put hundreds of concurrent queries on
 * the backend's blocking pool and froze the app. The two compose: debounce the trigger,
 * coalesce the call.
 *
 * `call` resolves once the work it stands for has settled. A caller that gets
 * superseded resolves immediately (its request is now the queued one, which the
 * in-flight caller's promise covers), so ❌ don't read a resolved `call` as "the newest
 * request finished" — only the run that owns the drain can say that.
 */
export function createCoalesced<A>(run: (arg: A) => Promise<void>) {
  let inFlight = false
  let queued: { arg: A } | null = null

  async function call(arg: A): Promise<void> {
    if (inFlight) {
      // Latest wins: an older pending request describes a screen that has moved on.
      queued = { arg }
      return
    }
    inFlight = true
    try {
      await run(arg)
      // Drain inside the same in-flight window, so a burst can't open a second run.
      while (queued !== null) {
        const next = queued
        queued = null
        await run(next.arg)
      }
    } finally {
      inFlight = false
      queued = null
    }
  }

  /** Drop a queued request (for teardown, so a destroyed owner fires no more work). */
  function cancel() {
    queued = null
  }

  return { call, cancel }
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
