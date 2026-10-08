import { tick } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { type ViewerRow } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { createRowHeightMap, getLineHeight } from './viewer-line-heights.svelte'
import { createViewerRowFetch, FETCH_BATCH } from './viewer-row-fetch.svelte'
import { onDebouncedScaleChange } from '$lib/text-size.svelte'
import { pluralize } from '$lib/utils/pluralize'
import { dependOn } from '$lib/utils/reactivity'
import { ensureVisibleOffset, recenterOffset } from './viewer-search-scroll'
import { caretRectFor, measureColumnWidth } from './viewer-pointer'
import { EOF_ROW, type RowOffset } from './selection.svelte'
import { isScrolledToEnd } from './viewer-tail-follow'

const log = getAppLogger('viewer')

/**
 * How much content to keep drawn on each side of the viewport, in CSS pixels, and the
 * row counts that budget is clamped between.
 *
 * ❗ PIXELS, not rows. 900 px is exactly the 50 rows the viewer has always buffered at
 * the default 18 px line height, so ordinary files are unaffected. The difference shows
 * with word wrap on, where one row can be hundreds of pixels tall: 50 SUCH rows would
 * paint hundreds of thousands of pixels for a 600 px viewport, and the DOM height map
 * that would otherwise catch it only ever engages on `fullLoad` files.
 */
const BUFFER_PX = 900
const BUFFER_ROWS_MAX = 50
const BUFFER_ROWS_MIN = 2

/**
 * Rows this far outside the rendered window survive eviction: one fetch batch on either
 * side, so ordinary scrolling never re-fetches what it just dropped.
 */
const CACHE_KEEP_MARGIN = FETCH_BATCH

/**
 * Cache size that starts an eviction pass. Below it the map isn't worth walking, and the
 * gap to the keep window gives the pass hysteresis: it runs rarely and drops a lot.
 *
 * The number that matters is bytes, not rows. An ordinary row is a few dozen bytes, but
 * a row inside a long line runs to ~40 KB, which puts the ceiling near 80 MB of text held
 * in the renderer. The alternative was unbounded: the cache was cleared only on open,
 * reload, and encoding change, so scrolling through 1% of a 50 GB file parked ~500 MB in
 * the webview with nothing drawing it.
 */
export const CACHE_EVICT_ABOVE = FETCH_BATCH * 4

// WebKit caps element height at ~2^25 px (33.5M). Stay well below to avoid scroll cutoff.
const MAX_SCROLL_HEIGHT = 30_000_000
// A full DOM re-measure of every line (on a width change) is ~70 ms, so it can't
// run on every ResizeObserver frame during a live window drag. Debounce to the
// resize-settle, the same way the text-size slider path debounces (see
// `onDebouncedScaleChange`). Between drag and settle the CSS re-wraps live; only
// the scroll geometry lags a beat, then snaps into place.
const REFLOW_DEBOUNCE_MS = 150

/**
 * One cached row: what the backend served for it.
 *
 * ❗ `continues` and `lineNumber` are FACTS the backend states, ❌ never inferred here.
 * `continues` says Cmdr ended the row at a segment boundary rather than at a newline the
 * file holds, which drives the continuation marker and zeroes the row's delimiter byte.
 * `lineNumber` is the 0-based physical line the row STARTS, or `null` on a continuation
 * row, which is what lets the gutter print a number once per line.
 */
export interface CachedRow {
  text: string
  continues: boolean
  lineNumber: number | null
}

interface ScrollDeps {
  getSessionId: () => string
  /** Counted rows, or `null` while only an estimate exists. */
  getTotalRows: () => number | null
  setTotalRows: (v: number) => void
  getEstimatedRows: () => number
  getBackendType: () => 'fullLoad' | 'byteSeek' | 'lineIndex'
  onTimeoutError: () => void
  /** Every row's text, in order, for the height map. `fullLoad` only. */
  getAllRowTexts: () => string[] | null
  getTextWidth: () => number
}

export { getLineHeight, MAX_SCROLL_HEIGHT }

/** How many rows of `lineHeight` fit in the off-screen pixel buffer, within its clamps. */
export function bufferRows(lineHeight: number): number {
  if (!(lineHeight > 0)) return BUFFER_ROWS_MAX
  return Math.max(BUFFER_ROWS_MIN, Math.min(BUFFER_ROWS_MAX, Math.ceil(BUFFER_PX / lineHeight)))
}

