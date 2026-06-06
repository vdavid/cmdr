<script lang="ts" module>
    // Must match INDEXING_TIMEOUT_SECS in src-tauri/src/file_viewer/session.rs
    const INDEXING_TIMEOUT_SECS = 5
</script>

<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        viewerOpen,
        viewerGetLines,
        viewerClose,
        viewerSetupMenu,
        viewerSetWordWrap,
        isIpcError,
    } from '$lib/tauri-commands'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { initializeSettings, getSetting, setSetting } from '$lib/settings'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initTextSize, cleanupTextSize } from '$lib/text-size.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import { pluralize } from '$lib/utils/pluralize'
    import { createViewerSearch } from './viewer-search.svelte'
    import { createViewerScroll } from './viewer-scroll.svelte'
    import { createTextWidthTracker } from './viewer-text-width.svelte'
    import { createIndexingPoll } from './viewer-indexing-poll'
    import { createViewerKeyboard } from './viewer-keyboard'
    import { createViewerTail } from './viewer-tail.svelte'
    import {
        createViewerSelection,
        describeSelectionForAt,
        estimateSelectionBytes,
        getLineSegmentBounds,
        isWholeFileSelection,
        normaliseSelection,
    } from './selection.svelte'
    import { createViewerCopy, createViewerCopyOrchestrator } from './viewer-copy.svelte'
    import { createViewerPointerDrag } from './viewer-pointer-drag.svelte'
    import ViewerContextMenu from './ViewerContextMenu.svelte'
    import ViewerToolbar from './ViewerToolbar.svelte'
    import ViewerStatusBar from './ViewerStatusBar.svelte'
    import ViewerCopyDialogs from './ViewerCopyDialogs.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { commands } from '$lib/ipc/bindings'
    import type { EncodingChoice, FileEncoding } from '$lib/ipc/bindings'
    import type { RangeEnd } from '$lib/tauri-commands'
    import { initAppMode, decorateChildWindowTitle } from '$lib/app-mode'
    import { categorizeForViewerWarning } from '$lib/file-viewer/binary-warning'

    const log = getAppLogger('viewer')

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
    /**
     * Encoding picker state. `currentEncoding` follows the user's selection (set
     * synchronously when they pick), `detectedEncoding` is the auto-detection
     * result at open time, and `encodingChoices` is the backend-authoritative
     * dropdown list. We refetch the choices once on open; they don't change after.
     */
    let currentEncoding = $state<FileEncoding>('utf8')
    let detectedEncoding = $state<FileEncoding>('utf8')
    let encodingChoices = $state<EncodingChoice[]>([])
    let viewMode = $state<'text'>('text')

    /**
     * Tail mode: when on, the open viewport auto-follows newly appended bytes.
     * When off, the watcher's `viewer:file-changed` event surfaces a persistent
     * reload toast. The setting persists per file path (SHA-256 truncated) so
     * a log the user tailed yesterday opens tailed today.
     */
    let tailMode = $state(false)

    // Derive current mode: if we started with byteSeek but now have totalLines, we upgraded to lineIndex
    const currentMode = $derived(backendType === 'byteSeek' && totalLines !== null ? 'lineIndex' : backendType)

    const indexingPoll = createIndexingPoll({
        getSessionId: () => sessionId,
        onStatus: ({ backendType: bt, isIndexing: ind, totalLines: tl }) => {
            backendType = bt
            isIndexing = ind
            if (tl !== null) totalLines = tl
        },
    })

    const viewerTail = createViewerTail({
        getSessionId: () => sessionId,
        getTailMode: () => tailMode,
        onAppendDetected: () => {
            // Tail mode is on: the backend already extended its index in
            // response to the watcher event. Refetch the visible window so the
            // user sees the new bytes immediately, and restart the indexing
            // poll so the status bar's `totalLines` / `totalBytes` reflect the
            // new content (the BE advanced `total_bytes` inside
            // `apply_tail_extend`, but the FE state is updated by the poll).
            // The poll self-terminates once it sees `is_indexing: false` again,
            // so this is cheap.
            scroll.lineCache.clear()
            scroll.fetchVisibleNow()
            indexingPoll.start()
        },
    })

    /**
     * Flip the tail-mode flag and push the new value down to the backend. Tail
     * mode is per-session only: it defaults off on every viewer open and isn't
     * persisted across sessions. Calling without a sessionId (during startup)
     * is a no-op.
     */
    async function toggleTailMode(): Promise<void> {
        if (!sessionId) return
        const next = !tailMode
        tailMode = next
        try {
            const res = await commands.viewerSetTailMode(sessionId, next)
            if (res.status === 'error') {
                log.warn('viewer_set_tail_mode failed: {error}', { error: res.error })
                tailMode = !next
                return
            }
        } catch (e) {
            log.warn('viewer_set_tail_mode threw: {error}', { error: String(e) })
            tailMode = !next
        }
    }

    // Window lifecycle state: prevents closing before WebKit is fully initialized
    let windowReady = $state(false)
    let closeRequested = $state(false)
    let closing = false

    // Binary-file warning banner. Read the persisted suppress setting once at
    // mount; we don't reactively follow live setting changes during a single
    // viewer session (the banner is per-instance UI). `bannerDismissed` is the
    // local "Close" action; the "Never show again" action flips the setting
    // AND sets this flag for the current instance.
    let warningSuppressed = $state(false)
    let bannerDismissed = $state(false)
    const warning = $derived(categorizeForViewerWarning(fileName))
    const showWarningBanner = $derived(warning.shouldWarn && !bannerDismissed && !warningSuppressed && !loading)

    function dismissBanner(): void {
        bannerDismissed = true
    }

    function suppressBannerForever(): void {
        setSetting('fileViewer.suppressBinaryWarning', true)
        bannerDismissed = true
    }

    /**
     * Switch encoding for the current session. The backend may swap the active
     * backend (instant for same-byte-layout encoding pairs, ByteSeek + background
     * LineIndex rebuild otherwise); we follow up by restarting the indexing poll
     * so the toolbar surfaces the rebuild progress like the initial open does.
     * Clears the line cache so stale decoded strings don't linger.
     */
    async function handleEncodingChange(encoding: FileEncoding): Promise<void> {
        if (!sessionId || encoding === currentEncoding) return
        const prev = currentEncoding
        currentEncoding = encoding
        try {
            const res = await commands.viewerSetEncoding(sessionId, encoding)
            if (res.status === 'error') {
                log.error('set_encoding failed: {message}', { message: res.error })
                currentEncoding = prev
                return
            }
            scroll.lineCache.clear()
            await tick()
            // Force-fetch the current visible range under the new encoding so the
            // user doesn't have to scroll to see the re-decoded content.
            scroll.fetchVisibleNow()
            // Start polling if a background rebuild is now running. The poll
            // self-terminates when the backend reports `is_indexing: false`.
            indexingPoll.start()
        } catch (e) {
            log.error('set_encoding threw: {error}', { error: String(e) })
            currentEncoding = prev
        }
    }

    // Event listener cleanup functions
    let unlistenMcpClose: UnlistenFn | undefined
    let unlistenMcpFocus: UnlistenFn | undefined
    let unlistenWordWrap: UnlistenFn | undefined

    const textWidthTracker = createTextWidthTracker({
        getContentRef: () => scroll.contentRef,
        getVisibleLinesKey: () => scroll.visibleLines,
    })

    const scroll = createViewerScroll({
        getSessionId: () => sessionId,
        getTotalLines: () => totalLines,
        setTotalLines: (v: number) => {
            totalLines = v
        },
        getEstimatedLines: () => estimatedLines,
        getBackendType: () => backendType,
        onTimeoutError: () => {
            error = "Couldn't load the file. The volume may be slow or unresponsive."
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
        getTextWidth: () => textWidthTracker.textWidth,
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

    const selection = createViewerSelection()

    /**
     * Screen-reader-friendly announcement of the current selection. Empty string when
     * there's nothing selected (the live region stays silent). The format names the
     * selected line range and a UTF-16 character count for orientation.
     */
    const selectionAnnouncement = $derived(
        describeSelectionForAt(selection.selection, (line) => scroll.lineCache.get(line)?.length ?? null),
    )

    /**
     * Converts a `Selection` to the `(anchor, focus)` `RangeEnd`s the IPC layer accepts.
     * For ⌘A in ByteSeek-no-index mode we emit `Eof` so the backend can resolve the
     * end of the file without a fake line number; everywhere else we emit `Line { ... }`.
     */
    function getRangeEndsForCurrentSelection(): { anchor: RangeEnd; focus: RangeEnd } | null {
        const sel = selection.selection
        if (sel === null) return null
        const { start, end } = normaliseSelection(sel)
        const startEnd: RangeEnd = { kind: 'line', line: start.line, offset: start.offset }
        // In ByteSeek-no-index mode, the FE used `Infinity` (or a fake totalLines) for ⌘A.
        // Translate "selection extends past every line we know about" into RangeEnd::Eof.
        const knownTotal = totalLines
        const usesEof = knownTotal === null && end.line === Number.MAX_SAFE_INTEGER
        const endEnd: RangeEnd = usesEof
            ? { kind: 'eof' }
            : { kind: 'line', line: end.line, offset: end.offset }
        return { anchor: startEnd, focus: endEnd }
    }

    /**
     * Estimates the UTF-8 byte length of the current selection using cached line lengths.
     * Returns `null` if any required line isn't in the cache (the copy flow will route to
     * the "unknown size" branch and confirm before reading).
     */
    function estimateCurrentSelectionBytes(): number | null {
        const sel = selection.selection
        if (sel === null) return 0
        // Whole-file shortcut: ⌘A on a multi-MB file selects lines the user never scrolled
        // through, so the line cache can't service the per-line walk. `totalBytes` is the
        // exact answer (it's the file size from `viewer_open`) and avoids the bail-to-null
        // that would otherwise route to the "unknown size" confirm dialog instead of the
        // correct refuse / confirm tier.
        if (isWholeFileSelection(sel, totalLines)) {
            return totalBytes
        }
        return estimateSelectionBytes(
            sel,
            (n) => {
                const txt = scroll.lineCache.get(n)
                if (txt === undefined) return null
                // +1 for the trailing newline (the cache stores line text without newlines).
                return new TextEncoder().encode(txt).length + 1
            },
            (n) => {
                const txt = scroll.lineCache.get(n)
                if (txt === undefined) return null
                return txt.length
            },
        )
    }

    const copy = createViewerCopy({
        getSessionId: () => sessionId,
        getSelectionBytes: estimateCurrentSelectionBytes,
        getRangeEnds: getRangeEndsForCurrentSelection,
    })

    const copyFlow = createViewerCopyOrchestrator({
        copy,
        getFileName: () => fileName,
    })

    const pointerDrag = createViewerPointerDrag({
        getContentRef: () => scroll.contentRef,
        getLineText: (line) => scroll.lineCache.get(line),
        hasSelection: () => selection.selection !== null,
        setAnchor: selection.setAnchor,
        setFocus: selection.setFocus,
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
        return textWidthTracker.runResizeEffect()
    })

    // Re-measure text width when lines first appear (ResizeObserver won't fire if container size didn't change)
    $effect(() => {
        textWidthTracker.runVisibleLinesEffect()
    })

    // Debounce search input
    $effect(() => {
        search.runDebounceEffect()
    })

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

        // Defer the close() past the current event-loop iteration so the keydown
        // handler (or whichever caller invoked us) can settle before webkit2gtk
        // begins destroying this webview — the Linux GTK-main-loop-stall fix.
        // setTimeout(0) instead of nested rAFs: macOS WKWebView throttles rAF on
        // unfocused windows (e.g. the E2E case where a viewer opens without
        // grabbing focus), which can push close past the test's confirmation budget.
        setTimeout(() => {
            log.debug('closeWindow: calling close() after {elapsed}ms', {
                elapsed: Math.round(performance.now() - start),
            })
            currentWindow.close().catch((e: unknown) => {
                log.error('closeWindow: close failed: {error}', { error: String(e) })
            })
        }, 0)
    }

    function toggleWordWrap(fromMenu = false) {
        scroll.wordWrap = !scroll.wordWrap
        scroll.contentWidth = 0
        if (!fromMenu) {
            viewerSetWordWrap(getCurrentWindow().label, scroll.wordWrap).catch(() => {})
        }
        setSetting('viewer.wordWrap', scroll.wordWrap)
    }

    const keyboard = createViewerKeyboard({
        getTotalLines: () => totalLines,
        getTotalBytes: () => totalBytes,
        getLineText: (line) => scroll.lineCache.get(line),
        selection: { selectAll: selection.selectAll },
        scroll,
        search: {
            get searchVisible() {
                return search.searchVisible
            },
            get searchStatus() {
                return search.searchStatus
            },
            get searchInputRef() {
                return search.searchInputRef
            },
            openSearch: search.openSearch,
            closeSearch: search.closeSearch,
            stopSearch: search.stopSearch,
            findNext: search.findNext,
            findPrev: search.findPrev,
            toggleUseRegex: search.toggleUseRegex,
            toggleCaseSensitive: search.toggleCaseSensitive,
        },
        copy: {
            get busy() {
                return copy.busy
            },
            cancelInFlight: copy.cancelInFlight,
        },
        isCopyConfirmOpen: () => copyFlow.isConfirmOpen,
        isCopyRefuseOpen: () => copyFlow.isRefuseOpen,
        isContextMenuOpen: () => pointerDrag.contextMenuPos !== null,
        cancelCopyConfirm: copyFlow.cancelConfirm,
        dismissCopyRefuse: copyFlow.dismissRefuse,
        closeContextMenu: pointerDrag.closeContextMenu,
        logEscape: () => {
            log.debug('ESC pressed, searchVisible={searchVisible}, windowReady={windowReady}', {
                searchVisible: search.searchVisible,
                windowReady,
            })
        },
        runCopy: () => {
            void copyFlow.handleCopy()
        },
        toggleTailMode: () => {
            void toggleTailMode()
        },
        toggleWordWrap,
        closeWindow,
    })

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
        // Pass the window label so the backend can free this session when the
        // window is closed via the titlebar X (which never fires `viewerClose`).
        const result = await viewerOpen(path, getCurrentWindow().label)
        log.debug('viewer_open IPC took {ms}ms', { ms: Math.round(performance.now() - t0) })

        sessionId = result.sessionId
        fileName = result.fileName
        totalBytes = result.totalBytes
        totalLines = result.totalLines
        estimatedLines = result.estimatedTotalLines
        backendType = result.backendType
        isIndexing = result.isIndexing
        currentEncoding = result.encoding
        detectedEncoding = result.encoding
        // Fetch the dropdown options once; they don't change after open.
        void commands
            .viewerGetEncodingOptions(result.sessionId)
            .then((res) => {
                if (res.status === 'ok') {
                    encodingChoices = res.data.all
                    detectedEncoding = res.data.detected
                    currentEncoding = res.data.current
                }
            })
            .catch(() => {})

        log.debug(
            'Opened file: {fileName}, {totalBytes} {bytesNoun}, totalLines={totalLines}, estimatedTotalLines={estimatedTotalLines}, backend={backendType}, isIndexing={isIndexing}',
            {
                fileName: result.fileName,
                totalBytes: result.totalBytes,
                bytesNoun: pluralize(result.totalBytes, 'byte'),
                totalLines: result.totalLines,
                estimatedTotalLines: result.estimatedTotalLines,
                backendType: result.backendType,
                isIndexing: result.isIndexing,
            },
        )

        if (result.isIndexing) {
            indexingPoll.start()
        }

        // Subscribe to the watcher event stream. Tail mode itself starts off on
        // every open; the user re-enables it per session.
        await viewerTail.init()

        scroll.lineCache.clear()
        for (let i = 0; i < result.initialLines.lines.length; i++) {
            scroll.lineCache.set(result.initialLines.firstLineNumber + i, result.initialLines.lines[i])
        }

        log.debug('Initial cache: {count} {linesNoun} loaded', {
            count: result.initialLines.lines.length,
            linesNoun: pluralize(result.initialLines.lines.length, 'line'),
        })

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
                    log.debug('FullLoad fetch remaining {count} {linesNoun} took {ms}ms', {
                        count: chunk.lines.length,
                        linesNoun: pluralize(chunk.lines.length, 'line'),
                        ms: Math.round(performance.now() - tFetch),
                    })
                    for (let i = 0; i < chunk.lines.length; i++) {
                        scroll.lineCache.set(startLine + i, chunk.lines[i])
                    }
                })
                .catch(() => {}) // Non-critical: height map just won't activate
        }

        await initAppMode()
        getCurrentWindow()
            .setTitle(decorateChildWindowTitle(`${result.fileName} | Viewer`))
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
                error = "Couldn't load the file. The volume may be slow or unresponsive."
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

        // The viewer has no store capability (see `src-tauri/capabilities/CLAUDE.md`
        // § viewer), so settings come from the restricted-window snapshot + the
        // cross-window change events. Non-throwing: falls back to registry defaults.
        await initializeSettings({ restrictedWindow: true })
        scroll.wordWrap = getSetting('viewer.wordWrap')
        warningSuppressed = getSetting('fileViewer.suppressBinaryWarning')

        // Apply compounded text size after settings are loaded so the user's
        // persisted slider value is honored on first paint.
        await initTextSize()

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
                error = "Couldn't load the file. The volume may be slow or unresponsive."
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

            // `setTimeout(0)`, NOT `requestAnimationFrame`: macOS WKWebView
            // throttles (or fully starves) rAF in windows that opened without
            // focus, and E2E opens viewer windows with `focus: false`
            // (`open-viewer.ts`). An rAF here left `data-window-ready` stuck
            // on "loading" whenever another window had focus, timing out every
            // viewer E2E spec while a human used the machine. Third sighting
            // of this trap (settings close, viewer close, now readiness) —
            // see docs/testing.md § "rAF in unfocused windows".
            setTimeout(() => {
                windowReady = true
                log.debug('Window ready, closeRequested={closeRequested}', { closeRequested })
                if (closeRequested) {
                    closeWindow()
                }
            }, 0)
        }
    })

    onDestroy(() => {
        cleanupAccentColor()
        cleanupTextSize()
        cleanupListeners()
        search.destroy()
        scroll.destroy()
        indexingPoll.stop()
        viewerTail.destroy()
    })
