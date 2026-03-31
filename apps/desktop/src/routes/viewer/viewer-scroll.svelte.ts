import { SvelteMap } from 'svelte/reactivity'
import { viewerGetLines, isIpcError } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('viewer')

const LINE_HEIGHT = 18
const BUFFER_LINES = 50
const FETCH_BATCH = 500
// WebKit caps element height at ~2^25 px (33.5M). Stay well below to avoid scroll cutoff.
const MAX_SCROLL_HEIGHT = 30_000_000
const FETCH_DEBOUNCE_MS = 100

interface ScrollDeps {
  getSessionId: () => string
  getTotalLines: () => number | null
  setTotalLines: (v: number) => void
  getEstimatedLines: () => number
  getBackendType: () => 'fullLoad' | 'byteSeek' | 'lineIndex'
  onTimeoutError: () => void
}

export { LINE_HEIGHT, MAX_SCROLL_HEIGHT }

export function createViewerScroll(deps: ScrollDeps) {
  const lineCache = new SvelteMap<number, string>()

  let scrollTop = $state(0)
  let viewportHeight = $state(600)
  let contentRef: HTMLDivElement | undefined = $state()
  let containerRef: HTMLDivElement | undefined = $state()
  let linesContainerRef: HTMLDivElement | undefined = $state()

  let contentWidth = $state(0)

  let wordWrap = $state(false)
  let avgWrappedLineHeight = $state(LINE_HEIGHT)
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- wordWrap is reactive $state
  const effectiveLineHeight = $derived(wordWrap ? avgWrappedLineHeight : LINE_HEIGHT)

  let fetchDebounceTimer: ReturnType<typeof setTimeout> | undefined
  let currentFetchId = 0

  function estimatedTotalLines(): number {
    const totalLines = deps.getTotalLines()
    if (totalLines !== null) return totalLines
    return deps.getEstimatedLines()
  }

  const scrollScale = $derived.by(() => {
    const fullHeight = estimatedTotalLines() * effectiveLineHeight
    return fullHeight > MAX_SCROLL_HEIGHT ? MAX_SCROLL_HEIGHT / fullHeight : 1
  })
  const scrollLineHeight = $derived(effectiveLineHeight * scrollScale)

  const visibleFrom = $derived(Math.max(0, Math.floor(scrollTop / scrollLineHeight) - BUFFER_LINES))
  const visibleTo = $derived(
    Math.min(estimatedTotalLines(), Math.ceil((scrollTop + viewportHeight) / scrollLineHeight) + BUFFER_LINES),
  )
  const visibleLines = $derived(getVisibleLines())
  const gutterWidth = $derived(String(estimatedTotalLines()).length)

  function getVisibleLines(): Array<{ lineNumber: number; text: string }> {
    const result: Array<{ lineNumber: number; text: string }> = []
    const end = Math.min(visibleTo, estimatedTotalLines())
    for (let i = visibleFrom; i < end; i++) {
      result.push({ lineNumber: i, text: lineCache.get(i) ?? '' })
    }
    return result
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
      const seekValue = supportsLineSeek ? fetchFrom : fetchFrom / estimatedTotalLines()

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
        'fetchLines[{fetchId}]: received {lineCount} lines, backend says firstLine={firstLine}, caching at {cacheStart}',
        {
          fetchId,
          lineCount: chunk.lines.length,
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
        if (isIpcError(e) && e.timedOut) {
          deps.onTimeoutError()
          log.error('fetchLines[{fetchId}]: timed out', { fetchId })
        } else {
          const msg = isIpcError(e) ? e.message : String(e)
          log.error('fetchLines[{fetchId}]: failed with error {error}', { fetchId, error: msg })
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
    if (contentRef) {
      contentRef.scrollTop = Math.max(0, contentRef.scrollTop + lines * scrollLineHeight)
    }
  }

  function scrollByPages(pages: number) {
    if (contentRef) {
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

  function runFetchEffect() {
    const from = visibleFrom
    const to = visibleTo
    const sessionId = deps.getSessionId()
    if (sessionId && needsFetch(from, to)) {
      scheduleFetch(from, to)
    }
  }

  function runContentWidthEffect() {
    if (wordWrap) return
    void visibleLines
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
    void scrollTop
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

  let prevScrollLineHeight = LINE_HEIGHT
  function runScrollCompensationEffect() {
    const newSLH = scrollLineHeight
    if (!contentRef || prevScrollLineHeight === newSLH) {
      prevScrollLineHeight = newSLH
      return
    }
    const ratio = newSLH / prevScrollLineHeight
    contentRef.scrollTop = Math.round(contentRef.scrollTop * ratio)
    prevScrollLineHeight = newSLH
  }

  function destroy() {
    if (fetchDebounceTimer) clearTimeout(fetchDebounceTimer)
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
    set containerRef(v: HTMLDivElement | undefined) {
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
    estimatedTotalLines,
    handleScroll,
    scrollByLines,
    scrollByPages,
    scrollToStart,
    scrollToEnd,
    runFetchEffect,
    runContentWidthEffect,
    runWrappedLineHeightEffect,
    runScrollCompensationEffect,
    destroy,
  }
}
