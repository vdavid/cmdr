/** Tracks the width available for line text inside the given content ref: the
 *  scroll container's client width minus the `.line` row padding and the gutter
 *  (line number + its margin). Used by the viewer's height-map logic to wrap
 *  lines at the same width the browser wraps them.
 *
 *  Never measure the `.line-text` span itself: it's a flex item with no
 *  `flex-grow`, so it shrink-wraps to its own content. A file whose first
 *  rendered line is short ("# Cmdr") would yield a ~44px "available width",
 *  making the height map wrap every line at 44px and inflate the scroll
 *  height ~7x (blank space below ~line 60, end of the file unreachable).
 *  The `.line` row can't be measured either: in no-wrap mode the
 *  `.lines-container` is `max-content`, so the row is as wide as the WIDEST
 *  line. Deriving the width from the scroll container keeps the measurement
 *  correct in both wrap modes.
 *
 *  Exposed effects mirror the composable pattern used by `viewer-scroll.svelte.ts`:
 *  the page component wires them as `$effect`s so reactivity is preserved. */

interface TextWidthDeps {
  getContentRef: () => HTMLDivElement | undefined
  getVisibleLinesKey: () => unknown
}

/**
 * Width available for a line's text: the scroll container's content area minus
 * the `.line` row padding and the gutter (the `.line-number` outer width plus
 * its right margin). Clamped to 0 for degenerate inputs.
 */
export function computeAvailableTextWidth(args: {
  contentClientWidth: number
  linePaddingLeft: number
  linePaddingRight: number
  gutterOuterWidth: number
}): number {
  const { contentClientWidth, linePaddingLeft, linePaddingRight, gutterOuterWidth } = args
  return Math.max(0, contentClientWidth - linePaddingLeft - linePaddingRight - gutterOuterWidth)
}

export function createTextWidthTracker(deps: TextWidthDeps) {
  let textWidth = $state(0)

  function measure(el: HTMLElement) {
    const line = el.querySelector('.line')
    const lineNumber = el.querySelector('.line-number')
    if (!(line instanceof HTMLElement) || !(lineNumber instanceof HTMLElement)) return
    const lineStyle = getComputedStyle(line)
    const lineNumberStyle = getComputedStyle(lineNumber)
    const w = computeAvailableTextWidth({
      contentClientWidth: el.clientWidth,
      linePaddingLeft: Number.parseFloat(lineStyle.paddingLeft) || 0,
      linePaddingRight: Number.parseFloat(lineStyle.paddingRight) || 0,
      gutterOuterWidth:
        lineNumber.getBoundingClientRect().width + (Number.parseFloat(lineNumberStyle.marginRight) || 0),
    })
    if (w > 0 && Math.abs(w - textWidth) > 1) {
      textWidth = w
    }
  }

  /** Runs a `ResizeObserver` on the content ref so we re-measure on width changes.
   *  Returns a cleanup function for `$effect` to call when the ref changes or the
   *  component unmounts. Returns `undefined` if there's nothing to observe yet. */
  function runResizeEffect(): (() => void) | undefined {
    const ref = deps.getContentRef()
    if (!ref) return undefined
    const observer = new ResizeObserver(() => {
      measure(ref)
    })
    observer.observe(ref)
    // Initial measurement after mount
    requestAnimationFrame(() => {
      measure(ref)
    })
    return () => {
      observer.disconnect()
    }
  }

  /** Re-measures when visible lines first appear: `ResizeObserver` won't fire if the
   *  container size didn't change but the inner `.line` row just became present in
   *  the DOM. */
  function runVisibleLinesEffect() {
    void deps.getVisibleLinesKey()
    if (textWidth > 0) return
    requestAnimationFrame(() => {
      const ref = deps.getContentRef()
      if (!ref) return
      measure(ref)
    })
  }

  return {
    get textWidth() {
      return textWidth
    },
    runResizeEffect,
    runVisibleLinesEffect,
  }
}
