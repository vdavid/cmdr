<script lang="ts" module>
    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }

    const LINE_HEIGHT = 18
    const BUFFER_LINES = 50
    const FETCH_BATCH = 500
    const SEARCH_POLL_INTERVAL = 100
</script>

<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteMap } from 'svelte/reactivity'
    import type { ViewerSearchMatch } from '$lib/tauri-commands'

    let fileName = $state('')
    let totalLines = $state<number | null>(null)
    let totalBytes = $state(0)
    let error = $state('')
    let loading = $state(true)
    let sessionId = $state('')
    let backendType = $state<'fullLoad' | 'byteSeek' | 'lineIndex'>('fullLoad')

    // Line cache: lineNumber -> text
    const lineCache = new SvelteMap<number, string>()
    // The range of lines currently in cache
    let cachedFrom = $state(0)
    let cachedTo = $state(0)

    // Virtual scroll state
    let scrollTop = $state(0)
    let viewportHeight = $state(600)
    let contentRef: HTMLDivElement | undefined = $state()

    // Derived: which lines are visible
    const visibleFrom = $derived(Math.max(0, Math.floor(scrollTop / LINE_HEIGHT) - BUFFER_LINES))
    const visibleTo = $derived(
        Math.min(estimatedTotalLines(), Math.ceil((scrollTop + viewportHeight) / LINE_HEIGHT) + BUFFER_LINES),
    )
    const visibleLines = $derived(getVisibleLines())
    const gutterWidth = $derived(String(estimatedTotalLines()).length)

    // Search state
    let searchVisible = $state(false)
    let searchQuery = $state('')
    let searchMatches = $state<ViewerSearchMatch[]>([])
    let currentMatchIndex = $state(-1)
    let searchStatus = $state<'idle' | 'running' | 'done' | 'cancelled'>('idle')
    let searchProgress = $state(0)
    let searchInputRef: HTMLInputElement | undefined = $state()
    let searchPollTimer: ReturnType<typeof setInterval> | undefined

    // Track pending fetches to avoid duplicate requests
    let fetchingRange = $state(false)

    function estimatedTotalLines(): number {
        if (totalLines !== null) return totalLines
        // Estimate based on average 80 bytes per line
        return Math.max(1, Math.ceil(totalBytes / 80))
    }

    function getVisibleLines(): Array<{ lineNumber: number; text: string }> {
        const result: Array<{ lineNumber: number; text: string }> = []
        const end = Math.min(visibleTo, estimatedTotalLines())
        for (let i = visibleFrom; i < end; i++) {
            result.push({ lineNumber: i, text: lineCache.get(i) ?? '' })
        }
        return result
    }

    // Fetch lines when visible range changes
    $effect(() => {
        const from = visibleFrom
        const to = visibleTo
        if (sessionId && !fetchingRange) {
            // Check if we need to fetch
            if (from < cachedFrom || to > cachedTo) {
                void fetchLines(from, to)
            }
        }
    })

    async function fetchLines(from: number, to: number) {
        if (!sessionId || fetchingRange) return
        fetchingRange = true
        try {
            const { viewerGetLines } = await import('$lib/tauri-commands')
            // Fetch a larger batch to reduce round-trips
            const fetchFrom = Math.max(0, from - BUFFER_LINES)
            const fetchCount = Math.min(FETCH_BATCH, to - fetchFrom + BUFFER_LINES * 2)
            const chunk = await viewerGetLines(sessionId, 'line', fetchFrom, fetchCount)

            // Update cache
            for (let i = 0; i < chunk.lines.length; i++) {
                lineCache.set(chunk.firstLineNumber + i, chunk.lines[i])
            }
            cachedFrom = Math.min(cachedFrom, chunk.firstLineNumber)
            cachedTo = Math.max(cachedTo, chunk.firstLineNumber + chunk.lines.length)

            // Update totalLines if backend now knows it
            if (chunk.totalLines !== null) {
                totalLines = chunk.totalLines
            }
        } catch {
            // Fetch failed, will retry on next scroll
        } finally {
            fetchingRange = false
        }
    }

    function handleScroll() {
        if (contentRef) {
            scrollTop = contentRef.scrollTop
            viewportHeight = contentRef.clientHeight
        }
    }

    // Search functions
    async function startSearch(query: string) {
        if (!sessionId || !query) return
        searchMatches = []
        currentMatchIndex = -1
        searchStatus = 'running'
        searchProgress = 0

        try {
            const { viewerSearchStart } = await import('$lib/tauri-commands')
            await viewerSearchStart(sessionId, query)
            pollSearch()
        } catch {
            searchStatus = 'idle'
        }
    }

    function pollSearch() {
        stopSearchPoll()
        searchPollTimer = setInterval(() => {
            void pollSearchTick()
        }, SEARCH_POLL_INTERVAL)
    }

    async function pollSearchTick() {
        if (!sessionId) return
        try {
            const { viewerSearchPoll } = await import('$lib/tauri-commands')
            const result = await viewerSearchPoll(sessionId)
            searchMatches = result.matches
            searchProgress = totalBytes > 0 ? result.bytesScanned / totalBytes : 0
            if (currentMatchIndex === -1 && result.matches.length > 0) {
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

    function stopSearchPoll() {
        if (searchPollTimer) {
            clearInterval(searchPollTimer)
            searchPollTimer = undefined
        }
    }

    async function cancelSearch() {
        stopSearchPoll()
        if (!sessionId) return
        try {
            const { viewerSearchCancel } = await import('$lib/tauri-commands')
            await viewerSearchCancel(sessionId)
        } catch {
            // Ignore
        }
        searchStatus = 'idle'
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

    function scrollToMatch(match: ViewerSearchMatch) {
        if (!contentRef) return
        const targetScroll = match.line * LINE_HEIGHT - viewportHeight / 2
        contentRef.scrollTop = Math.max(0, targetScroll)
    }

    // Debounce search input
    let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined
    $effect(() => {
        const query = searchQuery
        if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
        if (query && searchVisible) {
            searchDebounceTimer = setTimeout(() => {
                void startSearch(query)
            }, 300)
        } else {
            void cancelSearch()
            searchMatches = []
            currentMatchIndex = -1
        }
    })

    async function closeWindow() {
        if (sessionId) {
            try {
                const { viewerClose } = await import('$lib/tauri-commands')
                await viewerClose(sessionId)
            } catch {
                // Ignore
            }
        }
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            await getCurrentWindow().close()
        } catch {
            // Not in Tauri environment
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
            e.preventDefault()
            openSearch()
            return
        }

        if (e.key === 'Escape') {
            e.preventDefault()
            if (searchVisible) {
                closeSearch()
            } else {
                void closeWindow()
            }
            return
        }

        if (e.key === 'Enter' && searchVisible) {
            e.preventDefault()
            if (e.shiftKey) {
                findPrev()
            } else {
                findNext()
            }
        }
    }

    /** Highlights search matches within a line. */
    function getHighlightedSegments(lineNumber: number, lineText: string) {
        const lineMatches = searchMatches.filter((m) => m.line === lineNumber)
        if (lineMatches.length === 0) {
            return [{ text: lineText, highlight: false, active: false }]
        }

        const segments: Array<{ text: string; highlight: boolean; active: boolean }> = []
        let pos = 0
        for (const m of lineMatches) {
            if (m.column > pos) {
                segments.push({ text: lineText.slice(pos, m.column), highlight: false, active: false })
            }
            const isActive = searchMatches.indexOf(m) === currentMatchIndex
            segments.push({
                text: lineText.slice(m.column, m.column + m.length),
                highlight: true,
                active: isActive,
            })
            pos = m.column + m.length
        }
        if (pos < lineText.length) {
            segments.push({ text: lineText.slice(pos), highlight: false, active: false })
        }
        return segments
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        const params = new URLSearchParams(window.location.search)
        const filePath = params.get('path')

        if (!filePath) {
            error = 'No file path specified'
            loading = false
            return
        }

        try {
            const { viewerOpen } = await import('$lib/tauri-commands')
            const result = await viewerOpen(filePath)

            sessionId = result.sessionId
            fileName = result.fileName
            totalBytes = result.totalBytes
            totalLines = result.totalLines
            backendType = result.backendType

            // Populate cache with initial lines
            lineCache.clear()
            for (let i = 0; i < result.initialLines.lines.length; i++) {
                lineCache.set(result.initialLines.firstLineNumber + i, result.initialLines.lines[i])
            }
            cachedFrom = result.initialLines.firstLineNumber
            cachedTo = result.initialLines.firstLineNumber + result.initialLines.lines.length

            // Set window title
            try {
                const { getCurrentWindow } = await import('@tauri-apps/api/window')
                await getCurrentWindow().setTitle(`${result.fileName} — Viewer`)
            } catch {
                // Not in Tauri environment
            }
        } catch (e) {
            error = typeof e === 'string' ? e : 'Failed to read file'
        } finally {
            loading = false
        }
    })

    onDestroy(() => {
        stopSearchPoll()
        if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
    })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="viewer-container" onkeydown={handleKeyDown} tabindex={-1}>
    {#if searchVisible}
        <div class="search-bar" role="search">
            <input
                bind:this={searchInputRef}
                bind:value={searchQuery}
                type="text"
                placeholder="Find in file..."
                aria-label="Search text"
                class="search-input"
            />
            <span class="match-count" aria-live="polite">
                {#if searchMatches.length > 0}
                    {currentMatchIndex + 1} of {searchMatches.length}
                {:else if searchStatus === 'running'}
                    Searching... {Math.round(searchProgress * 100)}%
                {:else if searchQuery && searchStatus === 'done'}
                    No matches
                {/if}
            </span>
            <button
                onclick={findPrev}
                disabled={searchMatches.length === 0}
                aria-label="Previous match"
                title="Previous match (Shift+Enter)">&#x25B2;</button
            >
            <button
                onclick={findNext}
                disabled={searchMatches.length === 0}
                aria-label="Next match"
                title="Next match (Enter)">&#x25BC;</button
            >
            <button onclick={closeSearch} aria-label="Close search" title="Close (Escape)">&#x2715;</button>
        </div>
    {/if}

    {#if loading}
        <div class="status-message">Loading...</div>
    {:else if error}
        <div class="status-message error">{error}</div>
    {:else}
        <div
            class="file-content"
            role="document"
            aria-label="File content: {fileName}"
            bind:this={contentRef}
            onscroll={handleScroll}
        >
            <div class="scroll-spacer" style="height: {estimatedTotalLines() * LINE_HEIGHT}px">
                <div class="lines-container" style="transform: translateY({visibleFrom * LINE_HEIGHT}px)">
                    {#each visibleLines as { lineNumber, text } (lineNumber)}
                        <div class="line" data-line={lineNumber}>
                            <span class="line-number" style="width: {gutterWidth}ch" aria-hidden="true"
                                >{lineNumber + 1}</span
                            >
                            <span class="line-text"
                                >{#each getHighlightedSegments(lineNumber, text) as seg, segIdx (segIdx)}{#if seg.highlight}<mark
                                            class:active={seg.active}>{seg.text}</mark
                                        >{:else}{seg.text}{/if}{/each}</span
                            >
                        </div>
                    {/each}
                </div>
            </div>
        </div>
    {/if}

    <div class="status-bar" aria-label="File information">
        <span>{fileName}</span>
        {#if totalLines !== null}
            <span>{totalLines} {totalLines === 1 ? 'line' : 'lines'}</span>
        {/if}
        <span>{formatSize(totalBytes)}</span>
        {#if backendType !== 'fullLoad'}
            <span class="backend-badge">{backendType === 'lineIndex' ? 'indexed' : 'streaming'}</span>
        {/if}
        <span class="shortcut-hint">Ctrl+F search &middot; Esc close</span>
    </div>
</div>

<style>
    .viewer-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        font-family: var(--font-system);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .search-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        flex-shrink: 0;
    }

    .search-input {
        flex: 1;
        max-width: 300px;
        padding: 3px 8px;
        border: 1px solid var(--color-border-primary);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
    }

    .search-input:focus {
        border-color: var(--color-accent);
        outline: none;
    }

    .match-count {
        font-size: 11px;
        color: var(--color-text-secondary);
        min-width: 70px;
    }

    .search-bar button {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-secondary);
        border-radius: 3px;
        color: var(--color-text-primary);
        font-size: 11px;
        padding: 2px 6px;
        cursor: pointer;
        line-height: 1;
    }

    .search-bar button:hover:not(:disabled) {
        background: var(--color-button-hover);
    }

    .search-bar button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    .file-content {
        flex: 1;
        overflow: auto;
        font-family: 'SF Mono', Menlo, Monaco, Consolas, monospace;
        font-size: 12px;
        line-height: 1.5;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .scroll-spacer {
        position: relative;
    }

    .lines-container {
        position: absolute;
        left: 0;
        right: 0;
    }

    .line {
        display: flex;
        padding: 0 var(--spacing-sm);
        height: 18px;
    }

    .line:hover {
        background: var(--color-bg-hover);
    }

    .line-number {
        display: inline-block;
        text-align: right;
        color: var(--color-text-muted);
        padding-right: var(--spacing-sm);
        margin-right: var(--spacing-sm);
        border-right: 1px solid var(--color-border-secondary);
        flex-shrink: 0;
        user-select: none;
        -webkit-user-select: none;
    }

    .line-text {
        white-space: pre;
        word-break: break-all;
        flex: 1;
        min-width: 0;
        overflow: hidden;
    }

    mark {
        background: #fff3a8;
        color: #000;
        border-radius: 2px;
        padding: 0 1px;
    }

    mark.active {
        background: #ff9632;
        color: #fff;
    }

    @media (prefers-color-scheme: dark) {
        mark {
            background: #665d20;
            color: #fff;
        }
        mark.active {
            background: #cc6600;
            color: #fff;
        }
    }

    .status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-primary);
        font-size: 11px;
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .backend-badge {
        padding: 1px 4px;
        border-radius: 3px;
        background: var(--color-bg-tertiary);
        color: var(--color-text-muted);
        font-size: 10px;
    }

    .shortcut-hint {
        margin-left: auto;
        color: var(--color-text-muted);
    }

    .status-message {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: 1;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .status-message.error {
        color: var(--color-error);
    }
</style>
