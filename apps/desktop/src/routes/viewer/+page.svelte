<script lang="ts" module>
    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }

    // Must match INDEXING_TIMEOUT_SECS in src-tauri/src/file_viewer/session.rs
    const INDEXING_TIMEOUT_SECS = 5
</script>

<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        viewerOpen,
        viewerGetLines,
        viewerGetStatus,
        viewerClose,
        viewerSetupMenu,
        viewerSetWordWrap,
        isIpcError,
    } from '$lib/tauri-commands'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { initializeSettings, getSetting, setSetting } from '$lib/settings'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import { createViewerSearch } from './viewer-search.svelte'
    import { createViewerScroll } from './viewer-scroll.svelte'

    const log = getAppLogger('viewer')

    let textWidth = $state(0)
    let fileName = $state('')
    let totalLines = $state<number | null>(null)
    let estimatedLines = $state(1) // Backend's estimate based on initial sample
    let totalBytes = $state(0)
    let error = $state('')
    let errorIsTimeout = $state(false)
    let filePath = $state('')
    let loading = $state(true)
    let sessionId = $state('')
    let backendType = $state<'fullLoad' | 'byteSeek' | 'lineIndex'>('fullLoad')
    let isIndexing = $state(false)

    // Derive current mode: if we started with byteSeek but now have totalLines, we upgraded to lineIndex
    const currentMode = $derived(backendType === 'byteSeek' && totalLines !== null ? 'lineIndex' : backendType)

    // Indexing status polling
    let indexingPollTimer: ReturnType<typeof setInterval> | undefined
    const INDEXING_POLL_INTERVAL = 500

    // Window lifecycle state: prevents closing before WebKit is fully initialized
    let windowReady = $state(false)
    let closeRequested = $state(false)
    let closing = false

    // Event listener cleanup functions
    let unlistenMcpClose: UnlistenFn | undefined
    let unlistenMcpFocus: UnlistenFn | undefined
    let unlistenWordWrap: UnlistenFn | undefined

    const scroll = createViewerScroll({
        getSessionId: () => sessionId,
        getTotalLines: () => totalLines,
        setTotalLines: (v: number) => {
            totalLines = v
        },
        getEstimatedLines: () => estimatedLines,
        getBackendType: () => backendType,
        onTimeoutError: () => {
            error = "Couldn't load the file — the volume may be slow or unresponsive."
            errorIsTimeout = true
        },
        getAllLines: () => {
            if (backendType !== 'fullLoad') return null
            const total = totalLines
            if (total === null || total === 0) return null
            if (!scroll.lineCache.has(0) || !scroll.lineCache.has(total - 1)) return null
            const lines: string[] = new Array<string>(total)
            for (let i = 0; i < total; i++) {
                lines[i] = scroll.lineCache.get(i) ?? ''
            }
            return lines
        },
        getTextWidth: () => textWidth,
    })

    const search = createViewerSearch({
        getSessionId: () => sessionId,
        getTotalBytes: () => totalBytes,
        getTotalLines: () => totalLines,
        getEstimatedTotalLines: () => scroll.estimatedTotalLines(),
        getScrollLineHeight: () => scroll.scrollLineHeight,
        getLineTop: (n: number) => scroll.getLineTop(n),
        getViewportHeight: () => scroll.viewportHeight,
        getContentRef: () => scroll.contentRef,
    })

    // Fetch lines when visible range changes (debounced)
    $effect(() => {
        scroll.runFetchEffect()
    })

    // Track horizontal content width so .scroll-spacer can create a scrollbar
    $effect(() => {
        return scroll.runContentWidthEffect()
    })

    // Measure average wrapped line height for virtual scroll approximation
    $effect(() => {
        return scroll.runWrappedLineHeightEffect()
    })

    // Compensate scroll position when scrollLineHeight changes
    $effect(() => {
        scroll.runScrollCompensationEffect()
    })

    // Height map: trigger preparation when word wrap + fullLoad lines + textWidth are available
    $effect(() => {
        scroll.runHeightMapInitEffect()
    })

    // Height map: reflow when textWidth changes
    $effect(() => {
        scroll.runHeightMapReflowEffect()
    })

    // Track available text width for height map calculations via ResizeObserver + visible lines change
    $effect(() => {
        const ref = scroll.contentRef
        if (!ref) return
        const el = ref // Capture non-null ref for closures

        function measureTextWidth() {
            const lineText = el.querySelector('.line-text')
            if (lineText) {
                const w = lineText.getBoundingClientRect().width
                if (w > 0 && Math.abs(w - textWidth) > 1) {
                    textWidth = w
                }
            }
        }

        const observer = new ResizeObserver(() => {
            measureTextWidth()
        })
        observer.observe(el)

        // Initial measurement after mount
        requestAnimationFrame(() => {
            measureTextWidth()
        })

        return () => {
            observer.disconnect()
        }
    })

    // Re-measure text width when lines first appear (ResizeObserver won't fire if container size didn't change)
    $effect(() => {
        void scroll.visibleLines // Track when lines change
        if (textWidth > 0) return // Already measured
        requestAnimationFrame(() => {
            const ref = scroll.contentRef
            if (!ref) return
            const lineText = ref.querySelector('.line-text')
            if (lineText) {
                const w = lineText.getBoundingClientRect().width
                if (w > 0) {
                    textWidth = w
                }
            }
        })
    })

    // Debounce search input
    $effect(() => {
        search.runDebounceEffect()
    })

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
            if (!status.isIndexing) {
                log.info('Indexing finished, backendType={backendType}', { backendType: status.backendType })
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

    function closeWindow() {
        if (closing) return
        if (!windowReady) {
            log.debug('closeWindow: window not ready, queueing close')
            closeRequested = true
            return
        }
        closing = true

        const start = performance.now()
        log.debug('closeWindow: starting')

        if (sessionId) {
            viewerClose(sessionId).catch(() => {})
        }

        const currentWindow = getCurrentWindow()

        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                log.debug('closeWindow: calling close() after {elapsed}ms', {
                    elapsed: Math.round(performance.now() - start),
                })
                currentWindow.close().catch((e: unknown) => {
                    log.error('closeWindow: close failed: {error}', { error: String(e) })
                })
            })
        })
    }

    function toggleWordWrap(fromMenu = false) {
        scroll.wordWrap = !scroll.wordWrap
        scroll.contentWidth = 0
        if (!fromMenu) {
            viewerSetWordWrap(getCurrentWindow().label, scroll.wordWrap).catch(() => {})
        }
        setSetting('viewer.wordWrap', scroll.wordWrap)
    }

    function handleToggleKey(e: KeyboardEvent): boolean {
        if (e.key.toLowerCase() === 'w' && !e.metaKey && !e.ctrlKey && !e.altKey) {
            toggleWordWrap()
            return true
        }
        return false
    }

    function handleNavigationKey(key: string): boolean {
        switch (key) {
            case 'ArrowUp':
                scroll.scrollByLines(-1)
                return true
            case 'ArrowDown':
                scroll.scrollByLines(1)
                return true
            case 'PageUp':
                scroll.scrollByPages(-1)
                return true
            case 'PageDown':
                scroll.scrollByPages(1)
                return true
            case 'Home':
                scroll.scrollToStart()
                return true
            case 'End':
                scroll.scrollToEnd()
                return true
            default:
                return false
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
            e.preventDefault()
            search.openSearch()
            return
        }

        if (e.key === 'Escape') {
            e.preventDefault()
            log.debug('ESC pressed, searchVisible={searchVisible}, windowReady={windowReady}', {
                searchVisible: search.searchVisible,
                windowReady,
            })
            if (search.searchVisible) {
                if (search.searchStatus === 'running') {
                    search.stopSearch()
                } else {
                    search.closeSearch()
                }
            } else {
                closeWindow()
            }
            return
        }

        if (e.key === 'Enter' && search.searchVisible) {
            e.preventDefault()
            if (e.shiftKey) {
                search.findPrev()
            } else {
                search.findNext()
            }
            return
        }

        if (search.searchVisible && document.activeElement === search.searchInputRef) return

        if (handleToggleKey(e) || handleNavigationKey(e.key)) {
            e.preventDefault()
        }
    }

    async function setupMcpListeners(myFilePath: string) {
        unlistenMcpClose = await listen<{ path?: string }>('mcp-viewer-close', (event) => {
            const requestedPath = event.payload.path
            if (!requestedPath || requestedPath === myFilePath) {
                log.debug('MCP close request received for path={path}', { path: requestedPath ?? 'any' })
                closeWindow()
            }
        })

        unlistenMcpFocus = await listen<{ path?: string }>('mcp-viewer-focus', (event) => {
            const requestedPath = event.payload.path
            if (requestedPath === myFilePath) {
                log.debug('MCP focus request received for path={path}', { path: requestedPath })
                void getCurrentWindow().setFocus()
            }
        })
    }

    async function openViewerSession(path: string) {
        const t0 = performance.now()
        const result = await viewerOpen(path)
        log.debug('viewer_open IPC took {ms}ms', { ms: Math.round(performance.now() - t0) })

        sessionId = result.sessionId
        fileName = result.fileName
        totalBytes = result.totalBytes
        totalLines = result.totalLines
        estimatedLines = result.estimatedTotalLines
        backendType = result.backendType
        isIndexing = result.isIndexing

        log.debug(
            'Opened file: {fileName}, {totalBytes} bytes, totalLines={totalLines}, estimatedTotalLines={estimatedTotalLines}, backend={backendType}, isIndexing={isIndexing}',
            {
                fileName: result.fileName,
                totalBytes: result.totalBytes,
                totalLines: result.totalLines,
                estimatedTotalLines: result.estimatedTotalLines,
                backendType: result.backendType,
                isIndexing: result.isIndexing,
            },
        )

        if (result.isIndexing) {
            startIndexingPoll()
        }

        scroll.lineCache.clear()
        for (let i = 0; i < result.initialLines.lines.length; i++) {
            scroll.lineCache.set(result.initialLines.firstLineNumber + i, result.initialLines.lines[i])
        }

        log.debug('Initial cache: {count} lines loaded', { count: result.initialLines.lines.length })

        // For FullLoad files, fetch ALL lines so the height map can prepare them.
        // The initial chunk only contains ~200 lines, but FullLoad files are <1MB so
        // fetching the rest in one IPC call is trivial.
        if (
            result.backendType === 'fullLoad' &&
            result.totalLines !== null &&
            result.initialLines.lines.length < result.totalLines
        ) {
            const remaining = result.totalLines - result.initialLines.lines.length
            const startLine = result.initialLines.firstLineNumber + result.initialLines.lines.length
            const tFetch = performance.now()
            viewerGetLines(result.sessionId, 'line', startLine, remaining)
                .then((chunk) => {
                    log.debug('FullLoad fetch remaining {count} lines took {ms}ms', {
                        count: chunk.lines.length,
                        ms: Math.round(performance.now() - tFetch),
                    })
                    for (let i = 0; i < chunk.lines.length; i++) {
                        scroll.lineCache.set(startLine + i, chunk.lines[i])
                    }
                })
                .catch(() => {}) // Non-critical — height map just won't activate
        }

        getCurrentWindow()
            .setTitle(`${result.fileName} — Viewer`)
            .catch(() => {})

        await setupMcpListeners(path)

        const windowLabel = getCurrentWindow().label
        viewerSetupMenu(windowLabel)
            .then(() => {
                if (scroll.wordWrap) viewerSetWordWrap(windowLabel, true).catch(() => {})
            })
            .catch(() => {})

        unlistenWordWrap = await listen('viewer-word-wrap-toggled', () => {
            toggleWordWrap(true)
        })

        error = ''
        errorIsTimeout = false
    }

    async function retryOpen() {
        if (!filePath) return
        loading = true
        error = ''
        errorIsTimeout = false
        try {
            await openViewerSession(filePath)
        } catch (e) {
            if (isIpcError(e) && e.timedOut) {
                error = "Couldn't load the file — the volume may be slow or unresponsive."
                errorIsTimeout = true
            } else {
                error = typeof e === 'string' ? e : isIpcError(e) ? e.message : 'Failed to read file'
                errorIsTimeout = false
            }
            log.error('Retry failed: {error}', { error: String(e) })
        } finally {
            loading = false
            await tick()
            scroll.containerRef?.focus()
        }
    }

    function cleanupListeners() {
        unlistenMcpClose?.()
        unlistenMcpFocus?.()
        unlistenWordWrap?.()
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        await initAccentColor()

        try {
            await initializeSettings()
            scroll.wordWrap = getSetting('viewer.wordWrap')
        } catch {
            // Settings store not available in this context, use defaults
        }

        const params = new URLSearchParams(window.location.search)
        const pathParam = params.get('path')

        if (!pathParam) {
            error = 'No file path specified'
            errorIsTimeout = false
            loading = false
            return
        }

        filePath = pathParam

        try {
            await openViewerSession(pathParam)
        } catch (e) {
            if (isIpcError(e) && e.timedOut) {
                error = "Couldn't load the file — the volume may be slow or unresponsive."
                errorIsTimeout = true
            } else {
                error = typeof e === 'string' ? e : isIpcError(e) ? e.message : 'Failed to read file'
                errorIsTimeout = false
            }
            log.error('Failed to open file: {error}', { error: String(e) })
        } finally {
            loading = false
            await tick()
            scroll.containerRef?.focus()

            requestAnimationFrame(() => {
                windowReady = true
                log.debug('Window ready, closeRequested={closeRequested}', { closeRequested })
                if (closeRequested) {
                    closeWindow()
                }
            })
        }
    })

    onDestroy(() => {
        cleanupAccentColor()
        cleanupListeners()
        search.destroy()
        scroll.destroy()
        stopIndexingPoll()
    })
