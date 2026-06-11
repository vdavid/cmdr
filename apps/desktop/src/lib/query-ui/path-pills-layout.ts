/**
 * Pure layout helper for `PathPills.svelte`. Decides which segments to render directly
 * versus hide behind the `…` collapse pill, given the available container width.
 *
 * Pulled out as a separate module so we can pin the algorithm with mocked widths.
 * Tests in `path-pills-layout.test.ts`.
 *
 * This helper is correct only when given a real available width. The strip's
 * container (`.path-pills`) must fill its grid cell (`display: flex; width: 100%`),
 * NOT shrink-wrap (`inline-flex`): a shrink-wrapping container reports its content
 * width as the available width, which makes collapse self-reinforcing and abbreviates
 * paths with the column half-empty. See the `.path-pills` rule in `PathPills.svelte`.
 */

export interface Segment {
  label: string
  fullPath: string
}

export interface Layout {
  /** Pills rendered before the `…` collapse pill. */
  leading: Segment[]
  /** Pills hidden behind the `…` collapse pill (rendered in its tooltip). */
  collapsed: Segment[]
  /** Pills rendered after the `…` collapse pill. */
  trailing: Segment[]
}

export interface LayoutMetrics {
  /** Container width in pixels. `<= 0` keeps the layout uncollapsed (no measurement yet). */
  containerWidth: number
  /**
   * Measures the rendered text width of a label in pixels. `null` means measurement isn't
   * ready yet (e.g. pretext is still loading); the caller renders every segment until
   * measurement comes online, with overflow hidden as the safety net.
   */
  measureWidth: ((text: string) => number) | null
  /**
   * Width of the `/` separator between two pills (text width + the strip gap). Computed by
   * the caller because the separator font may differ from the pill font.
   */
  separatorWidth: number
  /**
   * Per-pill chrome (padding etc.) added on top of the measured text width. Sized at
   * 4 px (2 px padding each side) to match the real CSS so the strip doesn't collapse
   * when there's free space.
   */
  pillChrome: number
}

/** Sum of pill widths plus separators between them. */
function totalWidth(labels: string[], measure: (s: string) => number, chrome: number, sep: number): number {
  let w = 0
  for (let i = 0; i < labels.length; i++) {
    w += measure(labels[i]) + chrome
    if (i < labels.length - 1) w += sep
  }
  return w
}

/**
 * Schedules a sequence of re-measure callbacks to catch the CSS grid race where
 * `el.clientWidth` is read before the parent track has finished resolving its
 * width. Without this, the strip can render the full path first, then "gradually
 * wrap back to ellipses" once a late, smaller width read comes in. Re-reads the
 * width on the next animation frame and once more ~80 ms later. The two
 * follow-up reads cover two distinct settling moments:
 *
 *   1. First RAF: the grid's first layout pass usually completes here.
 *   2. ~80ms timeout: catches font loading / late style recalculations on
 *      lower-end hardware.
 *
 * Returns a `cancel` function the caller can invoke from a Svelte
 * `onDestroy` to clean up pending callbacks if the component unmounts before
 * the timers fire. Pure: takes a `read` closure and a callback so the helper
 * stays testable with a fake scheduler.
 */
export interface ReMeasureScheduler {
  /** Schedule a callback for the next animation frame. */
  requestFrame: (cb: () => void) => number
  /** Cancel a previously-scheduled frame. */
  cancelFrame: (id: number) => void
  /** Schedule a callback after `ms` milliseconds. */
  setTimer: (cb: () => void, ms: number) => ReturnType<typeof setTimeout>
  /** Cancel a previously-scheduled timer. */
  clearTimer: (id: ReturnType<typeof setTimeout>) => void
}

const DEFAULT_LATE_RE_MEASURE_MS = 80

export function scheduleStableWidthMeasure(
  read: () => void,
  scheduler: ReMeasureScheduler = {
    requestFrame: requestAnimationFrame,
    cancelFrame: cancelAnimationFrame,
    setTimer: setTimeout,
    clearTimer: clearTimeout,
  },
  lateMs: number = DEFAULT_LATE_RE_MEASURE_MS,
): () => void {
  // Re-measure on the next animation frame (catches grid track settling) and
  // again ~80ms later (catches font loads / late style recalculations).
  const rafId = scheduler.requestFrame(() => {
    read()
  })
  const timerId = scheduler.setTimer(() => {
    read()
  }, lateMs)
  return () => {
    scheduler.cancelFrame(rafId)
    scheduler.clearTimer(timerId)
  }
}

/**
 * Returns the layout that fits `containerWidth` best. Algorithm:
 *   1. If we don't have a measurer yet (or the container hasn't been laid out), show all
 *      segments and rely on CSS `overflow: hidden` to keep the strip from wrapping.
 *   2. Try fitting every segment. If it fits, render all of them — no `…` at all.
 *   3. Otherwise, pin the first segment plus the last segment (the most useful signals),
 *      drop everything in between behind the `…`, then add back trailing-edge segments
 *      one at a time while they still fit.
 *   4. If even first + `…` + last doesn't fit, drop the first too. If even `…` + last
 *      doesn't fit, drop the `…` and let CSS `overflow: hidden` swallow whatever's left.
 */
export function computePathPillsLayout(segments: Segment[], metrics: LayoutMetrics): Layout {
  const { containerWidth, measureWidth, separatorWidth, pillChrome } = metrics

  if (segments.length === 0) {
    return { leading: [], collapsed: [], trailing: [] }
  }
  if (segments.length <= 2 || !measureWidth || containerWidth <= 0) {
    return { leading: segments, collapsed: [], trailing: [] }
  }

  const measure = measureWidth

  // Phase 1: try all segments uncollapsed.
  const allLabels = segments.map((s) => s.label)
  if (totalWidth(allLabels, measure, pillChrome, separatorWidth) <= containerWidth) {
    return { leading: segments, collapsed: [], trailing: [] }
  }

  // Phase 2: pin first + last, ellipsis in the middle, re-add trailing-side segments.
  const first = segments[0]
  const last = segments[segments.length - 1]
  const middle = segments.slice(1, -1)
  const trailing: Segment[] = [last]
  const collapsed: Segment[] = [...middle]

  for (let i = middle.length - 1; i >= 0; i--) {
    const candidate = middle[i]
    const newTrailing = [candidate, ...trailing]
    const labels = [first.label, '…', ...newTrailing.map((s) => s.label)]
    if (totalWidth(labels, measure, pillChrome, separatorWidth) > containerWidth) break
    trailing.unshift(candidate)
    collapsed.pop()
  }

  const finalLabels = [first.label, '…', ...trailing.map((s) => s.label)]
  if (totalWidth(finalLabels, measure, pillChrome, separatorWidth) > containerWidth) {
    // Even first + `…` + last doesn't fit; drop first. The `…` stays so we still
    // signal "more here".
    const labelsNoFirst = ['…', ...trailing.map((s) => s.label)]
    if (totalWidth(labelsNoFirst, measure, pillChrome, separatorWidth) > containerWidth) {
      // Still doesn't fit. Keep only `last`; CSS overflow handles the rest.
      return { leading: [], collapsed: [first, ...collapsed], trailing }
    }
    return { leading: [], collapsed: [first, ...collapsed], trailing }
  }
  return { leading: [first], collapsed, trailing }
}
