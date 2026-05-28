import { tick } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import {
  viewerSearchStart,
  viewerSearchPoll,
  viewerSearchCancel,
  type ViewerSearchMatch,
  type ViewerSearchMode,
  type ViewerSearchStatus,
} from '$lib/tauri-commands'
import { segmentLine, type LineSegment, type SegmentMatchInput, type SelectionBoundsInput } from './line-segments'

const SEARCH_POLL_INTERVAL = 100

/** Flat status the toolbar UI inspects. Mirrors the backend `SearchStatus`
 *  with the invalid-query payload extracted into a sibling `searchError` state
 *  so template branches stay simple (`status === 'invalidQuery'` rather than a
 *  tagged-union match). */
export type ViewerSearchUiStatus = 'idle' | 'running' | 'done' | 'cancelled' | 'invalidQuery'

interface SearchDeps {
  getSessionId: () => string
  getTotalBytes: () => number
  getTotalLines: () => number | null
  getEstimatedTotalLines: () => number
  getScrollLineHeight: () => number
  getLineTop: (n: number) => number
  getViewportHeight: () => number
  getContentRef: () => HTMLDivElement | undefined
}

export function createViewerSearch(deps: SearchDeps) {
  let searchVisible = $state(false)
  let searchQuery = $state('')
  let searchMatches = $state<ViewerSearchMatch[]>([])
  let currentMatchIndex = $state(-1)
  let searchStatus = $state<ViewerSearchUiStatus>('idle')
  /** Non-null only when `searchStatus === 'invalidQuery'`. The message comes
   *  from the backend verbatim and is rendered as plain text in the toolbar. */
  let searchError = $state<string | null>(null)
  let searchProgress = $state(0)
  let searchLimitReached = $state(false)
  /** Whether the query is interpreted as a regex (`.*`). Default off. */
  let useRegex = $state(false)
  /** Whether the search is case-sensitive (`Aa`). Default on, per the plan. */
  let caseSensitive = $state(true)
  let searchInputRef: HTMLInputElement | undefined = $state()
  let searchPollTimer: ReturnType<typeof setInterval> | undefined
  let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined

  const matchesByLine = $derived.by(() => {
    const map = new SvelteMap<number, Array<{ match: ViewerSearchMatch; globalIndex: number }>>()
    for (let i = 0; i < searchMatches.length; i++) {
      const m = searchMatches[i]
      let entries = map.get(m.line)
      if (!entries) {
        entries = []
        map.set(m.line, entries)
      }
      entries.push({ match: m, globalIndex: i })
    }
    return map
  })

  function currentMode(): ViewerSearchMode {
    return { useRegex, caseSensitive }
  }

  function stopSearchPoll() {
    if (searchPollTimer) {
      clearInterval(searchPollTimer)
      searchPollTimer = undefined
    }
  }

  /** Project the backend's tagged-union `SearchStatus` to the flat UI status,
   *  and split the message into `searchError`. Doing it once per poll keeps
   *  the template branches free of the tag inspection. */
  function applyBackendStatus(status: ViewerSearchStatus) {
    switch (status.status) {
      case 'invalidQuery':
        searchStatus = 'invalidQuery'
        searchError = status.message
        break
      case 'running':
        searchStatus = 'running'
        searchError = null
        break
      case 'done':
        searchStatus = 'done'
        searchError = null
        break
      case 'cancelled':
        searchStatus = 'cancelled'
        searchError = null
        break
      case 'idle':
        searchStatus = 'idle'
        searchError = null
        break
    }
  }

  async function pollSearchTick() {
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    try {
      const result = await viewerSearchPoll(sessionId, searchMatches.length)
      if (result.newMatches.length > 0) {
        searchMatches = [...searchMatches, ...result.newMatches]
      }
      const totalBytes = deps.getTotalBytes()
      searchProgress = totalBytes > 0 ? result.bytesScanned / totalBytes : 0
      searchLimitReached = result.matchLimitReached
      if (currentMatchIndex === -1 && searchMatches.length > 0) {
        currentMatchIndex = 0
      }
      applyBackendStatus(result.status)
      if (result.status.status !== 'running') {
        stopSearchPoll()
      }
    } catch {
      stopSearchPoll()
      searchStatus = 'idle'
      searchError = null
    }
  }

  function pollSearch() {
    stopSearchPoll()
    searchPollTimer = setInterval(() => {
      void pollSearchTick()
    }, SEARCH_POLL_INTERVAL)
  }

  async function startSearch(query: string) {
    const sessionId = deps.getSessionId()
    if (!sessionId || !query) return
    searchMatches = []
    currentMatchIndex = -1
    searchStatus = 'running'
    searchError = null
    searchProgress = 0
    searchLimitReached = false

    try {
      await viewerSearchStart(sessionId, query, currentMode())
      pollSearch()
    } catch {
      searchStatus = 'idle'
    }
  }

  async function cancelSearch() {
    stopSearchPoll()
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    try {
      await viewerSearchCancel(sessionId)
    } catch {
      // Ignore
    }
    searchStatus = 'idle'
    searchError = null
  }

  function stopSearch() {
    stopSearchPoll()
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    viewerSearchCancel(sessionId).catch(() => {})
    searchStatus = 'cancelled'
  }

  function openSearch() {
    searchVisible = true
    void tick().then(() => {
      searchInputRef?.focus()
      searchInputRef?.select()
    })
  }

  function closeSearch() {
    searchVisible = false
    searchQuery = ''
    void cancelSearch()
    searchMatches = []
    currentMatchIndex = -1
    searchProgress = 0
    searchLimitReached = false
    searchError = null
  }

  /** Toggle a search-mode flag. If a query is active, immediately cancels the
   *  in-flight search and re-runs with the new mode so the user sees results
   *  for the toggled state. */
  function setUseRegex(next: boolean) {
    if (useRegex === next) return
    useRegex = next
    if (searchQuery && searchVisible) {
      void cancelSearch().then(() => {
        void startSearch(searchQuery)
      })
    }
  }

  function toggleUseRegex() {
    setUseRegex(!useRegex)
  }

  function setCaseSensitive(next: boolean) {
    if (caseSensitive === next) return
    caseSensitive = next
    if (searchQuery && searchVisible) {
      void cancelSearch().then(() => {
        void startSearch(searchQuery)
      })
    }
  }

  function toggleCaseSensitive() {
    setCaseSensitive(!caseSensitive)
  }

  function scrollToMatch(match: ViewerSearchMatch) {
    const contentRef = deps.getContentRef()
    if (!contentRef) return
    const totalLines = deps.getTotalLines()
    const totalBytes = deps.getTotalBytes()
    let targetLine: number
    if (totalLines !== null) {
      targetLine = match.line
    } else {
      targetLine = totalBytes > 0 ? (match.byteOffset / totalBytes) * deps.getEstimatedTotalLines() : match.line
    }
    const targetScroll = deps.getLineTop(targetLine) - deps.getViewportHeight() / 2
    contentRef.scrollTop = Math.max(0, targetScroll)
  }

  function findNext() {
    if (searchMatches.length === 0) return
    currentMatchIndex = (currentMatchIndex + 1) % searchMatches.length
    scrollToMatch(searchMatches[currentMatchIndex])
  }

  function findPrev() {
    if (searchMatches.length === 0) return
    currentMatchIndex = (currentMatchIndex - 1 + searchMatches.length) % searchMatches.length
    scrollToMatch(searchMatches[currentMatchIndex])
  }

  /**
   * Computes the search-match spans for a single line. Pure helper used by
   * `getHighlightedSegments` and exposed so callers (the page component) can
   * pass the matches through the shared segmenter together with selection
   * bounds.
   *
   * In regex mode, we cannot recompute matches client-side without re-running
   * the regex against each line, which would duplicate work. So in regex mode
   * we rely on the backend's authoritative `searchMatches` array and emit
   * spans directly from it.
   */
  function getLineMatches(lineNumber: number, lineText: string): SegmentMatchInput[] {
    if (!searchQuery || !searchVisible) return []

    if (useRegex) {
      const entries = matchesByLine.get(lineNumber)
      if (!entries || entries.length === 0) return []
      return entries.map(({ match, globalIndex }) => ({
        column: match.column,
        length: match.length,
        active: globalIndex === currentMatchIndex,
      }))
    }

    const queryLower = caseSensitive ? searchQuery : searchQuery.toLowerCase()
    const lineForSearch = caseSensitive ? lineText : lineText.toLowerCase()
    const result: SegmentMatchInput[] = []
    const activeEntry = matchesByLine.get(lineNumber)?.find((e) => e.globalIndex === currentMatchIndex)
    let searchStart = 0
    for (;;) {
      const idx = lineForSearch.indexOf(queryLower, searchStart)
      if (idx === -1) break
      const isActive = activeEntry !== undefined && activeEntry.match.column === idx
      result.push({ column: idx, length: queryLower.length, active: isActive })
      searchStart = idx + queryLower.length
    }
    return result
  }

  /**
   * Returns the rendered segments for a line, combining search-match
   * highlights with optional selection bounds.
   */
  function getHighlightedSegments(
    lineNumber: number,
    lineText: string,
    selectionBounds: SelectionBoundsInput | null = null,
  ): LineSegment[] {
    const matches = getLineMatches(lineNumber, lineText)
    return segmentLine(lineText, matches, selectionBounds)
  }

  function runDebounceEffect() {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- read the toggles so $effect re-runs when they change
    useRegex
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- same as above
    caseSensitive
    const query = searchQuery
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
    if (query && searchVisible) {
      searchDebounceTimer = setTimeout(() => {
        void startSearch(query)
      }, 100)
    } else {
      void cancelSearch()
      searchMatches = []
      currentMatchIndex = -1
    }
  }

  function destroy() {
    stopSearchPoll()
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
  }

  return {
    get searchVisible() {
      return searchVisible
    },
    get searchQuery() {
      return searchQuery
    },
    set searchQuery(v: string) {
      searchQuery = v
    },
    get searchMatches() {
      return searchMatches
    },
    get currentMatchIndex() {
      return currentMatchIndex
    },
    get searchStatus() {
      return searchStatus
    },
    get searchError() {
      return searchError
    },
    get searchProgress() {
      return searchProgress
    },
    get searchLimitReached() {
      return searchLimitReached
    },
    get useRegex() {
      return useRegex
    },
    get caseSensitive() {
      return caseSensitive
    },
    get searchInputRef() {
      return searchInputRef
    },
    set searchInputRef(v: HTMLInputElement | undefined) {
      searchInputRef = v
    },
    openSearch,
    closeSearch,
    findNext,
    findPrev,
    stopSearch,
    cancelSearch,
    toggleUseRegex,
    setUseRegex,
    toggleCaseSensitive,
    setCaseSensitive,
    getHighlightedSegments,
    runDebounceEffect,
    destroy,
  }
}
