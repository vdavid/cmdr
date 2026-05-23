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
    import { handleNavigationKey, handleToggleKey } from './viewer-keyboard'
    import {
        createViewerSelection,
        describeSelectionForAt,
        estimateSelectionBytes,
        getLineSegmentBounds,
        isWholeFileSelection,
        normaliseSelection,
    } from './selection.svelte'
    import { createViewerCopy } from './viewer-copy.svelte'
    import { caretFromPoint } from './viewer-pointer'
    import { computeAutoscrollPxPerFrame } from './viewer-autoscroll'
    import { createViewerAutoscroll } from './viewer-autoscroll.svelte'
    import ViewerContextMenu from './ViewerContextMenu.svelte'
    import { findWordBoundsAt } from './viewer-word'
    import { save as showSavePanel } from '@tauri-apps/plugin-dialog'
    import { addToast } from '$lib/ui/toast/toast-store.svelte'
    import { formatBytes, type RangeEnd } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Size from '$lib/ui/Size.svelte'
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

    /** Whether a copy confirm dialog (10 to 100 MiB band) is showing. */
    let copyConfirmBytes = $state<number | null>(null)
    let copyConfirmProceed: (() => Promise<void>) | null = null
    /** Whether the > 100 MiB refuse dialog is showing. */
    let copyRefuseBytes = $state<number | null>(null)

    /**
     * Whether a pointer drag is currently in progress. Tracks `pointerId` so we only
     * react to moves from the same pointer that started the gesture (multi-touch is a
     * future concern; today the viewer is a mouse-only surface but the type is
     * correct).
     */
    let dragPointerId: number | null = null

    /** Position of the in-app context menu while it's open, or `null`. */
    let contextMenuPos = $state<{ x: number; y: number } | null>(null)

    /** The pointer's most-recent Y position, used by the autoscroll RAF loop. */
    let dragPointerY: number = 0

    /**
     * Re-resolves the caret after each autoscroll step. Uses the X position one px
     * past the left edge of `.file-content` so the caret lands inside the line text
     * (not the line-number gutter, which sits flush to the left edge).
     */
    function reAimAfterAutoscroll(pointerY: number): void {
        if (!scroll.contentRef) return
        const rect = scroll.contentRef.getBoundingClientRect()
        const caret = caretFromPoint(document, rect.left + 1, pointerY)
        if (caret !== null) selection.setFocus(caret)
    }

    const autoscroll = createViewerAutoscroll({
        getContentRef: () => scroll.contentRef,
        getPointerY: () => dragPointerY,
        onScrollStep: reAimAfterAutoscroll,
    })

    function handleContentPointerDown(e: PointerEvent): void {
        // Left mouse button only (button 0). Right-click goes to the context menu.
        if (e.button !== 0) return
        const caret = caretFromPoint(document, e.clientX, e.clientY)
        if (caret === null) return
        e.preventDefault()

        // Shift-click extends the existing selection from its anchor to the clicked
        // position. If there's no current selection, treat shift-click as a plain click.
        if (e.shiftKey && selection.selection !== null) {
            selection.setFocus(caret)
        } else {
            selection.setAnchor(caret)
        }

        dragPointerId = e.pointerId
        dragPointerY = e.clientY
        // Capture so we keep receiving pointer events even if the cursor leaves the
        // webview (the user dragged past the edge into another macOS window or the
        // desktop). Without capture, autoscroll would never see a `pointerup` to stop.
        try {
            ;(e.currentTarget as Element | null)?.setPointerCapture(e.pointerId)
        } catch {
            // Capture can throw on some webviews if the target isn't focusable; ignoring
            // is safe (the drag still works, just without the safety net).
        }
    }

    function handleContentPointerMove(e: PointerEvent): void {
        if (dragPointerId === null || e.pointerId !== dragPointerId) return
        dragPointerY = e.clientY
        const caret = caretFromPoint(document, e.clientX, e.clientY)
        if (caret !== null) selection.setFocus(caret)

        // Check whether the pointer is near a viewport edge; start/stop autoscroll as needed.
        if (!scroll.contentRef) return
        const rect = scroll.contentRef.getBoundingClientRect()
        const delta = computeAutoscrollPxPerFrame(e.clientY, rect.top, rect.bottom)
        if (delta !== 0) {
            autoscroll.start()
        } else {
            autoscroll.stop()
        }
    }

    function endDrag(pointerId: number): void {
        if (dragPointerId !== pointerId) return
        dragPointerId = null
        autoscroll.stop()
    }

    function handleContentPointerUp(e: PointerEvent): void {
        endDrag(e.pointerId)
    }

    function handleContentPointerCancel(e: PointerEvent): void {
        endDrag(e.pointerId)
    }

    function handleContentContextMenu(e: MouseEvent): void {
        // Suppress the native OS context menu so our in-app one wins.
        e.preventDefault()
        contextMenuPos = { x: e.clientX, y: e.clientY }
    }

    /**
     * Selects the word under the pointer on double-click, or the whole line on
     * triple-click. The browser delivers consecutive clicks with `detail = 2` and
     * `detail = 3`; we read the click count from there.
     */
    function handleContentClick(e: MouseEvent): void {
        if (e.detail !== 2 && e.detail !== 3) return
        const caret = caretFromPoint(document, e.clientX, e.clientY)
        if (caret === null) return
        const lineText = scroll.lineCache.get(caret.line) ?? ''

        if (e.detail === 2) {
            const { start, end } = findWordBoundsAt(lineText, caret.offset)
            selection.setAnchor({ line: caret.line, offset: start })
            selection.setFocus({ line: caret.line, offset: end })
            return
        }

        // Triple-click: select the whole line.
        selection.setAnchor({ line: caret.line, offset: 0 })
        selection.setFocus({ line: caret.line, offset: lineText.length })
    }

    function closeContextMenu(): void {
        contextMenuPos = null
    }

    /**
     * Window `blur` safety net: macOS may hand focus to another app mid-drag without
     * firing a `pointerup` or `pointercancel`. Without this, the autoscroll RAF loop
     * would keep running indefinitely.
     */
    function handleWindowBlur(): void {
        if (dragPointerId !== null) {
            dragPointerId = null
        }
        autoscroll.stop()
    }

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

    function handleSelectAllShortcut(): void {
        if (totalLines !== null && totalLines > 0) {
            const lastLineText = scroll.lineCache.get(totalLines - 1) ?? ''
            selection.selectAll(totalLines, lastLineText.length)
            return
        }
        // ByteSeek-no-index ⌘A: we don't know `totalLines`. Use a sentinel that the
        // RangeEnd mapper translates to `RangeEnd::Eof` at the IPC boundary.
        if (totalBytes > 0) {
            selection.selectAll(Number.MAX_SAFE_INTEGER, 0)
        }
    }

    async function writeToClipboard(text: string): Promise<boolean> {
        try {
            await navigator.clipboard.writeText(text)
            return true
        } catch (e) {
            log.warn('Clipboard write rejected: {error}', { error: String(e) })
            return false
        }
    }

    async function handleSilentCopy(text: string, bytes: number): Promise<void> {
        const ok = await writeToClipboard(text)
        if (!ok) {
            addToast("Couldn't reach the clipboard. Try again?", { level: 'warn' })
            return
        }
        addToast(`${formatBytes(bytes)} on your clipboard`, { level: 'info' })
    }

    async function handleCopyShortcut(): Promise<void> {
        const outcome = await copy.runCopy()
        switch (outcome.kind) {
            case 'empty':
            case 'busy':
                return
            case 'silent':
                await handleSilentCopy(outcome.text, outcome.bytes)
                return
            case 'silent-error':
                if (outcome.reason === 'cancelled') return // user pressed Escape, intentional
                log.warn('Silent-band copy read failed: reason={reason}, error={error}', {
                    reason: outcome.reason,
                    error: outcome.error ? JSON.stringify(outcome.error) : 'none',
                })
                if (outcome.reason === 'timedOut') {
                    addToast('The read took too long. Try a smaller selection?', { level: 'warn' })
                } else {
                    addToast("Couldn't copy the selection. Try again?", { level: 'warn' })
                }
                return
            case 'confirm':
                copyConfirmBytes = outcome.bytes
                copyConfirmProceed = async () => {
                    copyConfirmBytes = null
                    const res = await outcome.proceed()
                    if (res.ok) {
                        await handleSilentCopy(res.text, outcome.bytes)
                    } else if (res.reason === 'cancelled') {
                        // User pressed Escape; no toast.
                    } else if (res.reason === 'timedOut') {
                        addToast('The read took too long. Try a smaller selection?', { level: 'warn' })
                    } else {
                        addToast("Couldn't read the selection. Try again?", { level: 'warn' })
                    }
                }
                return
            case 'unknown-size':
                // ByteSeek-no-index range we never scrolled through. Same UX as confirm,
                // but with a hint that we don't know the size yet.
                copyConfirmBytes = -1
                copyConfirmProceed = async () => {
                    copyConfirmBytes = null
                    const res = await outcome.proceed()
                    if (res.ok) {
                        const bytes = new TextEncoder().encode(res.text).length
                        await handleSilentCopy(res.text, bytes)
                    }
                }
                return
            case 'refuse':
                copyRefuseBytes = outcome.bytes
                return
        }
    }

    function cancelCopyConfirm(): void {
        copyConfirmBytes = null
        copyConfirmProceed = null
    }

    function dismissCopyRefuse(): void {
        copyRefuseBytes = null
    }

    /**
     * Save as file flow: opens the native macOS save panel via the Tauri dialog plugin
     * with a sensible default name (the open file's stem + ".selection.txt"), then
     * streams the selection to the chosen path via `viewer_write_range_to_file`.
     * Dismisses the open copy dialog and shows a success toast on completion.
     */
    async function handleSaveAs(): Promise<void> {
        const defaultName = `${fileName.replace(/\.[^.]*$/, '') || 'selection'}.selection.txt`
        let chosen: string | null
        try {
            chosen = await showSavePanel({ defaultPath: defaultName, title: 'Save selection' })
        } catch (e) {
            log.warn('Save panel rejected: {error}', { error: String(e) })
            addToast("Couldn't open the save panel. Try again?", { level: 'warn' })
            return
        }
        if (chosen === null) return // user cancelled

        // Close the open copy dialog so the user can see progress.
        copyConfirmBytes = null
        copyConfirmProceed = null
        copyRefuseBytes = null

        const res = await copy.saveAs(chosen)
        if (res.ok) {
            addToast(`Selection saved to ${chosen.split('/').pop() ?? chosen}`, { level: 'info' })
        } else if (res.reason === 'cancelled') {
            // No toast; the user pressed Escape.
        } else if (res.reason === 'timedOut') {
            addToast('Saving took too long. Try a smaller selection?', { level: 'warn' })
        } else {
            addToast("Couldn't save the selection. Try again?", { level: 'warn' })
        }
    }

    function handleEscapeKey(): void {
        log.debug('ESC pressed, searchVisible={searchVisible}, windowReady={windowReady}', {
            searchVisible: search.searchVisible,
            windowReady,
        })
        if (!search.searchVisible) {
            closeWindow()
            return
        }
        if (search.searchStatus === 'running') {
            search.stopSearch()
        } else {
            search.closeSearch()
        }
    }

    /**
     * Routes Escape to the right cancel surface in priority order: open context menu
     * (the menu owns its own Escape too, but we short-circuit here so the page's
     * `closeWindow()` path doesn't fire after the menu closes itself), then in-flight
     * copy read, then any open copy dialog, then the search bar logic.
     *
     * Returns `true` if Escape was consumed here.
     */
    function tryConsumeEscapeForCopy(): boolean {
        if (contextMenuPos !== null) {
            closeContextMenu()
            return true
        }
        if (copy.busy) {
            void copy.cancelInFlight()
            return true
        }
        if (copyConfirmBytes !== null) {
            cancelCopyConfirm()
            return true
        }
        if (copyRefuseBytes !== null) {
            dismissCopyRefuse()
            return true
        }
        return false
    }

    /**
     * Handles ⌘/Ctrl-prefixed shortcuts inside the viewer. Returns `true` if the key
     * was consumed; the caller falls through to other handlers when it returns `false`.
     * Defers to the browser's native ⌘A / ⌘C when the search input is focused.
     */
    function handleModifierShortcut(e: KeyboardEvent, searchInputFocused: boolean): boolean {
        if (searchInputFocused) {
            // Only ⌘F here; ⌘A / ⌘C go to the input's native handler.
            if (e.key === 'f') {
                e.preventDefault()
                search.openSearch()
                return true
            }
            return false
        }
        if (e.key === 'a') {
            e.preventDefault()
            handleSelectAllShortcut()
            return true
        }
        if (e.key === 'c') {
            e.preventDefault()
            void handleCopyShortcut()
            return true
        }
        if (e.key === 'f') {
            e.preventDefault()
            search.openSearch()
            return true
        }
        return false
    }

    function handleKeyDown(e: KeyboardEvent) {
        const searchInputFocused = search.searchVisible && document.activeElement === search.searchInputRef

        if ((e.metaKey || e.ctrlKey) && handleModifierShortcut(e, searchInputFocused)) return

        if (e.key === 'Escape') {
            e.preventDefault()
            if (tryConsumeEscapeForCopy()) return
            handleEscapeKey()
            return
        }

        if (e.key === 'Enter' && search.searchVisible) {
            e.preventDefault()
            if (e.shiftKey) search.findPrev()
            else search.findNext()
            return
        }

        if (searchInputFocused) return

        if (handleToggleKey(e, toggleWordWrap) || handleNavigationKey(e.key, scroll)) {
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

        try {
            await initializeSettings()
            scroll.wordWrap = getSetting('viewer.wordWrap')
            warningSuppressed = getSetting('fileViewer.suppressBinaryWarning')
        } catch {
            // Settings store not available in this context, use defaults
        }

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
        cleanupTextSize()
        cleanupListeners()
        search.destroy()
        scroll.destroy()
        indexingPoll.stop()
    })