/**
 * The half-open ROW range `[from, to)` to draw for a viewport, sized in PIXELS: a
 * viewport's worth of content plus the off-screen buffer, whatever the rows contain.
 * Both ends clamp to the file, so `from <= to` always holds. (When the row count lands
 * below the current scroll position, a byte-seek estimate replaced by the real index,
 * `scrollTop` points past the end for a beat; an unclamped `from` then overtook `to` and
 * the fetch asked for a negative count, ERR-VDVHD.)
 *
 * ❗ `scrollScale` converts the scroll position and nothing else. Past ~1.6M rows the
 * spacer is squeezed to stay under WebKit's element-height cap, so `scrollTop` is in
 * squeezed pixels while the viewport still shows real ones. Dividing the viewport height
 * by the scale as well is how a huge file ends up drawing tens of thousands of rows at
 * once.
 */
export function renderWindowRows({
  scrollTop,
  viewportHeight,
  scrollScale,
  lineHeight,
  totalRows,
}: {
  scrollTop: number
  viewportHeight: number
  scrollScale: number
  lineHeight: number
  totalRows: number
}): { from: number; to: number } {
  const height = Math.max(1, lineHeight)
  const top = scrollScale < 1 ? scrollTop / scrollScale : scrollTop
  const buffer = bufferRows(height)
  return {
    from: Math.min(totalRows, Math.max(0, Math.floor(top / height) - buffer)),
    to: Math.min(totalRows, Math.ceil((top + viewportHeight) / height) + buffer),
  }
}

/**
 * The cached row indexes outside `keep`, which is half-open `[from, to)`.
 *
 * ❗ The caller owes it a window that covers everything on screen plus its fetch buffer.
 * Evicting a row the render window still wants draws a blank row, and the fetch effect
 * pulls it straight back, so a too-tight window is a churn machine, not just a glitch.
 */
export function rowsToEvict(cached: Iterable<number>, keep: { from: number; to: number }): number[] {
  const evictable: number[] = []
  for (const row of cached) {
    if (row < keep.from || row >= keep.to) evictable.push(row)
  }
  return evictable
}