</script>

<svelte:window on:keydown={handleKeyDown} />

<main class="viewer-container" bind:this={scroll.containerRef} tabindex={-1}>
    <h1 class="sr-only">File viewer</h1>
    {#if search.searchVisible}
        <div class="search-bar" role="search">
            <input
                bind:this={search.searchInputRef}
                bind:value={search.searchQuery}
                type="text"
                placeholder="Find in file..."
                aria-label="Search text"
                class="search-input"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
            />
            <span class="match-count" aria-live="polite">
                {#if search.searchStatus === 'running'}
                    <span class="spinner spinner-sm search-spinner" aria-hidden="true"></span>
                    {#if search.searchMatches.length > 0}
                        {search.currentMatchIndex + 1} of {search.searchMatches.length}{search.searchLimitReached
                            ? '+'
                            : ''}
                        &middot; {Math.round(search.searchProgress * 100)}%
                    {:else}
                        Searching... {Math.round(search.searchProgress * 100)}%
                    {/if}
                {:else if search.searchMatches.length > 0}
                    {search.currentMatchIndex + 1} of {search.searchMatches.length}{search.searchLimitReached
                        ? '+'
                        : ''}
                    {#if search.searchStatus === 'cancelled'}
                        (partial)
                    {/if}
                {:else if search.searchQuery && (search.searchStatus === 'done' || search.searchStatus === 'cancelled')}
                    No matches{search.searchStatus === 'cancelled' ? ' (partial)' : ''}
                {/if}
            </span>
            {#if search.searchStatus === 'running'}
                <button
                    onclick={() => {
                        search.stopSearch()
                    }}
                    aria-label="Stop searching"
                    use:tooltip={'Stop scanning and keep results'}>&#x25A0;</button
                >
            {/if}
            <button
                onclick={() => {
                    search.findPrev()
                }}
                disabled={search.searchMatches.length === 0}
                aria-label="Previous match"
                use:tooltip={{ text: 'Previous match', shortcut: '⇧Enter' }}>&#x25B2;</button
            >
            <button
                onclick={() => {
                    search.findNext()
                }}
                disabled={search.searchMatches.length === 0}
                aria-label="Next match"
                use:tooltip={{ text: 'Next match', shortcut: 'Enter' }}>&#x25BC;</button
            >
            <button
                onclick={() => {
                    search.closeSearch()
                }}
                aria-label="Close search"
                use:tooltip={{ text: 'Close', shortcut: 'Esc' }}>&#x2715;</button
            >
            {#if search.searchStatus === 'running'}
                <div
                    class="search-progress"
                    role="progressbar"
                    aria-valuenow={Math.round(search.searchProgress * 100)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                >
                    <div class="search-progress-fill" style="width: {search.searchProgress * 100}%"></div>
                </div>
            {/if}
        </div>
    {/if}

    {#if loading}
        <div class="status-message">Loading...</div>
    {:else if error && errorIsTimeout}
        <div class="status-message timeout-error" role="alert">
            <p class="timeout-error-message">{error}</p>
            <div class="timeout-error-actions">
                <button class="viewer-action-btn" onclick={() => void retryOpen()}>Retry</button>
                <button class="viewer-action-btn viewer-action-secondary" onclick={closeWindow}>Cancel</button>
            </div>
        </div>
    {:else if error}
        <div class="status-message error">{error}</div>
    {:else}
        <div
            class="file-content"
            class:word-wrap={scroll.wordWrap}
            role="document"
            tabindex="0"
            aria-label="File content: {fileName}"
            bind:this={scroll.contentRef}
            onscroll={scroll.handleScroll}
        >
            <div
                class="scroll-spacer"
                style="height: {scroll.spacerHeight}px; min-width: {scroll.wordWrap
                    ? 0
                    : scroll.contentWidth}px"
            >
                <div
                    class="lines-container"
                    bind:this={scroll.linesContainerRef}
                    style="transform: translateY({scroll.linesOffset}px)"
                >
                    {#each scroll.visibleLines as { lineNumber, text } (lineNumber)}
                        <div class="line" data-line={lineNumber}>
                            <span class="line-number" style="width: {scroll.gutterWidth}ch" aria-hidden="true"
                                >{lineNumber + 1}</span
                            >
                            <span class="line-text"
                                >{#each search.getHighlightedSegments(lineNumber, text) as seg, segIdx (segIdx)}{#if seg.highlight}<mark
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
                use:tooltip={'You have the file entirely in memory. You can quickly scroll to any line.'}
                >in memory</span
            >
        {:else if currentMode === 'lineIndex'}
            <span
                class="backend-badge"
                use:tooltip={'You have the file indexed, so the line numbers are accurate, and you can quickly scroll to any point.'}
                >indexed</span
            >
        {:else if isIndexing}
            <span
                class="backend-badge"
                use:tooltip={`This is a large file in streaming mode. We're building an index in background (max ${String(INDEXING_TIMEOUT_SECS)} sec)... Line numbers are currently approximate.`}
                >streaming, indexing...</span
            >
        {:else}
            <span
                class="backend-badge"
                use:tooltip={`This is a large file in streaming mode. Indexing would've taken longer than ${String(INDEXING_TIMEOUT_SECS)} sec, so we didn't do it. The line numbers are estimates.`}
                >streaming</span
            >
        {/if}
        {#if scroll.wordWrap}
            <span class="backend-badge" use:tooltip={{ text: 'Lines wrap at the window edge', shortcut: 'W' }}
                >wrap</span
            >
        {/if}
        <span class="shortcut-hint">W wrap &middot; Ctrl+F search &middot; Esc close</span>
    </div>
</main>

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
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        flex-shrink: 0;
    }

    .search-input {
        flex: 1;
        max-width: 300px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
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
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        min-width: 70px;
        white-space: nowrap;
    }

    .search-spinner {
        vertical-align: text-bottom;
        margin-right: var(--spacing-xxs);
    }

    .search-progress {
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 2px;
        background: var(--color-bg-tertiary);
        overflow: hidden;
    }

    .search-progress-fill {
        height: 100%;
        background: var(--color-accent);
        transition: width var(--transition-base);
    }

    @media (prefers-reduced-motion: reduce) {
        .search-progress-fill {
            transition: none;
        }
    }

    .search-bar button {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        padding: var(--spacing-xxs) var(--spacing-xs);
        line-height: 1;
        transition: all var(--transition-base);
    }

    .search-bar button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .search-bar button:hover:not(:disabled) {
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
    }

    .search-bar button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    .file-content {
        flex: 1;
        overflow: auto;
        overflow-anchor: none; /* Virtual scroll manages scroll position programmatically */
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
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
        background: var(--color-bg-tertiary);
    }

    .line-number {
        display: inline-block;
        text-align: right;
        color: var(--color-text-tertiary);
        padding-right: var(--spacing-sm);
        margin-right: var(--spacing-sm);
        border-right: 1px solid var(--color-border-subtle);
        flex-shrink: 0;
        user-select: none;
        -webkit-user-select: none;
    }

    .line-text {
        white-space: pre;
    }

    .word-wrap {
        overflow-x: hidden;
    }

    .word-wrap .lines-container {
        width: auto;
        right: 0;
    }

    .word-wrap .line {
        height: auto;
    }

    .word-wrap .line-text {
        white-space: pre-wrap;
        overflow-wrap: break-word;
    }

    mark {
        background: var(--color-highlight);
        border-radius: var(--radius-xs);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 0 1px;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        margin: 0 -1px;
    }

    mark.active {
        background: var(--color-highlight-active);
    }

    .status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-lg);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-strong);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .backend-badge {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .shortcut-hint {
        margin-left: auto;
        color: var(--color-text-tertiary);
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

    .status-message.timeout-error {
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .timeout-error-message {
        margin: 0;
        color: var(--color-warning);
        font-size: var(--font-size-md);
        line-height: 1.4;
        text-align: center;
    }

    .timeout-error-actions {
        display: flex;
        gap: var(--spacing-sm);
    }

    .viewer-action-btn {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Button height target: matches mini button */
        padding: 3px 12px;
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        border-radius: var(--radius-sm);
        background: var(--color-warning);
        color: var(--color-accent-fg);
        border: none;
        transition: all var(--transition-base);
    }

    .viewer-action-btn:hover {
        filter: brightness(1.1);
    }

    .viewer-action-btn:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }

    .viewer-action-secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .viewer-action-secondary:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        filter: none;
    }
</style>
