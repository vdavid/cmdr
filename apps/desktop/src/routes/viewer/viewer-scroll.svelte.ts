import { SvelteMap } from 'svelte/reactivity'
import { viewerGetLines, asViewerError } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { createLineHeightMap, getLineHeight } from './viewer-line-heights.svelte'
import { onDebouncedScaleChange } from '$lib/text-size.svelte'
import { pluralize } from '$lib/utils/pluralize'
import { dependOn } from '$lib/utils/reactivity'
import { ensureVisibleOffset, recenterOffset } from './viewer-search-scroll'
import { caretRectFor, measureColumnWidth } from './viewer-pointer'
import { EOF_LINE, type LineOffset } from './selection.svelte'

const log = getAppLogger('viewer')

const BUFFER_LINES = 50
const FETCH_BATCH = 500
// WebKit caps element height at ~2^25 px (33.5M). Stay well below to avoid scroll cutoff.
const MAX_SCROLL_HEIGHT = 30_000_000
const FETCH_DEBOUNCE_MS = 100
// A full DOM re-measure of every line (on a width change) is ~70 ms, so it can't
// run on every ResizeObserver frame during a live window drag. Debounce to the
// resize-settle, the same way the text-size slider path debounces (see
// `onDebouncedScaleChange`). Between drag and settle the CSS re-wraps live; only
// the scroll geometry lags a beat, then snaps into place.
const REFLOW_DEBOUNCE_MS = 150

interface ScrollDeps {
  getSessionId: () => string
  getTotalLines: () => number | null
  setTotalLines: (v: number) => void
  getEstimatedLines: () => number
  getBackendType: () => 'fullLoad' | 'byteSeek' | 'lineIndex'
  onTimeoutError: () => void
  getAllLines: () => string[] | null
  getTextWidth: () => number
}

export { getLineHeight, MAX_SCROLL_HEIGHT }

