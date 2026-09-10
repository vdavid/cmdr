/**
 * What the viewer shows while an open pulls its file into a preview temp first (a file
 * on a phone or server, a big entry in a zip). The backend emits `viewer-pull-progress`
 * to this window while it pulls; this decides when that's worth showing, and what the
 * bar draws.
 *
 * - Hidden for the first second, so a quick pull never flashes a bar.
 * - Hidden while nothing is being pulled: an ordinary open that's merely slow keeps the
 *   plain loading line, since it has no honest progress to show.
 * - `stalled` once no new bytes arrive for a few seconds, which stops the bar's
 *   shimmer. Giving up is the backend's call (its stall rule answers
 *   `stoppedResponding`); this only stops claiming motion that isn't happening.
 */

/** How long an open pulls before the bar appears. */
export const PULL_BAR_DELAY_MS = 1000
/** How long without new bytes before the bar reads as halted. */
export const PULL_STALLED_AFTER_MS = 3000
/** How often `stalled` re-checks the clock while an open pulls. */
const CLOCK_TICK_MS = 500

/** One progress report, shaped like the backend's `ViewerPullProgress`. */
export interface PullReport {
  bytesDone: number
  /** `null` when the source didn't say how big the file is. */
  bytesTotal: number | null
}

export function createViewerPull() {
  let active = $state(false)
  let delayPassed = $state(false)
  let progress = $state<PullReport | null>(null)
  let lastNewsAt = $state(0)
  let clock = $state(0)
  let delayTimer: ReturnType<typeof setTimeout> | undefined
  let clockTimer: ReturnType<typeof setInterval> | undefined

  function stopTimers(): void {
    clearTimeout(delayTimer)
    clearInterval(clockTimer)
    delayTimer = undefined
    clockTimer = undefined
  }

  return {
    /** An open starts: forget the last one and start the one-second delay. */
    start(): void {
      stopTimers()
      active = true
      delayPassed = false
      progress = null
      lastNewsAt = Date.now()
      clock = lastNewsAt
      delayTimer = setTimeout(() => {
        delayPassed = true
      }, PULL_BAR_DELAY_MS)
      clockTimer = setInterval(() => {
        clock = Date.now()
      }, CLOCK_TICK_MS)
    },

    /** The backend reported progress. Ignored between opens. */
    report(next: PullReport): void {
      if (!active) return
      if (progress?.bytesDone !== next.bytesDone) {
        lastNewsAt = Date.now()
        clock = lastNewsAt
      }
      progress = next
    },

    /** The open resolved, one way or the other. */
    finish(): void {
      stopTimers()
      active = false
      delayPassed = false
      progress = null
    },

    destroy(): void {
      stopTimers()
    },

    get visible(): boolean {
      return active && delayPassed && progress !== null
    },

    get bytesDone(): number {
      return progress?.bytesDone ?? 0
    },

    get bytesTotal(): number | null {
      return progress?.bytesTotal ?? null
    },

    /** How far along, 0–1, or `null` when the total is unknown (or zero). */
    get fraction(): number | null {
      const total = progress?.bytesTotal ?? null
      if (progress === null || total === null || total === 0) return null
      return Math.min(1, progress.bytesDone / total)
    },

    get stalled(): boolean {
      return active && clock - lastNewsAt >= PULL_STALLED_AFTER_MS
    },
  }
}
