/** Tracks the rendered width of a `.line-text` element inside the given content ref.
 *  Used by the viewer's height-map logic to wrap lines correctly at the current width.
 *
 *  Exposed effects mirror the composable pattern used by `viewer-scroll.svelte.ts`:
 *  the page component wires them as `$effect`s so reactivity is preserved. */

interface TextWidthDeps {
  getContentRef: () => HTMLDivElement | undefined
  getVisibleLinesKey: () => unknown
}

export function createTextWidthTracker(deps: TextWidthDeps) {
  let textWidth = $state(0)

  function measure(el: HTMLElement) {
    const lineText = el.querySelector('.line-text')
    if (!lineText) return
    const w = lineText.getBoundingClientRect().width
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
   *  container size didn't change but the inner `.line-text` element just became
   *  present in the DOM. */
  function runVisibleLinesEffect() {
    void deps.getVisibleLinesKey()
    if (textWidth > 0) return
    requestAnimationFrame(() => {
      const ref = deps.getContentRef()
      if (!ref) return
      const lineText = ref.querySelector('.line-text')
      if (!lineText) return
      const w = lineText.getBoundingClientRect().width
      if (w > 0) textWidth = w
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