export function createViewerScroll(deps: ScrollDeps) {
  const lineCache = new SvelteMap<number, string>()
  const heightMap = createLineHeightMap()

  let scrollTop = $state(0)
  let viewportHeight = $state(600)
  let contentRef: HTMLDivElement | undefined = $state()
  let containerRef: HTMLElement | undefined = $state()
  let linesContainerRef: HTMLDivElement | undefined = $state()

  let contentWidth = $state(0)

  let wordWrap = $state(false)
  let avgWrappedLineHeight = $state(getLineHeight())
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- wordWrap is reactive $state
  const effectiveLineHeight = $derived(wordWrap ? avgWrappedLineHeight : getLineHeight())

  let fetchDebounceTimer: ReturnType<typeof setTimeout> | undefined
  let currentFetchId = 0

  function estimatedTotalLines(): number {
    const totalLines = deps.getTotalLines()
    if (totalLines !== null) return totalLines
    return deps.getEstimatedLines()
  }

  const scrollScale = $derived.by(() => {
    const totalHeight = heightMap.ready ? heightMap.getTotalHeight() : estimatedTotalLines() * effectiveLineHeight
    return totalHeight > MAX_SCROLL_HEIGHT ? MAX_SCROLL_HEIGHT / totalHeight : 1
  })
  const scrollLineHeight = $derived(effectiveLineHeight * scrollScale)

  const visibleFrom = $derived.by(() => {
    if (heightMap.ready) {
      const unscaledY = scrollScale < 1 ? scrollTop / scrollScale : scrollTop
      return Math.max(0, heightMap.getLineAtPosition(unscaledY) - BUFFER_LINES)
    }
    return Math.max(0, Math.floor(scrollTop / scrollLineHeight) - BUFFER_LINES)
  })

  const visibleTo = $derived.by(() => {
    if (heightMap.ready) {
      const unscaledY = scrollScale < 1 ? (scrollTop + viewportHeight) / scrollScale : scrollTop + viewportHeight
      return Math.min(estimatedTotalLines(), heightMap.getLineAtPosition(unscaledY) + BUFFER_LINES)
    }
    return Math.min(estimatedTotalLines(), Math.ceil((scrollTop + viewportHeight) / scrollLineHeight) + BUFFER_LINES)
  })

  const spacerHeight = $derived(
    heightMap.ready ? heightMap.getTotalHeight() * scrollScale : estimatedTotalLines() * scrollLineHeight,
  )

  const linesOffset = $derived(
    heightMap.ready ? heightMap.getLineTop(visibleFrom) * scrollScale : visibleFrom * scrollLineHeight,
  )

  /** One past the last line the template draws. `visibleTo` alone can overshoot the file. */
  const renderedTo = $derived(Math.min(visibleTo, estimatedTotalLines()))

  const visibleLines = $derived(getVisibleLines())
  const gutterWidth = $derived(String(estimatedTotalLines()).length)

  function getVisibleLines(): Array<{ lineNumber: number; text: string }> {
    const result: Array<{ lineNumber: number; text: string }> = []
    for (let i = visibleFrom; i < renderedTo; i++) {
      result.push({ lineNumber: i, text: lineCache.get(i) ?? '' })
    }
    return result
  }

  /**
   * The text the template SHOWS for a line, which is what the caret motion model has to
   * reason about, or `undefined` when no row is drawn for it yet.
   *
   * Inside the rendered range a cache miss draws as an empty row (`getVisibleLines`
   * applies the same `?? ''`), so a caller reading the cache directly would disagree with
   * the user's screen about which lines exist. That divergence is fatal for a file ending
   * in a newline on the `lineIndex` backend: it counts a last line `viewer_get_lines`
   * never emits, and ⌘⇧Down would ask for it forever.
   *
   * OUTSIDE the range `undefined` keeps its other meaning: not fetched yet, scroll and
   * retry. `moveFocus` turns that into `{ focus: null, targetLine }`, and the keyboard's
   * scroll is what pulls the line in so the next press lands. Widening the `''` past the
   * rendered range would break that two-press flow, and invent an offset for a line
   * nobody has seen.
   */
  function renderedLineText(line: number): string | undefined {
    const cached = lineCache.get(line)
    if (cached !== undefined) return cached
    return line >= visibleFrom && line < renderedTo ? '' : undefined
  }

  /** Returns the scaled Y offset for line n. Used by search for scroll-to-match. */
  function getLineTop(n: number): number {
    if (heightMap.ready) {
      return heightMap.getLineTop(n) * scrollScale
    }
    return n * scrollLineHeight
  }

  /** Returns the line at the current viewport top, using the height map (not the DOM buffer). */
  function getAnchorLine(): number {
    const unscaledY = scrollScale < 1 ? scrollTop / scrollScale : scrollTop
    return heightMap.getLineAtPosition(unscaledY)
  }

  function needsFetch(from: number, to: number): boolean {
    const samplesToCheck = [from, Math.floor((from + to) / 2), to - 1]
    for (const line of samplesToCheck) {
      if (line >= 0 && !lineCache.has(line)) {
        return true
      }
    }
    return false
  }

  function scheduleFetch(from: number, to: number) {
    if (fetchDebounceTimer) {
      clearTimeout(fetchDebounceTimer)
    }
    fetchDebounceTimer = setTimeout(() => {
      void fetchLines(from, to)
    }, FETCH_DEBOUNCE_MS)
  }

  function updateTotalLines(newTotal: number) {
    const oldEstimate = estimatedTotalLines()
    if (!contentRef || oldEstimate === 0 || newTotal === oldEstimate) {
      deps.setTotalLines(newTotal)
      return
    }
    const oldHeight = Math.min(oldEstimate * effectiveLineHeight, MAX_SCROLL_HEIGHT)
    const scrollFraction = contentRef.scrollTop / oldHeight
    log.debug('totalLines changed: {oldEstimate} -> {newTotal}, preserving scroll fraction {fraction}', {
      oldEstimate,
      newTotal,
      fraction: scrollFraction.toFixed(3),
    })
    deps.setTotalLines(newTotal)
    const newHeight = Math.min(newTotal * effectiveLineHeight, MAX_SCROLL_HEIGHT)
    const newScrollTop = Math.round(scrollFraction * newHeight)
    const ref = contentRef
    requestAnimationFrame(() => {
      ref.scrollTop = newScrollTop
    })
  }

  async function fetchLines(from: number, to: number) {
    const sessionId = deps.getSessionId()
    if (!sessionId) return

    const fetchId = ++currentFetchId

    try {
      const fetchFrom = Math.max(0, from - BUFFER_LINES)
      const fetchCount = Math.min(FETCH_BATCH, to - fetchFrom + BUFFER_LINES * 2)

      const totalLines = deps.getTotalLines()
      const supportsLineSeek = totalLines !== null
      const seekType = supportsLineSeek ? 'line' : 'fraction'
      // A 0 estimate would make the fraction division NaN (0/0) or Infinity (>0/0); both
      // serialize to JSON null, which the Rust f64 `targetValue` rejects. With no line count
      // to go on, seek to the start of the file (fraction 0).
      const estimated = estimatedTotalLines()
      const seekValue = supportsLineSeek ? fetchFrom : estimated > 0 ? fetchFrom / estimated : 0

      log.debug('fetchLines[{fetchId}]: requesting {seekType}={seekValue} count={fetchCount}', {
        fetchId,
        seekType,
        seekValue,
        fetchCount,
      })

      const chunk = await viewerGetLines(sessionId, seekType, seekValue, fetchCount)

      if (fetchId !== currentFetchId) {
        log.debug('fetchLines[{fetchId}]: discarding stale response (current={currentFetchId})', {
          fetchId,
          currentFetchId,
        })
        return
      }

      const cacheStartLine = seekType === 'fraction' ? fetchFrom : chunk.firstLineNumber

      log.debug(
        'fetchLines[{fetchId}]: received {lineCount} {linesNoun}, backend says firstLine={firstLine}, caching at {cacheStart}',
        {
          fetchId,
          lineCount: chunk.lines.length,
          linesNoun: pluralize(chunk.lines.length, 'line'),
          firstLine: chunk.firstLineNumber,
          cacheStart: cacheStartLine,
        },
      )

      for (let i = 0; i < chunk.lines.length; i++) {
        lineCache.set(cacheStartLine + i, chunk.lines[i])
      }

      if (chunk.totalLines !== null && chunk.totalLines !== deps.getTotalLines()) {
        updateTotalLines(chunk.totalLines)
      }
    } catch (e) {
      if (fetchId === currentFetchId) {
        // `viewerGetLines` throws the backend's typed `ViewerError` with its
        // fields copied onto the Error, so the timeout is a VARIANT, never a
        // flag beside a sentence.
        const kind = asViewerError(e)?.kind
        if (kind === 'timedOut') {
          deps.onTimeoutError()
          log.error('fetchLines[{fetchId}]: timed out', { fetchId })
        } else {
          log.error("fetchLines[{fetchId}]: didn't come back ({reason})", { fetchId, reason: kind ?? String(e) })
        }
      }
    }
  }

  function handleScroll() {
    if (contentRef) {
      scrollTop = contentRef.scrollTop
      viewportHeight = contentRef.clientHeight
    }
  }

  function scrollByLines(lines: number) {
    if (!contentRef) return

    if (heightMap.ready) {
      // Find current line at the top of viewport, move by `lines` lines, look up new position
      const unscaledY = scrollScale < 1 ? contentRef.scrollTop / scrollScale : contentRef.scrollTop
      const currentLine = heightMap.getLineAtPosition(unscaledY)
      const targetLine = Math.max(0, Math.min(estimatedTotalLines() - 1, currentLine + lines))
      contentRef.scrollTop = Math.max(0, heightMap.getLineTop(targetLine) * scrollScale)
    } else {
      contentRef.scrollTop = Math.max(0, contentRef.scrollTop + lines * scrollLineHeight)
    }
  }

  function scrollByPages(pages: number) {
    if (!contentRef) return

    if (heightMap.ready) {
      // Move by approximately one viewport worth of content
      const pageHeight = contentRef.clientHeight
      const newScrollTop = Math.max(0, contentRef.scrollTop + pages * pageHeight)
      contentRef.scrollTop = newScrollTop
    } else {
      const linesPerPage = Math.floor(contentRef.clientHeight / effectiveLineHeight) - 1
      contentRef.scrollTop = Math.max(0, contentRef.scrollTop + pages * linesPerPage * scrollLineHeight)
    }
  }

  function scrollToStart() {
    if (contentRef) {
      contentRef.scrollTop = 0
    }
  }

  function scrollToEnd() {
    if (contentRef) {
      contentRef.scrollTop = contentRef.scrollHeight - contentRef.clientHeight
    }
  }

  /**
   * The rendered height of line `n`, in the same scaled space `getLineTop` speaks. The
   * height map holds the real per-line height once it's ready (a wrapped line is several
   * rows tall); before that every line is one row.
   */
  function lineHeightAt(n: number): number {
    if (heightMap.ready) {
      const measured = (heightMap.getLineTop(n + 1) - heightMap.getLineTop(n)) * scrollScale
      if (measured > 0) return measured
    }
    return scrollLineHeight
  }

  /**
   * Scrolls line `n` just into view, with one line of breathing room, and leaves an
   * already-visible line alone. Drives keyboard selection extension: every extend press
   * calls this, including the one whose target line isn't cached yet, because the scroll
   * is what pulls the line into the render window and triggers its fetch.
   *
   * ❌ The `EOF_LINE` branch is NOT redundant, however much the arithmetic below looks
   * like it would cope. `⌘⇧Down` on a file with no line index reports the sentinel as its
   * target, and it only survives `getLineTop` today through integer overflow plus the
   * browser clamping an absurd `scrollTop` — don't lean on that. Worse, the obvious later
   * tidy-up `Math.min(n, totalLines - 1)` yields `NaN` on exactly this branch (the line
   * count is `null` precisely when the sentinel appears), and `scrollTop = NaN` throws the
   * view to the TOP of the file. Branching here keeps that visible to whoever reaches for
   * the clamp.
   */
  function ensureLineVisible(n: number) {
    if (!contentRef) return
    if (n === EOF_LINE) {
      scrollToEnd()
      return
    }
    const next = ensureVisibleOffset({
      lineTop: getLineTop(n),
      lineHeight: lineHeightAt(n),
      scrollTop: contentRef.scrollTop,
      viewportHeight: contentRef.clientHeight,
      margin: scrollLineHeight,
    })
    if (next !== null) contentRef.scrollTop = next
  }

  /**
   * Brings the character at `point` into view horizontally, the way search does for a
   * match: measure the real rect, recentre against the content box, set `scrollLeft`.
   * Without it, repeated Shift+Right on a long unwrapped line walks the focus past the
   * right edge with nothing following it. Word wrap has no horizontal overflow, so it's
   * a no-op there.
   */
  function ensureColumnVisible(point: LineOffset) {
    if (!contentRef || wordWrap) return
    const caret = caretRectFor(contentRef, point)
    if (caret === null) return
    const view = contentRef.getBoundingClientRect()
    const left = recenterOffset({
      markStart: caret.left,
      markEnd: caret.right,
      viewStart: view.left,
      viewEnd: view.right,
      currentScroll: contentRef.scrollLeft,
    })
    if (left !== null && Math.abs(left - contentRef.scrollLeft) > 2) contentRef.scrollLeft = left
  }

  /**
   * One column's advance width, measured once off a rendered row and dropped when the
   * text scale settles (the only thing that changes it). `null` until a row with text
   * exists, which is also when there's nothing to scroll.
   */
  let columnWidth: number | null = null
  function scrollByColumns(columns: number) {
    if (!contentRef || wordWrap) return
    columnWidth ??= measureColumnWidth(contentRef)
    if (columnWidth === null) return
    contentRef.scrollLeft = Math.max(0, contentRef.scrollLeft + columns * columnWidth)
  }

  function runFetchEffect() {
    const from = visibleFrom
    const to = visibleTo
    const sessionId = deps.getSessionId()
    if (sessionId && needsFetch(from, to)) {
      scheduleFetch(from, to)
    }
  }

  /**
   * Force a fetch of the current visible range, bypassing the cache check. Used
   * when the cache was deliberately invalidated (encoding switch) so the next
   * render shows freshly-decoded lines without waiting for the user to scroll.
   */
  function fetchVisibleNow() {
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    void fetchLines(visibleFrom, visibleTo)
  }

  function runContentWidthEffect() {
    if (wordWrap) return
    dependOn(visibleLines)
    const rafId = requestAnimationFrame(() => {
      if (linesContainerRef) {
        const w = linesContainerRef.scrollWidth
        if (w > contentWidth) {
          contentWidth = w
        }
      }
    })
    return () => {
      cancelAnimationFrame(rafId)
    }
  }

  function runWrappedLineHeightEffect() {
    if (!wordWrap) return

    if (heightMap.ready) return // Height map replaces DOM-based averaging
    dependOn(scrollTop)
    const rafId = requestAnimationFrame(() => {
      if (!linesContainerRef) return
      const lineCount = linesContainerRef.children.length
      if (lineCount === 0) return
      const renderedHeight = linesContainerRef.getBoundingClientRect().height
      if (renderedHeight > 0) {
        const measured = renderedHeight / lineCount
        if (Math.abs(measured - avgWrappedLineHeight) > 1) {
          avgWrappedLineHeight = measured
        }
      }
    })
    return () => {
      cancelAnimationFrame(rafId)
    }
  }

  let prevScrollLineHeight = getLineHeight()
  function runScrollCompensationEffect() {
    const newSLH = scrollLineHeight
    if (!contentRef || prevScrollLineHeight === newSLH) {
      prevScrollLineHeight = newSLH
      return
    }

    if (heightMap.ready) {
      // With height map: find the line at current viewport top, look up its new position
      const anchorLine = getAnchorLine()
      contentRef.scrollTop = heightMap.getLineTop(anchorLine) * scrollScale
    } else {
      // Without height map: scale proportionally (existing behavior)
      const ratio = newSLH / prevScrollLineHeight
      contentRef.scrollTop = Math.round(contentRef.scrollTop * ratio)
    }
    prevScrollLineHeight = newSLH
  }

  /**
   * Watches wordWrap + getAllLines + getTextWidth and triggers height map preparation
   * when all conditions are met (word wrap on, fullLoad lines available, width known).
   * Does NOT re-prepare if the height map is already ready; width changes are handled
   * by runHeightMapReflowEffect via reflow() instead.
   */
  function runHeightMapInitEffect() {
    // Read reactive deps to establish tracking

    const ww = wordWrap
    const lines = deps.getAllLines()
    const textWidth = deps.getTextWidth()

    if (!ww) {
      heightMap.cancel()
      return
    }

    if (heightMap.ready) return // Width changes handled by reflow, not re-preparation

    if (lines !== null && lines.length > 0 && textWidth > 0) {
      heightMap.prepareLines(lines, textWidth)
    }
  }

  /**
   * Re-measures all line heights at the new width and preserves the scroll
   * fraction across the change. Uses the container's real `scrollHeight` (the
   * rendered spacer) for the fraction rather than a derived value, and applies
   * the new position after the spacer DOM has settled.
   */
  function doReflow(newWidth: number) {
    if (!heightMap.ready || !contentRef) return
    const oldScrollHeight = contentRef.scrollHeight
    const fraction = oldScrollHeight > 0 ? contentRef.scrollTop / oldScrollHeight : 0
    heightMap.reflow(newWidth)
    const ref = contentRef
    requestAnimationFrame(() => {
      ref.scrollTop = fraction * ref.scrollHeight
    })
  }

  /**
   * Watches textWidth and re-measures the height map when it changes. Debounced
   * to the resize-settle: a full DOM re-measure is ~70 ms, too slow to run on
   * every ResizeObserver frame during a live window drag.
   */
  let prevTextWidth = 0
  let reflowDebounceTimer: ReturnType<typeof setTimeout> | undefined
  function runHeightMapReflowEffect() {
    const textWidth = deps.getTextWidth()

    // Only react to actual textWidth changes. The prevTextWidth guard prevents this
    // effect from re-running due to other reactive dependencies (heightMap.ready, version).
    if (textWidth <= 0 || textWidth === prevTextWidth) return
    if (!heightMap.ready) {
      prevTextWidth = textWidth
      return
    }
    prevTextWidth = textWidth

    if (reflowDebounceTimer) clearTimeout(reflowDebounceTimer)
    reflowDebounceTimer = setTimeout(() => {
      doReflow(textWidth)
    }, REFLOW_DEBOUNCE_MS)
  }

  // After the user settles on a new text scale, the line height the height
  // map baked in is stale. Re-layout (without changing width) so wrapped-line
  // heights match the new font. This runs inside the same debounced "settled"
  // event the file-list column-width path uses, so we don't thrash mid-drag.
  const unsubscribeScaleChange = onDebouncedScaleChange(() => {
    heightMap.recomputeForLineHeightChange()
    // A new font size means a new column advance; re-measure it on the next press.
    columnWidth = null
  })

  function destroy() {
    if (fetchDebounceTimer) clearTimeout(fetchDebounceTimer)
    if (reflowDebounceTimer) clearTimeout(reflowDebounceTimer)
    heightMap.cancel()
    unsubscribeScaleChange()
  }

  return {
    lineCache,
    get scrollTop() {
      return scrollTop
    },
    get viewportHeight() {
      return viewportHeight
    },
    get contentRef() {
      return contentRef
    },
    set contentRef(v: HTMLDivElement | undefined) {
      contentRef = v
    },
    get containerRef() {
      return containerRef
    },
    set containerRef(v: HTMLElement | undefined) {
      containerRef = v
    },
    get linesContainerRef() {
      return linesContainerRef
    },
    set linesContainerRef(v: HTMLDivElement | undefined) {
      linesContainerRef = v
    },
    get contentWidth() {
      return contentWidth
    },
    set contentWidth(v: number) {
      contentWidth = v
    },
    get wordWrap() {
      return wordWrap
    },
    set wordWrap(v: boolean) {
      wordWrap = v
    },
    get effectiveLineHeight() {
      return effectiveLineHeight
    },
    get scrollLineHeight() {
      return scrollLineHeight
    },
    get visibleFrom() {
      return visibleFrom
    },
    get visibleLines() {
      return visibleLines
    },
    get gutterWidth() {
      return gutterWidth
    },
    get spacerHeight() {
      return spacerHeight
    },
    get linesOffset() {
      return linesOffset
    },
    get heightMapReady() {
      return heightMap.ready
    },
    estimatedTotalLines,
    renderedLineText,
    getLineTop,
    handleScroll,
    scrollByLines,
    scrollByPages,
    scrollToStart,
    scrollToEnd,
    scrollByColumns,
    ensureLineVisible,
    ensureColumnVisible,
    runFetchEffect,
    fetchVisibleNow,
    runContentWidthEffect,
    runWrappedLineHeightEffect,
    runScrollCompensationEffect,
    runHeightMapInitEffect,
    runHeightMapReflowEffect,
    destroy,
  }
}
