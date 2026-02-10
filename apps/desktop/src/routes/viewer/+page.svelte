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
    // Must match INDEXING_TIMEOUT_SECS in src-tauri/src/file_viewer/session.rs
    const INDEXING_TIMEOUT_SECS = 5
</script>

<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteMap } from 'svelte/reactivity'
    import {
        viewerOpen,
        viewerGetLines,
        viewerGetStatus,
        viewerClose,
        viewerSearchStart,
        viewerSearchPoll,
        viewerSearchCancel,
        feLog,
        type ViewerSearchMatch,
        type BackendCapabilities,
    } from '$lib/tauri-commands'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'

    let fileName = $state('')
    let totalLines = $state<number | null>(null)
    let estimatedLines = $state(1) // Backend's estimate based on initial sample
    let totalBytes = $state(0)
    let error = $state('')
    let loading = $state(true)
    let sessionId = $state('')
    let backendType = $state<'fullLoad' | 'byteSeek' | 'lineIndex'>('fullLoad')
    let capabilities = $state<BackendCapabilities | null>(null)
    let isIndexing = $state(false)

    // Line cache: lineNumber -> text (sparse - may have gaps)
    const lineCache = new SvelteMap<number, string>()

    // Virtual scroll state
    let scrollTop = $state(0)
    let viewportHeight = $state(600)
    let contentRef: HTMLDivElement | undefined = $state()
    let containerRef: HTMLDivElement | undefined = $state()
    let linesContainerRef: HTMLDivElement | undefined = $state()

    // High watermark of rendered line container width, for horizontal scroll
    let contentWidth = $state(0)

    // Derived: which lines are visible
    const visibleFrom = $derived(Math.max(0, Math.floor(scrollTop / LINE_HEIGHT) - BUFFER_LINES))
    const visibleTo = $derived(
        Math.min(estimatedTotalLines(), Math.ceil((scrollTop + viewportHeight) / LINE_HEIGHT) + BUFFER_LINES),
    )
    const visibleLines = $derived(getVisibleLines())
    const gutterWidth = $derived(String(estimatedTotalLines()).length)
    // Derive current mode: if we started with byteSeek but now have totalLines, we upgraded to lineIndex
    const currentMode = $derived(backendType === 'byteSeek' && totalLines !== null ? 'lineIndex' : backendType)

    // Search state
    let searchVisible = $state(false)
    let searchQuery = $state('')
    let searchMatches = $state<ViewerSearchMatch[]>([])
    let currentMatchIndex = $state(-1)
    let searchStatus = $state<'idle' | 'running' | 'done' | 'cancelled'>('idle')
    let searchProgress = $state(0)
    let searchInputRef: HTMLInputElement | undefined = $state()
    let searchPollTimer: ReturnType<typeof setInterval> | undefined

    // Fetch state: debounce timer and request ID for cancellation
    let fetchDebounceTimer: ReturnType<typeof setTimeout> | undefined
    let currentFetchId = 0 // Incremented on each fetch, used to ignore stale responses
    const FETCH_DEBOUNCE_MS = 100

    // Indexing status polling
    let indexingPollTimer: ReturnType<typeof setInterval> | undefined
    const INDEXING_POLL_INTERVAL = 500

    // Window lifecycle state: prevents closing before WebKit is fully initialized
    let windowReady = $state(false)
    let closeRequested = $state(false)

    // MCP event listener cleanup functions
    let unlistenMcpClose: UnlistenFn | undefined
    let unlistenMcpFocus: UnlistenFn | undefined

    function estimatedTotalLines(): number {
        // Use exact count if known (FullLoad or LineIndex backends)
        if (totalLines !== null) return totalLines
        // Otherwise use backend's estimate based on initial sample (ByteSeek backend)
        return estimatedLines
    }

    function getVisibleLines(): Array<{ lineNumber: number; text: string }> {
        const result: Array<{ lineNumber: number; text: string }> = []
        const end = Math.min(visibleTo, estimatedTotalLines())
        for (let i = visibleFrom; i < end; i++) {
            result.push({ lineNumber: i, text: lineCache.get(i) ?? '' })
        }
        return result
    }

    /** Check if we need to fetch lines - returns true if any visible lines are missing from cache */
    function needsFetch(from: number, to: number): boolean {
        // Check a few sample points to avoid iterating the whole range
        const samplesToCheck = [from, Math.floor((from + to) / 2), to - 1]
        for (const line of samplesToCheck) {
            if (line >= 0 && !lineCache.has(line)) {
                return true
            }
        }
        return false
    }

    // Fetch lines when visible range changes (debounced)
    $effect(() => {
        const from = visibleFrom
        const to = visibleTo
        if (sessionId && needsFetch(from, to)) {
            scheduleFetch(from, to)
        }
    })

    // Track horizontal content width so .scroll-spacer can create a scrollbar
    $effect(() => {
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
    })

    function scheduleFetch(from: number, to: number) {
        // Clear any pending fetch
        if (fetchDebounceTimer) {
            clearTimeout(fetchDebounceTimer)
        }
        // Schedule new fetch after debounce
        fetchDebounceTimer = setTimeout(() => {
            void fetchLines(from, to)
        }, FETCH_DEBOUNCE_MS)
    }

    /** Update totalLines while preserving scroll fraction to prevent jump when height shrinks */
    function updateTotalLines(newTotal: number) {
        const oldEstimate = estimatedTotalLines()
        if (!contentRef || oldEstimate === 0 || newTotal === oldEstimate) {
            totalLines = newTotal
            return
        }
        // Calculate current scroll fraction before updating
        const oldHeight = oldEstimate * LINE_HEIGHT
        const scrollFraction = contentRef.scrollTop / oldHeight
        feLog(
            `[viewer] totalLines changed: ${String(oldEstimate)} -> ${String(newTotal)}, preserving scroll fraction ${scrollFraction.toFixed(3)}`,
        )
        totalLines = newTotal
        // Restore scroll fraction after DOM updates using rAF to run after browser's scroll clamping
        const newHeight = newTotal * LINE_HEIGHT
        const newScrollTop = Math.round(scrollFraction * newHeight)
        const ref = contentRef // Capture reference for rAF callback
        requestAnimationFrame(() => {
            ref.scrollTop = newScrollTop
        })
    }

    async function fetchLines(from: number, to: number) {
        if (!sessionId) return

        // Increment fetch ID to invalidate any in-flight requests
        const fetchId = ++currentFetchId

        try {
            // Fetch a larger batch to reduce round-trips
            const fetchFrom = Math.max(0, from - BUFFER_LINES)
            const fetchCount = Math.min(FETCH_BATCH, to - fetchFrom + BUFFER_LINES * 2)

            // Decide seek type based on backend capabilities
            const supportsLineSeek = capabilities?.supportsLineSeek ?? false
            const seekType = supportsLineSeek ? 'line' : 'fraction'
            const seekValue = supportsLineSeek ? fetchFrom : fetchFrom / estimatedTotalLines()

            feLog(
                `[viewer] fetchLines[${String(fetchId)}]: requesting ${seekType}=${String(seekValue)} count=${String(fetchCount)}`,
            )

            const chunk = await viewerGetLines(sessionId, seekType, seekValue, fetchCount)

            // Check if this response is still relevant (no newer fetch started)
            if (fetchId !== currentFetchId) {
                feLog(
                    `[viewer] fetchLines[${String(fetchId)}]: discarding stale response (current=${String(currentFetchId)})`,
                )
                return
            }

            // When using fraction seek, cache at the requested position since backend's line number
            // estimate may differ from frontend's estimate (different avg line length assumptions).
            // For line seek, use backend's authoritative line numbers.
            const cacheStartLine = seekType === 'fraction' ? fetchFrom : chunk.firstLineNumber

            feLog(
                `[viewer] fetchLines[${String(fetchId)}]: received ${String(chunk.lines.length)} lines, backend says firstLine=${String(chunk.firstLineNumber)}, caching at ${String(cacheStartLine)}`,
            )

            // Update cache
            for (let i = 0; i < chunk.lines.length; i++) {
                lineCache.set(cacheStartLine + i, chunk.lines[i])
            }

            // Update totalLines if backend now knows it
            if (chunk.totalLines !== null && chunk.totalLines !== totalLines) {
                updateTotalLines(chunk.totalLines)
            }
        } catch (e) {
            // Only log if this is still the current request
            if (fetchId === currentFetchId) {
                feLog(`[viewer] fetchLines[${String(fetchId)}]: failed with error ${String(e)}`)
            }
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
            await viewerSearchCancel(sessionId)
        } catch {
            // Ignore
        }
        searchStatus = 'idle'
    }

    // Indexing status polling functions
    function startIndexingPoll() {
        stopIndexingPoll()
        indexingPollTimer = setInterval(() => {
            void pollIndexingStatus()
        }, INDEXING_POLL_INTERVAL)
    }

    async function pollIndexingStatus() {
        if (!sessionId) return
        try {
            const status = await viewerGetStatus(sessionId)
            backendType = status.backendType
            isIndexing = status.isIndexing
            if (status.totalLines !== null) {
                totalLines = status.totalLines
            }
            // Stop polling when indexing is done
            if (!status.isIndexing) {
                feLog(`[viewer] Indexing finished, backendType=${status.backendType}`)
                stopIndexingPoll()
            }
        } catch {
            stopIndexingPoll()
        }
    }

    function stopIndexingPoll() {
        if (indexingPollTimer) {
            clearInterval(indexingPollTimer)
            indexingPollTimer = undefined
        }
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
            }, 100)
        } else {
            void cancelSearch()
            searchMatches = []
            currentMatchIndex = -1
        }
    })

    function closeWindow() {
        // If window isn't ready yet, queue the close for when it is
        if (!windowReady) {
            feLog('[viewer] closeWindow: window not ready, queueing close')
            closeRequested = true
            return
        }

        const start = performance.now()
        feLog('[viewer] closeWindow: starting')

        // Session cleanup first (fire-and-forget) - do this before closing
        if (sessionId) {
            viewerClose(sessionId).catch(() => {
                // Ignore - session cleanup is best-effort
            })
        }

        const currentWindow = getCurrentWindow()

        // Use double requestAnimationFrame to let WebKit finish pending content inset
        // updates before destroying the WebPageProxy. Single rAF isn't enough - we need
        // to wait for the current frame to complete AND the next frame to start.
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                feLog(`[viewer] closeWindow: calling close() after ${String(Math.round(performance.now() - start))}ms`)
                currentWindow.close().catch((e: unknown) => {
                    feLog(`[viewer] closeWindow: close failed: ${String(e)}`)
                })
            })
        })

        // NOTE: Don't call getAllWindows().setFocus() here - it can trigger WebKit to
        // recalculate content insets on the dying window, causing a crash. macOS will
        // automatically focus the main window when this one closes.
    }

    function scrollByLines(lines: number) {
        if (contentRef) {
            contentRef.scrollTop = Math.max(0, contentRef.scrollTop + lines * LINE_HEIGHT)
        }
    }

    function scrollByPages(pages: number) {
        if (contentRef) {
            const pageSize = contentRef.clientHeight - LINE_HEIGHT // Overlap by 1 line
            contentRef.scrollTop = Math.max(0, contentRef.scrollTop + pages * pageSize)
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

    /** Handle navigation keys (arrows, page up/down, home/end). Returns true if handled. */
    function handleNavigationKey(key: string): boolean {
        switch (key) {
            case 'ArrowUp':
                scrollByLines(-1)
                return true
            case 'ArrowDown':
                scrollByLines(1)
                return true
            case 'PageUp':
                scrollByPages(-1)
                return true
            case 'PageDown':
                scrollByPages(1)
                return true
            case 'Home':
                scrollToStart()
                return true
            case 'End':
                scrollToEnd()
                return true
            default:
                return false
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
            feLog(`[viewer] ESC pressed, searchVisible=${String(searchVisible)}, windowReady=${String(windowReady)}`)
            if (searchVisible) {
                closeSearch()
            } else {
                closeWindow()
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
            return
        }

        // Navigation keys (only when search input is not focused)
        const isSearchInputFocused = searchVisible && document.activeElement === searchInputRef
        if (!isSearchInputFocused && handleNavigationKey(e.key)) {
            e.preventDefault()
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

    /** Set up MCP event listeners for close/focus commands from the main window */
    async function setupMcpListeners(myFilePath: string) {
        // Listen for close requests - close this viewer if the path matches or no path is specified
        unlistenMcpClose = await listen<{ path?: string }>('mcp-viewer-close', (event) => {
            const requestedPath = event.payload.path
            if (!requestedPath || requestedPath === myFilePath) {
                feLog(`[viewer] MCP close request received for path=${requestedPath ?? 'any'}`)
                closeWindow()
            }
        })

        // Listen for focus requests - focus this viewer if the path matches
        unlistenMcpFocus = await listen<{ path?: string }>('mcp-viewer-focus', (event) => {
            const requestedPath = event.payload.path
            if (requestedPath === myFilePath) {
                feLog(`[viewer] MCP focus request received for path=${requestedPath}`)
                void getCurrentWindow().setFocus()
            }
        })
    }

    /** Clean up MCP event listeners */
    function cleanupMcpListeners() {
        unlistenMcpClose?.()
        unlistenMcpFocus?.()
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
            const result = await viewerOpen(filePath)

            sessionId = result.sessionId
            fileName = result.fileName
            totalBytes = result.totalBytes
            totalLines = result.totalLines
            estimatedLines = result.estimatedTotalLines
            backendType = result.backendType
            capabilities = result.capabilities
            isIndexing = result.isIndexing

            feLog(
                `[viewer] Opened file: ${result.fileName}, ${String(result.totalBytes)} bytes, totalLines=${String(result.totalLines)}, estimatedTotalLines=${String(result.estimatedTotalLines)}, backend=${result.backendType}, isIndexing=${String(result.isIndexing)}`,
            )

            // Start polling for indexing status if indexing is in progress
            if (result.isIndexing) {
                startIndexingPoll()
            }

            // Populate cache with initial lines
            lineCache.clear()
            for (let i = 0; i < result.initialLines.lines.length; i++) {
                lineCache.set(result.initialLines.firstLineNumber + i, result.initialLines.lines[i])
            }

            feLog(`[viewer] Initial cache: ${String(result.initialLines.lines.length)} lines loaded`)

            // Set window title (fire-and-forget, don't block)
            getCurrentWindow()
                .setTitle(`${result.fileName} — Viewer`)
                .catch(() => {
                    // Not in Tauri environment
                })

            // Set up MCP event listeners for close/focus commands
            await setupMcpListeners(filePath)
        } catch (e) {
            error = typeof e === 'string' ? e : 'Failed to read file'
            feLog(`[viewer] Failed to open file: ${String(e)}`)
        } finally {
            loading = false
            // Auto-focus the container so keyboard events work immediately
            await tick()
            containerRef?.focus()

            // Mark window as ready after WebKit has had a frame to settle
            requestAnimationFrame(() => {
                windowReady = true
                feLog(`[viewer] Window ready, closeRequested=${String(closeRequested)}`)
                if (closeRequested) {
                    closeWindow()
                }
            })
        }
    })

    onDestroy(() => {
        cleanupMcpListeners()
        stopSearchPoll()
        stopIndexingPoll()
        if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
        if (fetchDebounceTimer) clearTimeout(fetchDebounceTimer)
    })
</script>

<svelte:window on:keydown={handleKeyDown} />

<div class="viewer-container" bind:this={containerRef} tabindex={-1}>
    {#if searchVisible}
        <div class="search-bar" role="search">
            <input
                bind:this={searchInputRef}
                bind:value={searchQuery}
                type="text"
                placeholder="Find in file..."
                aria-label="Search text"
                class="search-input"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
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
            <div
                class="scroll-spacer"
                style="height: {estimatedTotalLines() * LINE_HEIGHT}px; min-width: {contentWidth}px"
            >
                <div
                    class="lines-container"
                    bind:this={linesContainerRef}
                    style="transform: translateY({visibleFrom * LINE_HEIGHT}px)"
                >
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
        {#if currentMode === 'fullLoad'}
            <span
                class="backend-badge"
                title="You have the file entirely in memory. You can quickly scroll to any line.">in memory</span
            >
        {:else if currentMode === 'lineIndex'}
            <span
                class="backend-badge"
                title="You have the file indexed, so the line numbers are accurate, and you can quickly scroll to any point."
                >indexed</span
            >
        {:else if isIndexing}
            <span
                class="backend-badge"
                title="This is a large file in streaming mode. We're building an index in background (max {INDEXING_TIMEOUT_SECS} sec)... Line numbers are currently approximate."
                >streaming, indexing...</span
            >
        {:else}
            <span
                class="backend-badge"
                title="This is a large file in streaming mode. Indexing would've taken longer than {INDEXING_TIMEOUT_SECS} sec, so we didn't do it. The line numbers are estimates."
                >streaming</span
            >
        {/if}
        <span class="shortcut-hint">Ctrl+F search &middot; Esc close</span>
    </div>
</div>

<style>
    .viewer-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        font-family: var(--font-system) sans-serif;
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
        font-family: var(--font-system) sans-serif;
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
        font-family: var(--font-mono);
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
        width: max-content;
        min-width: 100%;
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
    }

    mark {
        background: var(--color-highlight);
        border-radius: 2px;
        padding: 0 1px;
        margin: 0 -1px;
    }

    mark.active {
        background: var(--color-highlight-active);
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