export function createViewerScroll(deps: ScrollDeps) {
  const rowCache = new SvelteMap<number, CachedRow>()
  const heightMap = createRowHeightMap()

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

  function estimatedTotalRows(): number {
    const totalRows = deps.getTotalRows()
    if (totalRows !== null) return totalRows
    return deps.getEstimatedRows()
  }

  const scrollScale = $derived.by(() => {
    const totalHeight = heightMap.ready ? heightMap.getTotalHeight() : estimatedTotalRows() * effectiveLineHeight
    return totalHeight > MAX_SCROLL_HEIGHT ? MAX_SCROLL_HEIGHT / totalHeight : 1
  })
  const scrollLineHeight = $derived(effectiveLineHeight * scrollScale)

  /**
   * The rendered range, from measured heights when the height map is ready and from the
   * (also measured) average row height otherwise. Both paths spend a fixed pixel budget
   * rather than a fixed number of rows: `BUFFER_PX` above and below the viewport.
   */
  const renderWindow = $derived.by(() => {
    const total = estimatedTotalRows()
    if (heightMap.ready) {
      const unscaledY = scrollScale < 1 ? scrollTop / scrollScale : scrollTop
      return {
        from: Math.min(total, Math.max(0, heightMap.getRowAtPosition(Math.max(0, unscaledY - BUFFER_PX)))),
        to: Math.min(total, heightMap.getRowAtPosition(unscaledY + viewportHeight + BUFFER_PX) + 1),
      }
    }
    return renderWindowRows({
      scrollTop,
      viewportHeight,
      scrollScale,
      lineHeight: effectiveLineHeight,
      totalRows: total,
    })
  })

  const visibleFrom = $derived(renderWindow.from)
  const visibleTo = $derived(renderWindow.to)

  const spacerHeight = $derived(
    heightMap.ready ? heightMap.getTotalHeight() * scrollScale : estimatedTotalRows() * scrollLineHeight,
  )

  const rowsOffset = $derived(
    heightMap.ready ? heightMap.getRowTop(visibleFrom) * scrollScale : visibleFrom * scrollLineHeight,
  )

  /** One past the last row the template draws. `visibleTo` alone can overshoot the file. */
  const renderedTo = $derived(Math.min(visibleTo, estimatedTotalRows()))

  const visibleRows = $derived(getVisibleRows())
  /**
   * Gutter width in `ch`, sized off the ROW total. A physical line number never exceeds
   * the row index it sits on, so the row total is a safe upper bound and is the one
   * number that's always known.
   */
  const gutterWidth = $derived(String(estimatedTotalRows()).length)

  /**
   * The rows the template draws. A row the cache missed draws as an empty one with NO
   * gutter number: the physical line it belongs to is a fact only the backend has, and
   * printing the row index there would be a wrong line number on any wrapped file.
   */
  function getVisibleRows(): Array<{ rowNumber: number; text: string; continues: boolean; lineNumber: number | null }> {
    const result: Array<{ rowNumber: number; text: string; continues: boolean; lineNumber: number | null }> = []
    for (let i = visibleFrom; i < renderedTo; i++) {
      const row = rowCache.get(i)
      result.push({
        rowNumber: i,
        text: row?.text ?? '',
        continues: row?.continues ?? false,
        lineNumber: row?.lineNumber ?? null,
      })
    }
    return result
  }

  /**
   * The text the template SHOWS for a row, which is what the caret motion model has to
   * reason about, or `undefined` when no row is drawn for it yet.
   *
   * Inside the rendered range a cache miss draws as an empty row (`getVisibleRows`
   * applies the same `?? ''`), so a caller reading the cache directly would disagree with
   * the user's screen about which rows exist. That divergence is fatal for a file ending
   * in a newline on the `lineIndex` backend: it counts a last row `viewer_get_lines`
   * never emits, and ⌘⇧Down would ask for it forever.
   *
   * OUTSIDE the range `undefined` keeps its other meaning: not fetched yet, scroll and
   * retry. `moveFocus` turns that into `{ focus: null, targetRow }`, and the keyboard's
   * scroll is what pulls the row in so the next press lands. Widening the `''` past the
   * rendered range would break that two-press flow, and invent an offset for a row
   * nobody has seen.
   */
  function renderedRowText(row: number): string | undefined {
    const cached = rowCache.get(row)
    if (cached !== undefined) return cached.text
    return row >= visibleFrom && row < renderedTo ? '' : undefined
  }

  /** Returns the scaled Y offset for row n. Used by search for scroll-to-match. */
  function getRowTop(n: number): number {
    if (heightMap.ready) {
      return heightMap.getRowTop(n) * scrollScale
    }
    return n * scrollLineHeight
  }

  /** Returns the row at the current viewport top, using the height map (not the DOM buffer). */
  function getAnchorRow(): number {
    const unscaledY = scrollScale < 1 ? scrollTop / scrollScale : scrollTop
    return heightMap.getRowAtPosition(unscaledY)
  }

  /**
   * Writes `rows` into the cache starting at `firstRow`, exactly as the backend served
   * them. The page uses it for the open result's first chunk; the fetch walk's
   * `writeRows` dep for the rest.
   */
  function cacheRows(firstRow: number, rows: ViewerRow[]) {
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i]
      rowCache.set(firstRow + i, { text: row.text, continues: row.continues, lineNumber: row.lineNumber })
    }
  }

  /** Drops every cached row, and with it what the walk knew about where the file ends. */
  function clearCache() {
    rowCache.clear()
    rowFetch.forgetFileEnd()
  }

  /**
   * Drops cached rows far from the viewport, keeping a generous margin around what's
   * rendered. Runs after a fetch, since that's the only thing that grows the cache.
   *
   * ❌ Not on `fullLoad`: the height map measures EVERY row of such a file, and
   * `getAllRowTexts` hands it the cache. Evicting there would silently disable
   * variable-height word wrap. Those files are under a megabyte, so there's nothing to
   * reclaim anyway.
   */
  function evictDistantRows() {
    if (deps.getBackendType() === 'fullLoad') return
    if (rowCache.size <= CACHE_EVICT_ABOVE) return
    const keep = { from: visibleFrom - CACHE_KEEP_MARGIN, to: renderedTo + CACHE_KEEP_MARGIN }
    const evictable = rowsToEvict(rowCache.keys(), keep)
    for (const row of evictable) rowCache.delete(row)
    log.debug('evicted {count} cached {rowsNoun} outside [{from}, {to})', {
      count: evictable.length,
      rowsNoun: pluralize(evictable.length, 'row'),
      from: keep.from,
      to: keep.to,
    })
  }

  /**
   * Adopts a counted row total, keeping the user where they were in the file.
   *
   * The spacer's height is the row count times the row height, so a byte-seek estimate
   * being replaced by the real index moves every pixel under the scrollbar. Preserving
   * the FRACTION rather than the pixel is what stops the view jumping when the index
   * lands mid-scroll.
   */
  function updateTotalRows(newTotal: number) {
    const oldEstimate = estimatedTotalRows()
    if (!contentRef || oldEstimate === 0 || newTotal === oldEstimate) {
      deps.setTotalRows(newTotal)
      return
    }
    const oldHeight = Math.min(oldEstimate * effectiveLineHeight, MAX_SCROLL_HEIGHT)
    const scrollFraction = contentRef.scrollTop / oldHeight
    log.debug('totalRows changed: {oldEstimate} -> {newTotal}, preserving scroll fraction {fraction}', {
      oldEstimate,
      newTotal,
      fraction: scrollFraction.toFixed(3),
    })
    deps.setTotalRows(newTotal)
    const newHeight = Math.min(newTotal * effectiveLineHeight, MAX_SCROLL_HEIGHT)
    const newScrollTop = Math.round(scrollFraction * newHeight)
    const ref = contentRef
    requestAnimationFrame(() => {
      ref.scrollTop = newScrollTop
    })
  }

  /**
   * The fetch walk. It reads this composable's geometry and row store through getters and
   * owns nothing of them: the seam is what keeps pixel math and the ask-answer chain
   * separately readable. `writeRows` is the store's one writer, so every cached row goes
   * through the same eviction pass.
   */
  const rowFetch = createViewerRowFetch({
    getSessionId: () => deps.getSessionId(),
    getTotalRows: () => deps.getTotalRows(),
    getEstimatedTotalRows: estimatedTotalRows,
    getVisibleFrom: () => visibleFrom,
    getVisibleTo: () => visibleTo,
    getPrefetchRows: () => bufferRows(effectiveLineHeight),
    hasRow: (row) => rowCache.has(row),
    writeRows: (firstRow, rows) => {
      cacheRows(firstRow, rows)
      evictDistantRows()
    },
    setTotalRows: updateTotalRows,
    onTimeoutError: () => {
      deps.onTimeoutError()
    },
  })

  /**
   * Whether tail mode should keep the viewport on the last row as the file grows. Only a
   * scroll event moves it, so the user's own scrolling decides: up releases it, back down to
   * the end re-pins. Growth fires no scroll event (`scrollTop` stays put), so it survives that.
   */
  let followsEnd = false

  function handleScroll() {
    if (contentRef) {
      scrollTop = contentRef.scrollTop
      viewportHeight = contentRef.clientHeight
      followsEnd = isAtEnd()
    }
  }

  /** Jump to the end and keep following it, for a viewer that opens tailed. */
  function pinToEnd() {
    followsEnd = true
    scrollToEnd()
  }

  /**
   * Tail follow: re-run on every row-count change (the indexing poll reports growth), and
   * scroll to the new end when tail mode is on and the viewport was following it.
   */
  function runTailFollowEffect(tailMode: boolean) {
    dependOn(estimatedTotalRows())
    if (!tailMode || !followsEnd) return
    void tick().then(scrollToEnd)
  }

  function scrollByRows(rows: number) {
    if (!contentRef) return

    if (heightMap.ready) {
      // Find current row at the top of viewport, move by `rows` rows, look up new position
      const unscaledY = scrollScale < 1 ? contentRef.scrollTop / scrollScale : contentRef.scrollTop
      const currentRow = heightMap.getRowAtPosition(unscaledY)
      const targetRow = Math.max(0, Math.min(estimatedTotalRows() - 1, currentRow + rows))
      contentRef.scrollTop = Math.max(0, heightMap.getRowTop(targetRow) * scrollScale)
    } else {
      contentRef.scrollTop = Math.max(0, contentRef.scrollTop + rows * scrollLineHeight)
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
      const rowsPerPage = Math.floor(contentRef.clientHeight / effectiveLineHeight) - 1
      contentRef.scrollTop = Math.max(0, contentRef.scrollTop + pages * rowsPerPage * scrollLineHeight)
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

  /** Whether the viewport shows the end of the file, give or take one row. */
  function isAtEnd(): boolean {
    return contentRef ? isScrolledToEnd(contentRef, scrollLineHeight) : false
  }

  /**
   * The rendered height of row `n`, in the same scaled space `getRowTop` speaks. The
   * height map holds the real per-row height once it's ready (a row wrapped by CSS is
   * several text lines tall); before that every row is one text line.
   */
  function rowHeightAt(n: number): number {
    if (heightMap.ready) {
      const measured = (heightMap.getRowTop(n + 1) - heightMap.getRowTop(n)) * scrollScale
      if (measured > 0) return measured
    }
    return scrollLineHeight
  }

  /**
   * Scrolls row `n` just into view, with one row of breathing room, and leaves an
   * already-visible row alone. Drives keyboard selection extension: every extend press
   * calls this, including the one whose target row isn't cached yet, because the scroll
   * is what pulls the row into the render window and triggers its fetch.
   *
   * ❌ The `EOF_ROW` branch is NOT redundant, however much the arithmetic below looks
   * like it would cope. `⌘⇧Down` on a file with no index reports the sentinel as its
   * target, and it only survives `getRowTop` today through integer overflow plus the
   * browser clamping an absurd `scrollTop` — don't lean on that. Worse, the obvious later
   * tidy-up `Math.min(n, totalRows - 1)` yields `NaN` on exactly this branch (the row
   * count is `null` precisely when the sentinel appears), and `scrollTop = NaN` throws the
   * view to the TOP of the file. Branching here keeps that visible to whoever reaches for
   * the clamp.
   */
  function ensureRowVisible(n: number) {
    if (!contentRef) return
    if (n === EOF_ROW) {
      scrollToEnd()
      return
    }
    const next = ensureVisibleOffset({
      rowTop: getRowTop(n),
      rowHeight: rowHeightAt(n),
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
  function ensureColumnVisible(point: RowOffset) {
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

  function runContentWidthEffect() {
    if (wordWrap) return
    dependOn(visibleRows)
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
      const rowCount = linesContainerRef.children.length
      if (rowCount === 0) return
      const renderedHeight = linesContainerRef.getBoundingClientRect().height
      if (renderedHeight > 0) {
        const measured = renderedHeight / rowCount
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
      // With height map: find the row at current viewport top, look up its new position
      const anchorRow = getAnchorRow()
      contentRef.scrollTop = heightMap.getRowTop(anchorRow) * scrollScale
    } else {
      // Without height map: scale proportionally (existing behavior)
      const ratio = newSLH / prevScrollLineHeight
      contentRef.scrollTop = Math.round(contentRef.scrollTop * ratio)
    }
    prevScrollLineHeight = newSLH
  }

  /**
   * Watches wordWrap + getAllRowTexts + getTextWidth and triggers height map preparation
   * when all conditions are met (word wrap on, fullLoad rows available, width known).
   * Does NOT re-prepare if the height map is already ready; width changes are handled
   * by runHeightMapReflowEffect via reflow() instead.
   */
  function runHeightMapInitEffect() {
    // Read reactive deps to establish tracking

    const ww = wordWrap
    const rows = deps.getAllRowTexts()
    const textWidth = deps.getTextWidth()

    if (!ww) {
      heightMap.cancel()
      return
    }

    if (heightMap.ready) return // Width changes handled by reflow, not re-preparation

    if (rows !== null && rows.length > 0 && textWidth > 0) {
      heightMap.prepareRows(rows, textWidth)
    }
  }

  /**
   * Re-measures all row heights at the new width and preserves the scroll
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
  // map baked in is stale. Re-layout (without changing width) so wrapped-row
  // heights match the new font. This runs inside the same debounced "settled"
  // event the file-list column-width path uses, so we don't thrash mid-drag.
  const unsubscribeScaleChange = onDebouncedScaleChange(() => {
    heightMap.recomputeForLineHeightChange()
    // A new font size means a new column advance; re-measure it on the next press.
    columnWidth = null
  })

  function destroy() {
    rowFetch.destroy()
    if (reflowDebounceTimer) clearTimeout(reflowDebounceTimer)
    heightMap.cancel()
    unsubscribeScaleChange()
  }

  return {
    rowCache,
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
    get scrollScale() {
      return scrollScale
    },
    get visibleFrom() {
      return visibleFrom
    },
    get visibleRows() {
      return visibleRows
    },
    get gutterWidth() {
      return gutterWidth
    },
    get spacerHeight() {
      return spacerHeight
    },
    get rowsOffset() {
      return rowsOffset
    },
    get heightMapReady() {
      return heightMap.ready
    },
    cacheRows,
    clearCache,
    estimatedTotalRows,
    renderedRowText,
    getRowTop,
    handleScroll,
    scrollByRows,
    scrollByPages,
    scrollToStart,
    scrollToEnd,
    pinToEnd,
    runTailFollowEffect,
    scrollByColumns,
    ensureRowVisible,
    ensureColumnVisible,
    runFetchEffect: rowFetch.runFetchEffect,
    fetchVisibleNow: rowFetch.fetchVisibleNow,
    runContentWidthEffect,
    runWrappedLineHeightEffect,
    runScrollCompensationEffect,
    runHeightMapInitEffect,
    runHeightMapReflowEffect,
    destroy,
  }
}