</script>

<svelte:window on:keydown={keyboard.handleKeyDown} on:blur={pointerDrag.handleWindowBlur} />

<main
    class="viewer-container"
    bind:this={scroll.containerRef}
    tabindex={-1}
    data-window-ready={windowReady ? (error ? 'error' : 'loaded') : 'loading'}
    oncopy={(e: ClipboardEvent) => {
        // Intercept any copy gesture (menu Edit > Copy, ⌘C from anywhere inside the
        // viewer) so the custom selection model wins over the browser's native one.
        const target = e.target as HTMLElement | null
        if (target && (target.closest('.search-bar') || target.closest('.status-bar'))) {
            // Search input and status bar use the native selection; let it through.
            return
        }
        e.preventDefault()
        void copyFlow.handleCopy()
    }}
>
    <h1 class="sr-only">File viewer</h1>
    <ViewerToolbar
        {fileName}
        {viewMode}
        {currentEncoding}
        {detectedEncoding}
        {encodingChoices}
        {isIndexing}
        {tailMode}
        onViewModeChange={(mode: 'text') => {
            viewMode = mode
        }}
        onEncodingChange={(enc: FileEncoding) => void handleEncodingChange(enc)}
        onToggleTail={() => {
            void toggleTailMode()
        }}
    />
    <!--
        ARIA live region: announces selection state to assistive tech. Updates whenever
        the selection changes via any gesture (⌘A, drag, shift-click, double / triple-
        click, programmatic). Uses `polite` so it doesn't interrupt other speech;
        VoiceOver reads the new value after the user lands on a result.
    -->
    <div class="sr-only" aria-live="polite" aria-atomic="true">{selectionAnnouncement}</div>
    {#if showWarningBanner}
        <!--
            Banner explaining that the file viewer shows raw bytes; the user
            probably wanted Quick Look (⇧Space) or "Open in associated app"
            (Enter / double-click). Local "Close" dismisses this instance;
            "Never show this warning again" flips a persisted setting.
        -->
        <aside class="binary-warning" role="note">
            <p class="binary-warning-text">
                This is the raw view of the file. You might want to view the actual <strong>{warning.label}</strong>
                instead. To do that, close this window and press <ShortcutChip key="⇧Space" /> to open the quick view, or
                press <ShortcutChip key="Enter" /> (or double-click the file) to open it in the associated app.
            </p>
            <div class="binary-warning-actions">
                <button type="button" class="binary-warning-action" onclick={dismissBanner}>Close</button>
                <button type="button" class="binary-warning-action" onclick={suppressBannerForever}
                    >Never show this warning again</button
                >
            </div>
        </aside>
    {/if}
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
            <button
                type="button"
                class="search-toggle"
                class:active={search.caseSensitive}
                aria-pressed={search.caseSensitive}
                aria-label="Case sensitive"
                onclick={() => { search.toggleCaseSensitive(); }}
                use:tooltip={{ text: 'Case sensitive', shortcut: '⌘⌥C' }}
            >
                Aa
            </button>
            <button
                type="button"
                class="search-toggle"
                class:active={search.useRegex}
                aria-pressed={search.useRegex}
                aria-label="Regex"
                onclick={() => { search.toggleUseRegex(); }}
                use:tooltip={{ text: 'Regex', shortcut: '⌘⌥R' }}
            >
                .*
            </button>
            <span class="match-count" aria-live="polite">
                {#if search.searchStatus === 'invalidQuery'}
                    <span class="search-error" role="alert">{search.searchError}</span>
                {:else if search.searchStatus === 'running'}
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
            onpointerdown={pointerDrag.handlePointerDown}
            onpointermove={pointerDrag.handlePointerMove}
            onpointerup={pointerDrag.handlePointerUp}
            onpointercancel={pointerDrag.handlePointerCancel}
            oncontextmenu={pointerDrag.handleContextMenu}
            onclick={pointerDrag.handleClick}
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
                                >{#each search.getHighlightedSegments(lineNumber, text, getLineSegmentBounds(selection.selection, lineNumber, text.length)) as seg, segIdx (segIdx)}{#if seg.highlight}<mark
                                            class:active={seg.active}
                                            class:selected={seg.selected}>{seg.text}</mark
                                        >{:else if seg.selected}<span class="selected">{seg.text}</span>{:else}{seg.text}{/if}{/each}</span
                            >
                        </div>
                    {/each}
                </div>
            </div>
        </div>
    {/if}

    <ViewerStatusBar
        {fileName}
        {totalLines}
        {totalBytes}
        {currentMode}
        {isIndexing}
        wordWrap={scroll.wordWrap}
        indexingTimeoutSecs={INDEXING_TIMEOUT_SECS}
    />
</main>

{#if pointerDrag.contextMenuPos !== null}
    <ViewerContextMenu
        x={pointerDrag.contextMenuPos.x}
        y={pointerDrag.contextMenuPos.y}
        hasSelection={selection.selection !== null}
        onCopy={() => {
            void copyFlow.handleCopy()
        }}
        onSelectAll={keyboard.handleSelectAllShortcut}
        onClose={pointerDrag.closeContextMenu}
    />
{/if}

<ViewerCopyDialogs
    confirmBytes={copyFlow.confirmBytes}
    refuseBytes={copyFlow.refuseBytes}
    onCancelConfirm={copyFlow.cancelConfirm}
    onProceedConfirm={copyFlow.proceedConfirm}
    onDismissRefuse={copyFlow.dismissRefuse}
    onSaveAs={() => {
        void copyFlow.handleSaveAs()
    }}
/>

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

    /* Binary-file warning banner. Reuses the existing error palette
       (`--color-error-bg/text/border`) and the shared `ShortcutChip` for the
       key hints so it reads as part of Cmdr's visual language, not a one-off.
       Bottom border mirrors `.search-bar`. */
    .binary-warning {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        border-bottom: 1px solid var(--color-error-border);
        flex-shrink: 0;
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .binary-warning-text {
        margin: 0;
    }

    .binary-warning-actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-md);
    }

    /* Same shape as `LinkButton.svelte` — error-tinted to fit the banner
       (the global accent would clash with the red bg), but the rest is
       identical: underline always, no per-state recolor, same focus ring
       conventions. Both action buttons share this class; we don't fork
       "Close" vs "Never show again" visually. */
    .binary-warning-action {
        font: inherit;
        background: none;
        border: none;
        padding: 0;
        color: var(--color-error-text);
        text-decoration: underline;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- matches LinkButton convention for click affordance */
        cursor: pointer;
    }

    .binary-warning-action:hover {
        text-decoration: underline;
    }

    .binary-warning-action:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
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

    /* Search-mode toggles (Aa, .*). Use the same chrome as other search-bar
       buttons but switch background + text colour when active so the toggle
       state is visible at a glance. */
    .search-toggle {
        font-family: var(--font-mono);
        min-width: 2.2em;
    }

    .search-toggle.active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-accent-text);
    }

    .search-error {
        color: var(--color-error);
        font-size: var(--font-size-sm);
    }

    .file-content {
        flex: 1;
        overflow: auto;
        overflow-anchor: none; /* Virtual scroll manages scroll position programmatically */
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        /* The viewer owns its own selection model (see selection.svelte.ts). We
         * suppress the browser's native selection because it can't render a
         * selection that survives DOM recycling under virtual scroll. The custom
         * `.selected` class below paints the visible portion. */
        user-select: none;
        -webkit-user-select: none;
        cursor: text;
    }

    /* Selected text: gold foreground matches the file-list "selected = gold" language
     * (see design-system.md § File list). Background uses the accent-subtle token, the
     * same tint the cursor highlight uses. Both work in light and dark. */
    .line-text :global(.selected) {
        background: var(--color-accent-subtle);
        color: var(--color-selection-fg);
    }

    /* Search hit + selection on the same span: keep the highlight background (so search
     * remains the dominant signal) and apply the selection foreground colour. */
    .line-text :global(mark.selected) {
        color: var(--color-selection-fg);
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
        /* Stays in sync with `getLineHeight()` in `viewer-line-heights.svelte.ts`
         * via the `--font-scale` root variable. */
        height: calc(18px * var(--font-scale));
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
