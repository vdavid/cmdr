<script lang="ts" module>
    // Must match INDEXING_TIMEOUT_SECS in src-tauri/src/file_viewer/session.rs
    const INDEXING_TIMEOUT_SECS = 5
</script>

<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        viewerOpen,
        viewerOpenAsText,
        viewerGetLines,
        viewerClose,
        viewerSetupMenu,
        viewerSetWordWrap,
        viewerSetSearchInputFocused,
        onViewerPullProgress,
        onViewerWordWrapToggled,
        onViewerEditAction,
        activateWindowMenu,
    } from '$lib/tauri-commands'
    import { createViewerPull } from './viewer-pull.svelte'
    import PullProgressPanel from './PullProgressPanel.svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { getSetting, setSetting } from '$lib/settings'
    import { initWindowSettings, initWindowLanguageSync } from '$lib/settings/window-settings'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initReduceTransparency, cleanupReduceTransparency } from '$lib/reduce-transparency'
    import { initTextSize, cleanupTextSize } from '$lib/text-size.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import { pluralize } from '$lib/utils/pluralize'
    import { closeSelfWindow } from '$lib/child-window-close'
    import { createViewerSearch } from './viewer-search.svelte'
    import { createViewerScroll } from './viewer-scroll.svelte'
    import { createTextWidthTracker } from './viewer-text-width.svelte'
    import { createIndexingPoll } from './viewer-indexing-poll'
    import { handleOpenFailure } from './viewer-open-failure'
    import { createViewerKeyboard, isSearchInputFocused } from './viewer-keyboard'
    import { runViewerEditAction } from './viewer-menu-actions'
    import { createViewerTail } from './viewer-tail.svelte'
    import {
        createViewerSelection,
        describeSelectionForAt,
        estimateSelectionBytes,
        getRowSegmentBounds,
        rowMetrics,
        selectionBytesFromFileSize,
        toRangeEnds,
    } from './selection.svelte'
    import { createViewerCopy, createViewerCopyOrchestrator } from './viewer-copy.svelte'
    import { createViewerPointerDrag } from './viewer-pointer-drag.svelte'
    import { createViewerTextCursor } from './viewer-text-cursor.svelte'
    import ViewerTextCursor from './ViewerTextCursor.svelte'
    import { getViewerShowTextCursor } from '$lib/settings/reactive-settings.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import ViewerContextMenu from './ViewerContextMenu.svelte'
    import ViewerToolbar from './ViewerToolbar.svelte'
    import ViewerStatusBar from './ViewerStatusBar.svelte'
    import ViewerRow from './ViewerRow.svelte'
    import RawByteView from './RawByteView.svelte'
    import { modeForKey, type ViewerDisplayMode } from './viewer-view-mode'
    import { availableMediaKind } from './media-view'
    import ViewerCopyDialogs from './ViewerCopyDialogs.svelte'
    import MediaImageView from './MediaImageView.svelte'
    import MediaPdfView from './MediaPdfView.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import type { EncodingChoice, FileEncoding } from '$lib/ipc/bindings'
    import { viewerSetEncoding, viewerSetTailMode, viewerGetEncodingOptions } from '$lib/tauri-commands'
    import { initAppMode, decorateChildWindowTitle } from '$lib/app-mode'
    import { categorizeForViewerWarning } from '$lib/file-viewer/binary-warning'
    import { isMediaKind } from './media-view'
    import { createViewerMedia } from './viewer-media.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'

    const log = getAppLogger('viewer')

    let fileName = $state('')
    /**
     * Counted ROWS, or `null` while only an estimate exists (ByteSeek before its index
     * lands). The viewer's scroll coordinate.
     */
    let totalRows = $state<number | null>(null)
    /** Rows, always available: exact on `fullLoad` / `lineIndex`, sampled on `byteSeek`. */
    let estimatedRows = $state(1)
    /**
     * Physical LINES, when a backend knows them. The status bar's count and nothing
     * else. ❌ Never a scroll, selection, or gutter coordinate: a long line is several
     * rows, so these two numbers part company on exactly the files this work exists for.
     */
    let totalLines = $state<number | null>(null)
    let totalBytes = $state(0)
    let error = $state('')
    let errorCanRetry = $state(false)
    let filePath = $state('')
    // The volume the file lives on (`'root'` for the local drive), from the `volume`
    // URL param. Threaded to `viewerOpen` so a file inside a `.zip` on a remote
    // parent (direct SMB / MTP) is previewed by pulling the entry through that volume.
    let volumeId = $state('root')
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

    /**
     * Media (Image / PDF) concern: the `kind` / `mediaToken` / `mediaDimensions`
     * session state, the `isMedia` / `mediaSrc` deriveds, and the "View as text"
     * override. `text` flows through the line / virtual-scroll pipeline; `image` /
     * `pdf` render inline via the `cmdr-media://` scheme and leave the text fields
     * empty. Every text-only data path and control below guards on `media.isMedia`.
     */
    const media = createViewerMedia({
        reopenAsText: () => reopenAsText(),
        reopenNatural: () => reopenNatural(),
    })
    let viewMode = $state<ViewerDisplayMode>('text')
    let rawByteOffset = $state(0)
    const isTextView = $derived(viewMode === 'text')

    /**
     * Tail mode: when on, the open viewport auto-follows newly appended bytes.
     * When off, the watcher's `viewer:file-changed` event surfaces a persistent
     * reload toast. The setting persists per file path (SHA-256 truncated) so
     * a log the user tailed yesterday opens tailed today.
     */
    let tailMode = $state(false)

    // Derive current mode: if we started with byteSeek but now have a line count, we upgraded to lineIndex
    const currentMode = $derived(backendType === 'byteSeek' && totalLines !== null ? 'lineIndex' : backendType)

    const indexingPoll = createIndexingPoll({
        getSessionId: () => sessionId,
        // The poll is how the viewer learns the index landed without waiting for a
        // scroll, so it carries the row count too: ByteSeek's sampled estimate is
        // replaced by LineIndex's real one the moment it exists.
        onStatus: ({ backendType: bt, isIndexing: ind, totalRows: tr, totalLines: tl }) => {
            backendType = bt
            isIndexing = ind
            if (tr.kind === 'exact') totalRows = tr.rows
            estimatedRows = tr.rows
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
            scroll.clearCache()
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
            const res = await viewerSetTailMode(sessionId, next)
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

    let unsubscribeLanguage: (() => void) | undefined
    // The "fetching this file" bar for an open that pulls first (phone, server, zip).
    const pull = createViewerPull()
    let unlistenPull: UnlistenFn | undefined

    // `.scroll-spacer`, the box the text cursor is positioned against. Not on the scroll
    // composable: nothing else needs it, and the cursor is the only thing measured
    // against the spacer rather than the scroll container.
    let spacerRef = $state<HTMLDivElement>()

    // Window lifecycle state. `canClose` prevents closing before WebKit has settled the
    // mount; it flips right after mount, NOT when the open resolves, because a pull off
    // a phone can run for minutes and Escape / Cancel must end it. `windowReady` marks
    // the open resolved (the E2E readiness attribute).
    let windowReady = $state(false)
    let canClose = false
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
    // The extension classifier flags images and PDFs, but the warning concerns
    // decoded text only. Rendered media and byte views never show it.
    const showWarningBanner = $derived(
        isTextView && warning.shouldWarn && !bannerDismissed && !warningSuppressed && !loading,
    )

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
            const res = await viewerSetEncoding(sessionId, encoding)
            if (res.status === 'error') {
                log.error('set_encoding failed: {message}', { message: res.error })
                currentEncoding = prev
                return
            }
            scroll.clearCache()
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
    let unlistenEditAction: UnlistenFn | undefined
    let unlistenWindowFocus: UnlistenFn | undefined

    const textWidthTracker = createTextWidthTracker({
        getContentRef: () => scroll.contentRef,
        getVisibleRowsKey: () => scroll.visibleRows,
    })

    const scroll = createViewerScroll({
        getSessionId: () => sessionId,
        getTotalRows: () => totalRows,
        setTotalRows: (v: number) => {
            totalRows = v
        },
        getEstimatedRows: () => estimatedRows,
        getBackendType: () => backendType,
        onTimeoutError: () => {
            error = tString('viewer.error.timeout')
            errorCanRetry = true
        },
        getAllRowTexts: () => {
            if (backendType !== 'fullLoad') return null
            const total = totalRows
            if (total === null || total === 0) return null
            if (!scroll.rowCache.has(0) || !scroll.rowCache.has(total - 1)) return null
            const texts: string[] = new Array<string>(total)
            for (let i = 0; i < total; i++) {
                texts[i] = scroll.rowCache.get(i)?.text ?? ''
            }
            return texts
        },
        getTextWidth: () => textWidthTracker.textWidth,
    })

    const search = createViewerSearch({
        getSessionId: () => sessionId,
        getTotalBytes: () => totalBytes,
        getTotalRows: () => totalRows,
        getEstimatedTotalRows: () => scroll.estimatedTotalRows(),
        getScrollLineHeight: () => scroll.scrollLineHeight,
        getRowTop: (n: number) => scroll.getRowTop(n),
        getViewportHeight: () => scroll.viewportHeight,
        getContentRef: () => scroll.contentRef,
        isWordWrap: () => scroll.wordWrap,
    })

    const selection = createViewerSelection()

    /**
     * Screen-reader-friendly announcement of the current selection. Empty string when
     * there's nothing selected (the live region stays silent). It names the PHYSICAL
     * line range the gutter draws, plus a UTF-16 character count for orientation, so a
     * listener and a sighted user are told the same thing about the same selection.
     */
    const selectionAnnouncement = $derived(
        describeSelectionForAt(selection.selection, (row) => {
            const cached = scroll.rowCache.get(row)
            return cached === undefined ? null : { utf16Length: cached.text.length, lineNumber: cached.lineNumber }
        }),
    )

    /**
     * The UTF-8 byte length of the current selection. Returns `null` when it can't be
     * known (a row the walk needs isn't cached), which routes the copy flow to its
     * "unknown size" branch and a confirm before reading.
     */
    function estimateCurrentSelectionBytes(): number | null {
        const sel = selection.selection
        if (sel === null) return 0
        // From-the-top shortcut: ⌘A on a multi-MB file selects rows the user never
        // scrolled through, so the row cache can't service the per-row walk. The file's
        // own size answers it exactly, minus whatever the selection leaves behind in the
        // last row. ❗ The subtraction is the point: the same number picks the confirm
        // tier and the refusal, so it has to be what will actually be copied (I3).
        const fromStart = selectionBytesFromFileSize(sel, {
            totalRows,
            totalBytes,
            lastRowText: totalRows === null ? null : (scroll.rowCache.get(totalRows - 1)?.text ?? null),
        })
        if (fromStart !== null) return fromStart
        return estimateSelectionBytes(sel, (n) => {
            const row = scroll.rowCache.get(n)
            if (row === undefined) return null
            return rowMetrics({
                text: row.text,
                continues: row.continues,
                isLastRow: totalRows !== null && n >= totalRows - 1,
            })
        })
    }

    const copy = createViewerCopy({
        getSessionId: () => sessionId,
        getSelectionBytes: estimateCurrentSelectionBytes,
        getRangeEnds: () => toRangeEnds(selection.selection),
    })

    const copyFlow = createViewerCopyOrchestrator({
        copy,
        getFileName: () => fileName,
    })

    // The optional text cursor. `getLayoutKey` names everything that can move a rendered
    // row while the focus stays where it is; the measurement itself lives in the composable.
    const textCursor = createViewerTextCursor({
        isEnabled: () => getViewerShowTextCursor(),
        getFocus: () => selection.selection?.focus ?? null,
        getContentRef: () => scroll.contentRef,
        getSpacerRef: () => spacerRef,
        getLayoutKey: () => [scroll.scrollTop, scroll.rowsOffset, scroll.visibleRows, scroll.wordWrap],
    })

    // Each pointer setter also ends the keyboard's vertical run: a click or drag picks a
    // new column, so the next Shift+Up/Down aims from there rather than from wherever an
    // earlier run was heading. `keyboard` is defined below and read lazily here.
    const pointerDrag = createViewerPointerDrag({
        getContentRef: () => scroll.contentRef,
        getRowText: (row) => scroll.rowCache.get(row)?.text,
        hasSelection: () => selection.selection !== null,
        setAnchor: (point) => {
            keyboard.resetDesiredColumn()
            selection.setAnchor(point)
        },
        setFocus: (point) => {
            keyboard.resetDesiredColumn()
            selection.setFocus(point)
        },
        setRange: (range) => {
            keyboard.resetDesiredColumn()
            selection.setRange(range)
        },
        takeFocus: () => scroll.containerRef?.focus({ preventScroll: true }),
    })

    // Every effect below drives the text / virtual-scroll pipeline. In media mode the
    // text fields are empty and `.file-content` isn't rendered, so they all no-op:
    // gate on `isMedia` up front so nothing touches the (empty) line machinery.

    // Fetch lines when visible range changes (debounced)
    $effect(() => {
        if (!isTextView) return
        scroll.runFetchEffect()
    })

    // Track horizontal content width so .scroll-spacer can create a scrollbar
    $effect(() => {
        if (!isTextView) return
        return scroll.runContentWidthEffect()
    })

    // Measure average wrapped line height for virtual scroll approximation
    $effect(() => {
        if (!isTextView) return
        return scroll.runWrappedLineHeightEffect()
    })

    // Compensate scroll position when scrollLineHeight changes
    $effect(() => {
        if (!isTextView) return
        scroll.runScrollCompensationEffect()
    })

    // Height map: trigger preparation when word wrap + fullLoad lines + textWidth are available
    $effect(() => {
        if (!isTextView) return
        scroll.runHeightMapInitEffect()
    })

    // Height map: reflow when textWidth changes
    $effect(() => {
        if (!isTextView) return
        scroll.runHeightMapReflowEffect()
    })

    // Track available text width for height map calculations via ResizeObserver + visible lines change
    $effect(() => {
        if (!isTextView) return
        return textWidthTracker.runResizeEffect()
    })

    // Re-measure text width when lines first appear (ResizeObserver won't fire if container size didn't change)
    $effect(() => {
        if (!isTextView) return
        textWidthTracker.runVisibleRowsEffect()
    })

    // Debounce search input
    $effect(() => {
        if (!isTextView) return
        search.runDebounceEffect()
    })

    // Re-place the optional text cursor after anything that moves the focus or its row
    $effect(() => {
        if (!isTextView) return
        textCursor.runMeasureEffect()
    })

    function closeWindow() {
        if (closing) return
        if (!canClose) {
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

        // ❌ Never `getCurrentWindow().close()` here: destroying this webview from inside the
        // handler that asked for it stalls queued IPC on webkit2gtk and can segfault the whole app
        // on macOS WebKit. The backend hides it and destroys it a moment later.
        // See `$lib/child-window-close`.
        void closeSelfWindow().then(() => {
            log.debug('closeWindow: close requested after {elapsed}ms', {
                elapsed: Math.round(performance.now() - start),
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

    const keyboard = createViewerKeyboard({
        getTotalRows: () => totalRows,
        getTotalBytes: () => totalBytes,
        // What the template DRAWS, not what the cache holds: a rendered row the cache
        // missed shows as empty, and the motion model has to agree with the screen or a
        // chord aiming at that row is dead forever. See `scroll.renderedRowText`.
        getRowText: (row) => scroll.renderedRowText(row),
        getLastRenderedRow: () => scroll.visibleRows.at(-1)?.rowNumber ?? null,
        selection,
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

    /**
     * What the viewer's Edit menu acts on. The same two functions ⌘A / ⌘C reach, so the
     * menu path and the keyboard path can't drift apart.
     */
    const viewerEditActionDeps = {
        search: {
            get searchVisible() {
                return search.searchVisible
            },
            get searchInputRef() {
                return search.searchInputRef
            },
        },
        selectAllContent: keyboard.handleSelectAllShortcut,
        copyContent: () => {
            void copyFlow.handleCopy()
        },
        writeClipboardText: (text: string) => {
            navigator.clipboard.writeText(text).catch((e: unknown) => {
                log.warn('Copying the search query to the clipboard failed: {error}', { error: String(e) })
            })
        },
    }

    /**
     * Tells the viewer's menu bar whether the search box holds focus, which is what decides
     * whether its Edit > Cut / Paste look live: that box is the only editable field in this
     * window. Chrome only — ⌘X / ⌘V reach the box whatever the items look like.
     */
    function pushSearchInputFocused(focused: boolean) {
        viewerSetSearchInputFocused(getCurrentWindow().label, focused).catch(() => {})
    }

    /**
     * The search bar closing has to push too: removing a focused input from the DOM fires no
     * `blur`, so without this the two items would stay live over a viewer with no search box at
     * all. Also covers the initial render, where nothing is focused yet.
     */
    $effect(() => {
        if (!search.searchVisible) pushSearchInputFocused(false)
    })

    async function switchViewMode(next: ViewerDisplayMode): Promise<void> {
        if (loading || !sessionId || next === viewMode) return
        if (next === 'media' && availableMediaKind(media.kind, media.lastMediaKind) === null) return
        if (next !== 'text') search.closeSearch()
        pointerDrag.closeContextMenu()
        if (next === 'text' && media.kind !== 'text') {
            await media.viewAsText()
        } else if (next === 'media' && media.kind === 'text') {
            await media.viewAsMedia()
        } else {
            viewMode = next
            if (next === 'text') {
                await tick()
                scroll.contentRef?.focus()
            }
        }
    }

    /**
     * Window-level keydown router. In text mode it delegates to the full viewer
     * keyboard (search, selection, copy, navigation). In media mode the text
     * shortcuts don't apply (there are no lines to search / select / copy), so only
     * Escape closes the window; the image's own fit / zoom / pan keys are handled by
     * the focused `MediaImageView` stage, and the PDF embed owns its own keys.
     */
    function handleWindowKeyDown(e: KeyboardEvent) {
        const mode = modeForKey(e, availableMediaKind(media.kind, media.lastMediaKind) !== null)
        const target = e.target
        const editing = target instanceof HTMLElement && target.closest('input, textarea, [contenteditable="true"], [role="combobox"]')
        if (mode && !editing && !loading && sessionId) {
            e.preventDefault()
            void switchViewMode(mode)
            return
        }
        if (!isTextView) {
            if (e.key === 'Escape') {
                e.preventDefault()
                closeWindow()
            }
            return
        }
        keyboard.handleKeyDown(e)
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

    /**
     * Opens (or re-opens) the viewer session for `path`. `asText: true` forces a full
     * text session even for a media file (the "View as text" override); the default
     * lets the backend classify and return a media or text session.
     */
    async function openViewerSession(path: string, { asText = false }: { asText?: boolean } = {}) {
        const t0 = performance.now()
        // Pass the window label so the backend can free this session when the
        // window is closed via the titlebar X (which never fires `viewerClose`).
        const open = asText ? viewerOpenAsText : viewerOpen
        pull.start()
        let result: Awaited<ReturnType<typeof open>>
        try {
            result = await open(path, volumeId, getCurrentWindow().label)
        } finally {
            pull.finish()
        }
        log.debug('viewer_open IPC took {ms}ms', { ms: Math.round(performance.now() - t0) })

        sessionId = result.sessionId
        fileName = result.fileName
        totalBytes = result.totalBytes
        // `initialLines.totalRows` is the row total and says whether it's counted or
        // sampled; `result.totalLines` is the PHYSICAL line count, for the status bar.
        // (`result.estimatedTotalLines` carries the same row number as `totalRows.rows`;
        // the wire keeps its old spelling until the IPC rename lands.)
        totalRows = result.initialLines.totalRows.kind === 'exact' ? result.initialLines.totalRows.rows : null
        estimatedRows = result.initialLines.totalRows.rows
        totalLines = result.totalLines
        backendType = result.backendType
        isIndexing = result.isIndexing
        currentEncoding = result.encoding
        detectedEncoding = result.encoding
        media.setFromOpenResult(result)

        const openedAsMedia = isMediaKind(result.kind)
        viewMode = openedAsMedia ? 'media' : 'text'

        // Encoding options are text-only; a media session has no decoded bytes to pick
        // an encoding for. Fetch the dropdown options once for text; they don't change.
        if (!openedAsMedia) {
            void viewerGetEncodingOptions(result.sessionId)
                .then((res) => {
                    if (res.status === 'ok') {
                        encodingChoices = res.data.all
                        detectedEncoding = res.data.detected
                        currentEncoding = res.data.current
                    }
                })
                .catch(() => {})
        }

        log.debug(
            'Opened file: {fileName}, {totalBytes} {bytesNoun}, totalRows={totalRows}, totalLines={totalLines}, backend={backendType}, isIndexing={isIndexing}',
            {
                fileName: result.fileName,
                totalBytes: result.totalBytes,
                bytesNoun: pluralize(result.totalBytes, 'byte'),
                totalRows: result.initialLines.totalRows.rows,
                totalLines: result.totalLines,
                backendType: result.backendType,
                isIndexing: result.isIndexing,
            },
        )

        // The line / index / tail pipeline is text-only. A media session has no lines
        // to index, cache, or tail-follow, so skip all of it; the image / PDF renders
        // from `mediaToken` instead.
        if (!openedAsMedia) {
            if (result.isIndexing) {
                indexingPoll.start()
            }

            // Subscribe to the watcher event stream. Tail mode itself starts off on
            // every open; the user re-enables it per session.
            await viewerTail.init()

            scroll.clearCache()
            scroll.cacheRows(result.initialLines.firstRowNumber, result.initialLines.rows)

            log.debug('Initial cache: {count} {rowsNoun} loaded', {
                count: result.initialLines.rows.length,
                rowsNoun: pluralize(result.initialLines.rows.length, 'row'),
            })

            // For FullLoad files, fetch ALL rows so the height map can prepare them.
            // The initial chunk only contains ~200 rows, but FullLoad files are <1MB so
            // fetching the rest in one IPC call is trivial.
            const fullLoadRows = result.initialLines.totalRows
            if (
                result.backendType === 'fullLoad' &&
                fullLoadRows.kind === 'exact' &&
                result.initialLines.rows.length < fullLoadRows.rows
            ) {
                const remaining = fullLoadRows.rows - result.initialLines.rows.length
                const startRow = result.initialLines.firstRowNumber + result.initialLines.rows.length
                const tFetch = performance.now()
                viewerGetLines(result.sessionId, 'line', startRow, remaining)
                    .then((chunk) => {
                        log.debug('FullLoad fetch remaining {count} {rowsNoun} took {ms}ms', {
                            count: chunk.rows.length,
                            rowsNoun: pluralize(chunk.rows.length, 'row'),
                            ms: Math.round(performance.now() - tFetch),
                        })
                        scroll.cacheRows(chunk.firstRowNumber, chunk.rows)
                    })
                    .catch(() => {}) // Non-critical: height map just won't activate
            }
        }

        await initAppMode()
        getCurrentWindow()
            .setTitle(decorateChildWindowTitle(tString('viewer.window.titleSuffix', { fileName: result.fileName })))
            .catch(() => {})

        await setupMcpListeners(path)

        const windowLabel = getCurrentWindow().label
        viewerSetupMenu(windowLabel)
            .then(() => {
                if (scroll.wordWrap) viewerSetWordWrap(windowLabel, true).catch(() => {})
            })
            .catch(() => {})

        unlistenWordWrap = await onViewerWordWrapToggled(() => {
            toggleWordWrap(true)
        })

        // Edit > Copy / Select all from the viewer's own menu bar. Custom items rather than
        // native ones, because the native selectors would act on the status bar; see
        // `viewer-menu-actions.ts`. Media sessions have no text to select or copy.
        unlistenEditAction = await onViewerEditAction(({ action }) => {
            if ((viewMode === 'binary' || viewMode === 'hex') && action === 'copy') {
                const selected = window.getSelection()?.toString()
                if (selected) viewerEditActionDeps.writeClipboardText(selected)
                return
            }
            if (!isTextView) return
            runViewerEditAction(action, viewerEditActionDeps)
        })

        // On macOS the app-level menu bar is shared across windows, so each window swaps in its
        // own menu when it gains focus. A freshly-opened viewer window is already focused, so
        // `onFocusChanged` won't fire for this initial focus — activate the viewer menu explicitly
        // here, then sync the shared word-wrap checkbox to this viewer's own state.
        void activateWindowMenu('viewer')
        viewerSetWordWrap(windowLabel, scroll.wordWrap).catch(() => {})
        pushSearchInputFocused(isSearchInputFocused(viewerEditActionDeps.search))

        // The listener covers subsequent focus regains (clicking back to this viewer), re-syncing
        // the shared checkbox since multiple viewers can have different word-wrap states — and the
        // same for the search box's claim on Edit > Cut / Paste, which is what settles a switch
        // between two viewers whose blur and focus pushes cross in flight.
        unlistenWindowFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }: { payload: boolean }) => {
            if (focused) {
                void activateWindowMenu('viewer')
                viewerSetWordWrap(windowLabel, scroll.wordWrap).catch(() => {})
                pushSearchInputFocused(isSearchInputFocused(viewerEditActionDeps.search))
            }
        })

        error = ''
        errorCanRetry = false
    }

    async function retryOpen() {
        if (!filePath) return
        loading = true
        error = ''
        errorCanRetry = false
        try {
            await openViewerSession(filePath)
        } catch (e) {
            const failure = handleOpenFailure(log, 'Retry', e)
            error = failure.message
            errorCanRetry = failure.canRetry
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
        unlistenEditAction?.()
        unlistenWindowFocus?.()
    }

    /**
     * Page-side half of the two-way view switch (the media composable owns the
     * triggers and the media-state reset). Re-opens the same file and swaps to the
     * fresh session, closing the old one: we tear down the per-session listeners
     * first (they get re-attached by `openViewerSession`) and close the old session
     * explicitly (the new session has a different id, so window teardown alone
     * wouldn't free the old one).
     *
     * `asText: true` forces a text session ("View as text"); the default lets the
     * backend re-classify the file back to its natural media kind ("View as image /
     * PDF"). Both paths feed the result back through `media.setFromOpenResult` inside
     * `openViewerSession`, so the new kind / token / dimensions flow through.
     */
    async function reopenSession({ asText }: { asText: boolean }, logLabel: string) {
        if (!filePath) return
        const oldSessionId = sessionId
        loading = true
        error = ''
        errorCanRetry = false
        cleanupListeners()
        viewerTail.destroy()
        indexingPoll.stop()
        try {
            await openViewerSession(filePath, { asText })
            if (oldSessionId && oldSessionId !== sessionId) {
                viewerClose(oldSessionId).catch(() => {})
            }
        } catch (e) {
            const failure = handleOpenFailure(log, logLabel, e)
            error = failure.message
            errorCanRetry = failure.canRetry
        } finally {
            loading = false
            await tick()
            scroll.containerRef?.focus()
        }
    }

    /** "View as text" override: re-open the media file as a full text session. */
    function reopenAsText() {
        return reopenSession({ asText: true }, 'View as text')
    }

    /** "View as image / PDF" reverse switch: re-open the file so it re-renders as media. */
    function reopenNatural() {
        return reopenSession({ asText: false }, 'View as media')
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }
        // `setTimeout(0)`, never rAF (see the readiness note below).
        setTimeout(() => {
            canClose = true
            if (closeRequested) closeWindow()
        }, 0)

        await initAccentColor()

        await initReduceTransparency()

        // Seeds the store AND the reactive layer that `<Size>` and friends read.
        // `window-settings.ts` knows the viewer has no store capability (see
        // `src-tauri/capabilities/CLAUDE.md` § viewer), so it takes the restricted
        // path: the backend snapshot plus cross-window change events. Non-throwing;
        // falls back to registry defaults.
        await initWindowSettings()
        // This webview has its own i18n runtime, so it needs its own language and
        // formatting sync: without it the viewer sits on the webview's tag and
        // formats sizes and dates differently from the main pane.
        unsubscribeLanguage = initWindowLanguageSync()
        scroll.wordWrap = getSetting('viewer.wordWrap')
        warningSuppressed = getSetting('fileViewer.suppressBinaryWarning')

        // Apply compounded text size after settings are loaded so the user's
        // persisted slider value is honored on first paint.
        await initTextSize()

        const params = new URLSearchParams(window.location.search)
        const pathParam = params.get('path')

        if (!pathParam) {
            error = tString('viewer.error.noPath')
            errorCanRetry = false
            loading = false
            return
        }

        filePath = pathParam
        // Missing / empty `volume` param means the local drive (older links, MCP).
        volumeId = params.get('volume') || 'root'
        unlistenPull = await onViewerPullProgress(getCurrentWindow(), (progress) => {
            pull.report(progress)
        })

        try {
            await openViewerSession(pathParam)
        } catch (e) {
            const failure = handleOpenFailure(log, 'Open', e)
            error = failure.message
            errorCanRetry = failure.canRetry
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
                log.debug('Window ready')
            }, 0)
        }
    })

    onDestroy(() => {
        unsubscribeLanguage?.()
        cleanupAccentColor()
        cleanupReduceTransparency()
        cleanupTextSize()
        cleanupListeners()
        search.destroy()
        scroll.destroy()
        indexingPoll.stop()
        viewerTail.destroy()
        unlistenPull?.()
        pull.destroy()
    })
</script>

<svelte:window on:keydown={handleWindowKeyDown} on:blur={pointerDrag.handleWindowBlur} />

<main
    class="viewer-container"
    bind:this={scroll.containerRef}
    tabindex={-1}
    data-window-ready={windowReady ? (error ? 'error' : 'loaded') : 'loading'}
    oncopy={(e: ClipboardEvent) => {
        // Media mode has no custom text selection; let the browser's native copy
        // (e.g. copying the image) run unintercepted.
        if (!isTextView) return
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
    <h1 class="sr-only">{tString('viewer.srHeading')}</h1>
    <ViewerToolbar
        {fileName}
        {filePath}
        kind={media.kind}
        mode={viewMode}
        lastMediaKind={media.lastMediaKind}
        {currentEncoding}
        {detectedEncoding}
        {encodingChoices}
        {isIndexing}
        {tailMode}
        onModeChange={(mode: ViewerDisplayMode) => { void switchViewMode(mode) }}
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
                {#snippet kindSnippet(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}
                {#snippet quickLookKey(children: import('svelte').Snippet)}<ShortcutChip
                        key="⇧Space"
                    />{@render children()}{/snippet}
                {#snippet openKey(children: import('svelte').Snippet)}<ShortcutChip key="Enter" />{@render children()}{/snippet}
                <Trans
                    key="viewer.binaryWarning.body"
                    params={{ category: warning.category ?? 'binary', ext: warning.ext }}
                    snippets={{ kindName: kindSnippet, quickLookKey, openKey }}
                />
            </p>
            <div class="binary-warning-actions">
                <button type="button" class="binary-warning-action" onclick={dismissBanner}
                    >{tString('viewer.binaryWarning.dismiss')}</button
                >
                <button type="button" class="binary-warning-action" onclick={suppressBannerForever}
                    >{tString('viewer.binaryWarning.suppressForever')}</button
                >
            </div>
        </aside>
    {/if}
    {#if search.searchVisible}
        <div class="search-bar" role="search">
            <TextInput
                bind:inputElement={search.searchInputRef}
                bind:value={search.searchQuery}
                type="search"
                radius="sm"
                containerStyle="flex: 1; max-width: 300px"
                placeholder={tString('viewer.search.placeholder')}
                ariaLabel={tString('viewer.search.ariaLabel')}
                autocomplete="off"
                autocapitalize="off"
                spellcheck={false}
                onfocus={() => { pushSearchInputFocused(true); }}
                onblur={() => { pushSearchInputFocused(false); }}
            />
            <button
                type="button"
                class="search-toggle"
                class:active={search.caseSensitive}
                aria-pressed={search.caseSensitive}
                aria-label={tString('viewer.search.caseSensitive')}
                onclick={() => { search.toggleCaseSensitive(); }}
                use:tooltip={{ text: tString('viewer.search.caseSensitive'), shortcut: '⌘⌥C' }}
            >
                <!-- eslint-disable-next-line cmdr/no-raw-user-facing-string -- typographic glyph for the case-sensitive toggle, not copy (the a11y label + tooltip carry the copy) -->
                <span>Aa</span>
            </button>
            <button
                type="button"
                class="search-toggle"
                class:active={search.useRegex}
                aria-pressed={search.useRegex}
                aria-label={tString('viewer.search.regex')}
                onclick={() => { search.toggleUseRegex(); }}
                use:tooltip={{ text: tString('viewer.search.regex'), shortcut: '⌘⌥R' }}
            >
                .*
            </button>
            <span class="match-count" aria-live="polite">
                {#if search.searchStatus === 'invalidQuery'}
                    <span class="search-error" role="alert">{search.searchError}</span>
                {:else if search.searchStatus === 'running'}
                    <span class="search-spinner"><Spinner size="sm" /></span>
                    {#if search.searchMatches.length > 0}
                        {tString('viewer.search.matchPosition', {
                            current: search.currentMatchIndex + 1,
                            total: search.searchMatches.length,
                            more: search.searchLimitReached ? 'yes' : 'no',
                        })}
                        &middot; {Math.round(search.searchProgress * 100)}%
                    {:else}
                        {tString('viewer.search.searching')} {Math.round(search.searchProgress * 100)}%
                    {/if}
                {:else if search.searchMatches.length > 0}
                    {tString('viewer.search.matchPosition', {
                        current: search.currentMatchIndex + 1,
                        total: search.searchMatches.length,
                        more: search.searchLimitReached ? 'yes' : 'no',
                    })}
                    {#if search.searchStatus === 'cancelled'}
                        {tString('viewer.search.partial')}
                    {/if}
                {:else if search.searchQuery && (search.searchStatus === 'done' || search.searchStatus === 'cancelled')}
                    {tString('viewer.search.noMatches')}{search.searchStatus === 'cancelled'
                        ? ' ' + tString('viewer.search.partial')
                        : ''}
                {/if}
            </span>
            {#if search.searchStatus === 'running'}
                <button
                    onclick={() => {
                        search.stopSearch()
                    }}
                    aria-label={tString('viewer.search.stop')}
                    use:tooltip={tString('viewer.search.stopTooltip')}>&#x25A0;</button
                >
            {/if}
            <button
                onclick={() => {
                    search.findPrev()
                }}
                disabled={search.searchMatches.length === 0}
                aria-label={tString('viewer.search.previous')}
                use:tooltip={{ text: tString('viewer.search.previous'), shortcut: '⇧Enter' }}>&#x25B2;</button
            >
            <button
                onclick={() => {
                    search.findNext()
                }}
                disabled={search.searchMatches.length === 0}
                aria-label={tString('viewer.search.next')}
                use:tooltip={{ text: tString('viewer.search.next'), shortcut: 'Enter' }}>&#x25BC;</button
            >
            <button
                onclick={() => {
                    search.closeSearch()
                }}
                aria-label={tString('viewer.search.close')}
                use:tooltip={{ text: tString('viewer.search.closeTooltip'), shortcut: 'Esc' }}>&#x2715;</button
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

    {#if loading && pull.visible}
        <PullProgressPanel
            fileName={filePath.split('/').pop() ?? filePath}
            bytesDone={pull.bytesDone}
            bytesTotal={pull.bytesTotal}
            fraction={pull.fraction}
            stalled={pull.stalled}
            onCancel={closeWindow}
        />
    {:else if loading}
        <div class="status-message">{tString('viewer.loading')}</div>
    {:else if error && errorCanRetry}
        <div class="status-message timeout-error" role="alert">
            <p class="timeout-error-message">{error}</p>
            <div class="timeout-error-actions">
                <button class="viewer-action-btn" onclick={() => void retryOpen()}>{tString('viewer.error.retry')}</button>
                <button class="viewer-action-btn viewer-action-secondary" onclick={closeWindow}
                    >{tString('viewer.error.cancel')}</button
                >
            </div>
        </div>
    {:else if error}
        <div class="status-message error">{error}</div>
    {:else if viewMode === 'binary' || viewMode === 'hex'}
        {#key viewMode}
            <RawByteView
                {sessionId}
                {totalBytes}
                fileName={fileName}
                mode={viewMode}
                initialOffset={rawByteOffset}
                onOffsetChange={(offset: number) => { rawByteOffset = offset }}
            />
        {/key}
    {:else if media.kind === 'image'}
        <MediaImageView src={media.mediaSrc} {fileName} />
    {:else if media.kind === 'pdf'}
        <MediaPdfView src={media.mediaSrc} {fileName} />
    {:else}
        <div
            class="file-content"
            class:word-wrap={scroll.wordWrap}
            role="document"
            tabindex="0"
            aria-label={tString('viewer.content.ariaLabel', { fileName })}
            bind:this={scroll.contentRef}
            onscroll={scroll.handleScroll}
            onpointerdown={pointerDrag.handlePointerDown}
            onpointermove={pointerDrag.handlePointerMove}
            onpointerup={pointerDrag.handlePointerUp}
            onpointercancel={pointerDrag.handlePointerCancel}
            oncontextmenu={pointerDrag.handleContextMenu}
        >
            <div
                class="scroll-spacer"
                bind:this={spacerRef}
                style="height: {scroll.spacerHeight}px; min-width: {scroll.wordWrap
                    ? 0
                    : scroll.contentWidth}px"
            >
                <div
                    class="lines-container"
                    bind:this={scroll.linesContainerRef}
                    style="transform: translateY({scroll.rowsOffset}px)"
                >
                    <!-- One `ViewerRow` per ROW: the gutter prints a number only where
                         a physical line starts, and a row Cmdr broke out of a long line
                         carries the continuation marker instead. -->
                    {#each scroll.visibleRows as { rowNumber, text, continues, lineNumber } (rowNumber)}
                        <ViewerRow
                            {rowNumber}
                            {lineNumber}
                            {continues}
                            gutterWidth={scroll.gutterWidth}
                            wordWrap={scroll.wordWrap}
                            segments={search.getHighlightedSegments(
                                rowNumber,
                                text,
                                getRowSegmentBounds(selection.selection, rowNumber, text.length),
                            )}
                        />
                    {/each}
                </div>
                <!-- A SIBLING of `.lines-container`, never a child: that container's
                     height is divided by its child count to derive the average wrapped
                     line height. -->
                <ViewerTextCursor box={textCursor.box} blinkKey={textCursor.blinkKey} />
            </div>
        </div>
    {/if}

    <ViewerStatusBar
        {fileName}
        kind={media.kind}
        mode={viewMode}
        mediaDimensions={media.mediaDimensions}
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
        line-height: var(--font-line-height-normal);
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
        line-height: var(--font-line-height-flat);
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
        /* Not a leading token: this number is a MEASURED contract. The wrapped-line
           measurer in `viewer-line-heights.svelte.ts` renders its probe with the same
           literal, and `.line`'s 18px height is 12px × this. Change one, change all three. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- measured contract, see above */
        line-height: 1.5;
        /* The viewer owns its own selection model (see selection.svelte.ts). We
         * suppress the browser's native selection because it can't render a
         * selection that survives DOM recycling under virtual scroll. The custom
         * `.selected` class below paints the visible portion. */
        user-select: none;
        -webkit-user-select: none;
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

    /* Everything inside a row — `.line`, its gutter, its text, the selection and
       search spans, and the continuation marker — lives in `ViewerRow.svelte`, which
       takes `wordWrap` as a prop and applies the wrapped variants itself. */

    .word-wrap {
        overflow-x: hidden;
    }

    .word-wrap .lines-container {
        width: auto;
        right: 0;
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
        line-height: var(--font-line-height-normal);
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
        line-height: var(--font-line-height-flat);
        border-radius: var(--radius-sm);
        background: var(--color-warning-bg-solid);
        color: var(--color-warning-text);
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
