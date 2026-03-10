import { tick } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { viewerSearchStart, viewerSearchPoll, viewerSearchCancel, type ViewerSearchMatch } from '$lib/tauri-commands'

const SEARCH_POLL_INTERVAL = 100

interface SearchDeps {
    getSessionId: () => string
    getTotalBytes: () => number
    getTotalLines: () => number | null
    getEstimatedTotalLines: () => number
    getScrollLineHeight: () => number
    getViewportHeight: () => number
    getContentRef: () => HTMLDivElement | undefined
}

export function createViewerSearch(deps: SearchDeps) {
    let searchVisible = $state(false)
    let searchQuery = $state('')
    let searchMatches = $state<ViewerSearchMatch[]>([])
    let currentMatchIndex = $state(-1)
    let searchStatus = $state<'idle' | 'running' | 'done' | 'cancelled'>('idle')
    let searchProgress = $state(0)
    let searchLimitReached = $state(false)
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

    function stopSearchPoll() {
        if (searchPollTimer) {
            clearInterval(searchPollTimer)
            searchPollTimer = undefined
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
            if (result.status !== 'running') {
                searchStatus = result.status
                stopSearchPoll()
            }
        } catch {
            stopSearchPoll()
            searchStatus = 'idle'
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
        searchProgress = 0
        searchLimitReached = false

        try {
            await viewerSearchStart(sessionId, query)
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
        const targetScroll = targetLine * deps.getScrollLineHeight() - deps.getViewportHeight() / 2
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

    function getHighlightedSegments(lineNumber: number, lineText: string) {
        if (!searchQuery || !searchVisible) {
            return [{ text: lineText, highlight: false, active: false }]
        }

        const queryLower = searchQuery.toLowerCase()
        const lineLower = lineText.toLowerCase()
        const localMatches: Array<{ column: number; length: number }> = []
        let searchStart = 0
        for (;;) {
            const idx = lineLower.indexOf(queryLower, searchStart)
            if (idx === -1) break
            localMatches.push({ column: idx, length: queryLower.length })
            searchStart = idx + queryLower.length
        }

        if (localMatches.length === 0) {
            return [{ text: lineText, highlight: false, active: false }]
        }

        const activeEntry = matchesByLine.get(lineNumber)?.find((e) => e.globalIndex === currentMatchIndex)

        const segments: Array<{ text: string; highlight: boolean; active: boolean }> = []
        let pos = 0
        for (const m of localMatches) {
            if (m.column > pos) {
                segments.push({ text: lineText.slice(pos, m.column), highlight: false, active: false })
            }
            segments.push({
                text: lineText.slice(m.column, m.column + m.length),
                highlight: true,
                active: activeEntry !== undefined && activeEntry.match.column === m.column,
            })
            pos = m.column + m.length
        }
        if (pos < lineText.length) {
            segments.push({ text: lineText.slice(pos), highlight: false, active: false })
        }
        return segments
    }

    function runDebounceEffect() {
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
        get searchProgress() {
            return searchProgress
        },
        get searchLimitReached() {
            return searchLimitReached
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
        getHighlightedSegments,
        runDebounceEffect,
        destroy,
    }
}