</script>

<svelte:window on:keydown={handleKeyDown} on:blur={handleWindowBlur} />

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
        void handleCopyShortcut()
    }}
>
    <h1 class="sr-only">File viewer</h1>
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
                instead. To do that, close this window and press <kbd>⇧Space</kbd> to open the quick view, or press
                <kbd>Enter</kbd> (or double-click the file) to open it in the associated app.
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
            onpointerdown={handleContentPointerDown}
            onpointermove={handleContentPointerMove}
            onpointerup={handleContentPointerUp}
            onpointercancel={handleContentPointerCancel}
            oncontextmenu={handleContentContextMenu}
            onclick={handleContentClick}
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

    <div class="status-bar" aria-label="File information">
        <span>{fileName}</span>
        {#if totalLines !== null}
            <span>{totalLines} {totalLines === 1 ? 'line' : 'lines'}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
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
        <span class="shortcut-hint">W wrap &middot; ⌘A select all &middot; ⌘C copy &middot; ⌘F search &middot; Esc close</span>
    </div>
</main>

{#if contextMenuPos !== null}
    <ViewerContextMenu
        x={contextMenuPos.x}
        y={contextMenuPos.y}
        hasSelection={selection.selection !== null}
        onCopy={() => {
            void handleCopyShortcut()
        }}
        onSelectAll={handleSelectAllShortcut}
        onClose={closeContextMenu}
    />
{/if}

{#if copyConfirmBytes !== null}
    {@const confirmBytes = copyConfirmBytes}
    <ModalDialog
        dialogId="viewer-copy-confirm"
        titleId="viewer-copy-confirm-title"
        onclose={cancelCopyConfirm}
        containerStyle="max-width: 480px"
    >
        {#snippet title()}
            <h2 id="viewer-copy-confirm-title" class="copy-dialog-title">
                {#if confirmBytes === -1}
                    Copy this selection to the clipboard?
                {:else}
                    Copy {formatBytes(confirmBytes)} to the clipboard?
                {/if}
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">
                Large pastes can slow down other apps. Try search (⌘F) to narrow it down.
            </p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={cancelCopyConfirm}>Cancel</Button>
                <Button
                    variant="secondary"
                    onclick={() => {
                        void handleSaveAs()
                    }}>Save as file…</Button
                >
                <Button
                    variant="primary"
                    autoFocus
                    onclick={() => {
                        if (copyConfirmProceed) void copyConfirmProceed()
                    }}>Copy</Button
                >
            </div>
        </div>
    </ModalDialog>
{/if}

{#if copyRefuseBytes !== null}
    {@const refuseBytes = copyRefuseBytes}
    <ModalDialog
        dialogId="viewer-copy-refuse"
        titleId="viewer-copy-refuse-title"
        onclose={dismissCopyRefuse}
        containerStyle="max-width: 480px"
    >
        {#snippet title()}
            <h2 id="viewer-copy-refuse-title" class="copy-dialog-title">
                Copy {formatBytes(refuseBytes)} to the clipboard?
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">
                That's larger than the 100 MB clipboard limit. Try search (⌘F) to find what you need, or save the
                selection as a file.
            </p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={dismissCopyRefuse}>Cancel</Button>
                <Button
                    variant="primary"
                    autoFocus
                    onclick={() => {
                        void handleSaveAs()
                    }}>Save as file…</Button
                >
            </div>
        </div>
    </ModalDialog>
{/if}

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
       (`--color-error-bg/text/border`) and the project-standard inline
       `<kbd>` + link-button conventions (see `lib/ui/LinkButton.svelte` and
       the MTP / Quick Look hint toasts) so it reads as part of Cmdr's
       visual language, not a one-off. Bottom border mirrors `.search-bar`. */
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

    /* Matches the `<kbd>` styling in the Quick Look hint toast
       (`QuickLookHintToastContent.svelte`): tertiary background, primary
       text color. Reads as a key inset across both modes without needing
       a special palette per banner color. */
    .binary-warning-text kbd {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        white-space: nowrap;
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
        /* Opt back in to native selection here so users can copy the file name or line
         * count. The global reset is `user-select: none`, and `.file-content` keeps
         * that for its custom selection model; the status bar is plain chrome. */
        user-select: text;
        -webkit-user-select: text;
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

    .copy-dialog-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        text-align: center;
        margin: 0;
    }

    /* Matches the AlertDialog body wrapper: design-system § Dialogs body padding 0 24px 24px. */
    .copy-dialog-body-wrap {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .copy-dialog-body {
        font-size: var(--font-size-md);
        line-height: 1.4;
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-xl);
    }

    .copy-dialog-actions {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }
</style>
