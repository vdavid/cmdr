<script lang="ts">
    /**
     * SearchDialog - Whole-drive file search overlay.
     *
     * Follows the command palette pattern (custom overlay, not ModalDialog).
     * Searches the in-memory index by filename (wildcards), size, and date.
     *
     * Layout (post-M3):
     *   1. SearchBar: one input drives all modes (AI, filename, regex).
     *   2. SearchModeChips: mode discriminator (chips below the bar).
     *   3. SearchFilterChips: Size / Modified / Search in chips with popovers, plus Add filter.
     *   4. SearchResults: column headers + results + status bar.
     *
     * This is the orchestrator: overlay, mount/unmount, keyboard dispatch, search execution,
     * state wiring to child components via props/callbacks.
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import {
        notifyDialogOpened,
        notifyDialogClosed,
        prepareSearchIndex,
        searchFiles,
        releaseSearchIndex,
        translateSearchQuery,
        parseSearchScope,
        getSystemDirExcludes,
        onSearchIndexReady,
    } from '$lib/tauri-commands'
    import { iconCacheVersion } from '$lib/icon-cache'
    import type { UnlistenFn } from '$lib/tauri-commands'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { isScanning, getEntriesScanned } from '$lib/indexing'
    import {
        getQuery,
        setQuery,
        getMode,
        setMode,
        getSizeFilter,
        setSizeFilter,
        getSizeValue,
        setSizeValue,
        getSizeUnit,
        setSizeUnit,
        getSizeValueMax,
        setSizeValueMax,
        getSizeUnitMax,
        setSizeUnitMax,
        getDateFilter,
        setDateFilter,
        getDateValue,
        setDateValue,
        getDateValueMax,
        setDateValueMax,
        getResults,
        setResults,
        getTotalCount,
        setTotalCount,
        getCursorIndex,
        setCursorIndex,
        getIsIndexReady,
        setIsIndexReady,
        getIndexEntryCount,
        setIndexEntryCount,
        getIsSearching,
        setIsSearching,
        getIsIndexAvailable,
        setIsIndexAvailable,
        getCaseSensitive,
        setCaseSensitive,
        getScope,
        setScope,
        getExcludeSystemDirs,
        setExcludeSystemDirs,
        getLastAiPrompt,
        setLastAiPrompt,
        getLastAiCaveat,
        setLastAiCaveat,
        buildSearchQuery,
        clearSearchState,
        SEARCH_AUTO_APPLY_DEBOUNCE_MS,
        type SearchMode,
    } from './search-state.svelte'
    import SearchBar from './SearchBar.svelte'
    import SearchModeChips from './SearchModeChips.svelte'
    import SearchFilterChips from './SearchFilterChips.svelte'
    import SearchResults from './SearchResults.svelte'
    import AiTransparencyStrip from './AiTransparencyStrip.svelte'
    import RecentSearchesFooter from './RecentSearchesFooter.svelte'
    import RecentSearchesPopover from './RecentSearchesPopover.svelte'
    import {
        loadRecentSearches,
        getRecentSearchesList,
        setRecentSearchesList,
    } from './recent-searches-state.svelte'
    import { applyHistoryEntry } from './search-state.svelte'
    import {
        getRecentSearches as fetchRecentSearches,
        removeRecentSearch as removeRecentSearchIpc,
        type HistoryEntry,
    } from '$lib/tauri-commands'

    interface Props {
        /** Called when user selects a result: receives the full path */
        onNavigate: (path: string) => void
        /** Called when dialog is closed */
        onClose: () => void
        /** Current directory path of the focused pane (for ⌥F scope shortcut) */
        currentFolderPath: string
    }

    const { onNavigate, onClose, currentFolderPath }: Props = $props()

    let queryInputElement: HTMLInputElement | undefined = $state()
    let dialogElement: HTMLDivElement | undefined = $state()
    let searchResultsComponent: SearchResults | undefined = $state()
    let hoveredIndex = $state<number | null>(null)
    let debounceTimer: ReturnType<typeof setTimeout> | undefined
    let unlistenReady: UnlistenFn | undefined
    let unlistenAutoApply: (() => void) | undefined
    let systemDirExcludeTooltip = $state('Excludes common system and build folders')

    // Auto-apply toggle. Reactively mirrored from the `search.autoApply` setting so changes in the
    // settings window take effect without reopening the dialog (live-apply contract).
    let autoApplyEnabled = $state<boolean>(getSetting('search.autoApply'))

    // True while an IME composition is in progress. We don't schedule auto-apply during composition
    // (would fire mid-character on Chinese/Japanese/Korean input); on compositionend we reset the
    // debounce timer so the user gets exactly one fire after composition completes.
    let imeComposing = false

    /**
     * Query string at the time of the last actually-issued search (auto-applied or manual). Used by
     * the "Press Enter to search" hint to detect "the user has typed since the last run". `null`
     * means no search has been run yet this session/state.
     */
    let lastRunQuery = $state<string | null>(null)

    // Resizable column widths (px). Icon column is fixed at 24px.
    const colWidths = $state({ name: 250, path: 350, size: 80, modified: 120 })
    let dragCol: keyof typeof colWidths | null = null
    let dragStartX = 0
    let dragStartWidth = 0

    const gridTemplate = $derived(
        `24px ${String(colWidths.name)}px ${String(colWidths.path)}px ${String(colWidths.size)}px ${String(colWidths.modified)}px`,
    )

    function handleColumnDragStart(col: keyof typeof colWidths, e: MouseEvent): void {
        e.preventDefault()
        dragCol = col
        dragStartX = e.clientX
        dragStartWidth = colWidths[col]
        document.addEventListener('mousemove', handleColumnDragMove)
        document.addEventListener('mouseup', handleColumnDragEnd)
    }

    function handleColumnDragMove(e: MouseEvent): void {
        if (!dragCol) return
        const delta = e.clientX - dragStartX
        const minWidth = dragCol === 'size' || dragCol === 'modified' ? 60 : 80
        colWidths[dragCol] = Math.max(minWidth, dragStartWidth + delta)
    }

    function handleColumnDragEnd(): void {
        dragCol = null
        document.removeEventListener('mousemove', handleColumnDragMove)
        document.removeEventListener('mouseup', handleColumnDragEnd)
    }

    // Reactive derived state (read from search-state module)
    const query = $derived(getQuery())
    const mode = $derived(getMode())
    const sizeFilter = $derived(getSizeFilter())
    const sizeValue = $derived(getSizeValue())
    const sizeUnit = $derived(getSizeUnit())
    const sizeValueMax = $derived(getSizeValueMax())
    const sizeUnitMax = $derived(getSizeUnitMax())
    const dateFilter = $derived(getDateFilter())
    const dateValue = $derived(getDateValue())
    const dateValueMax = $derived(getDateValueMax())
    const results = $derived(getResults())
    const totalCount = $derived(getTotalCount())
    const cursorIndex = $derived(getCursorIndex())
    const isIndexReady = $derived(getIsIndexReady())
    const indexEntryCount = $derived(getIndexEntryCount())
    const isSearching = $derived(getIsSearching())
    const isIndexAvailable = $derived(getIsIndexAvailable())
    const caseSensitive = $derived(getCaseSensitive())
    const scope = $derived(getScope())
    const excludeSystemDirs = $derived(getExcludeSystemDirs())
    const lastAiPrompt = $derived(getLastAiPrompt())
    const lastAiCaveat = $derived(getLastAiCaveat())
    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())

    /** Whether AI search is enabled (provider configured and index available). */
    const aiEnabled = $derived(getSetting('ai.provider') !== 'off' && isIndexAvailable)
    /** Whether inputs/filters should be disabled (index not available or still scanning with no index). */
    const inputsDisabled = $derived(!isIndexAvailable)

    /**
     * True when the bar should show the "Press Enter to search" hint. Two cases:
     *   1. Auto-apply is off (any mode), and the query has changed since the last run.
     *   2. Mode is AI (which never auto-applies), and the query has changed since the last run.
     * Trimmed-empty queries hide the hint; there's nothing to run.
     */
    const showRunHint = $derived.by(() => {
        if (inputsDisabled) return false
        const trimmed = query.trim()
        if (!trimmed) return false
        const changed = trimmed !== (lastRunQuery ?? '').trim()
        if (!changed) return false
        return mode === 'ai' || !autoApplyEnabled
    })

    let highlightedFields = new SvelteSet<string>()
    /** True once the user has triggered at least one search (so we can distinguish "no query yet" from "0 results"). */
    let hasSearched = $state(false)

    // Recent searches: the footer anchor doubles as the popover anchor when the user opens the
    // popover via the trailing chip. ⌘H anchors to the search input as a fallback.
    let footerRef: HTMLDivElement | undefined = $state()
    let recentPopoverOpen = $state(false)
    const recentEntries = $derived(getRecentSearchesList())

    // Subscribe to icon cache version for reactivity
    const iconVersion = $derived($iconCacheVersion)

    /**
     * When AI gets disabled mid-session (provider switched off), make sure we're not stuck on
     * the AI mode. Filename is the fallback. Doesn't run on the AI-on side because we want the
     * user's explicit pick (filename or regex) to stick when AI comes back on.
     */
    $effect(() => {
        if (!aiEnabled && getMode() === 'ai') {
            setMode('filename')
        }
    })

    /** Focuses the unified query input. */
    function focusInput(): void {
        queryInputElement?.focus()
    }

    /**
     * Capture-phase Escape handler. Fires before native elements (select, date picker) consume the
     * event, AND before any descendant handler (like the filter-chip popover's). When a filter-chip
     * popover is open, Escape belongs to the popover, not the whole dialog: we defer here and let
     * the popover's own keydown handler close itself on the bubble. Without this guard, the
     * dialog's capture-phase listener would always run first and close the entire dialog.
     */
    function handleEscapeCapture(e: KeyboardEvent): void {
        if (e.key !== 'Escape') return
        if (dialogElement?.querySelector('.filter-chip-popover')) {
            // Let the popover handle Escape on the bubble; it'll close itself and stopPropagation.
            // This covers both the filter chips and the recent-searches popover (both reuse the
            // same `FilterChipPopover` primitive, so the DOM selector matches).
            return
        }
        e.preventDefault()
        e.stopPropagation()
        onClose()
    }

    /** Opens the recent-searches popover, anchored to the footer (or the input as fallback). */
    function openRecentPopover(): void {
        recentPopoverOpen = true
    }

    function closeRecentPopover(): void {
        recentPopoverOpen = false
    }

    /** Loads + runs a history entry. AI entries get the same explicit-trigger treatment as Enter. */
    function activateHistoryEntry(entry: HistoryEntry): void {
        applyHistoryEntry(entry)
        closeRecentPopover()
        void tick().then(() => {
            focusInput()
        })
        if (entry.mode === 'ai') {
            if (aiEnabled) {
                void executeAiSearch(entry.query)
            }
        } else {
            void executeSearch()
        }
    }

    /** Removes a recent search entry. Backend write is async; we update the cache eagerly. */
    function removeHistoryEntry(entry: HistoryEntry): void {
        // Optimistic UI: drop locally first so the chip animates out without waiting.
        setRecentSearchesList(getRecentSearchesList().filter((e) => e.id !== entry.id))
        void removeRecentSearchIpc(entry.id).then(async () => {
            // Re-fetch to stay consistent if the backend evicted other entries since last load.
            try {
                setRecentSearchesList(await fetchRecentSearches())
            } catch {
                // Already fell back to the optimistic snapshot; nothing to do.
            }
        })
    }

    /** Empty-state chip pick: load + run, mirroring the recent-search activation path. */
    function pickExample(chip: { mode: SearchMode; query: string }): void {
        setQuery(chip.query)
        setMode(chip.mode)
        if (chip.mode === 'ai') {
            if (aiEnabled) {
                void executeAiSearch(chip.query)
            }
        } else {
            void executeSearch()
        }
    }

    onMount(async () => {
        notifyDialogOpened('search').catch(() => {})
        window.addEventListener('keydown', handleEscapeCapture, true)

        // Live-mirror `search.autoApply`. The setting drives `scheduleSearch` and the run-hint
        // visibility; the dialog reads it reactively so toggling in the settings window takes
        // effect immediately, no reopen needed.
        unlistenAutoApply = onSpecificSettingChange('search.autoApply', (_id, value) => {
            autoApplyEnabled = value
        })

        // Listen for index ready event
        unlistenReady = await onSearchIndexReady((entryCount: number) => {
            setIsIndexReady(true)
            setIndexEntryCount(entryCount)
            // Auto-run pending search if user already typed something (filename/regex only;
            // AI mode always waits for explicit Enter / ⌘Enter).
            const pendingMode = getMode()
            if (
                pendingMode !== 'ai' &&
                (getQuery().trim() || getSizeFilter() !== 'any' || getDateFilter() !== 'any')
            ) {
                void executeSearch()
            }
        })

        // Start loading the index
        try {
            const result = await prepareSearchIndex()
            if (result.ready) {
                setIsIndexReady(true)
                setIndexEntryCount(result.entryCount)
            }
        } catch {
            // Index not available: indexing disabled, not started, or backend unavailable
            setIsIndexAvailable(false)
        }

        // Load persisted recent searches (newest first) into the in-memory store. Idempotent,
        // so closing + reopening the dialog doesn't refetch unless we explicitly invalidate.
        void loadRecentSearches()

        // Load system dir exclude list for tooltip display
        getSystemDirExcludes()
            .then((dirs) => {
                const shown = dirs.slice(0, 8)
                const rest = dirs.length - shown.length
                const list = shown.join(', ') + (rest > 0 ? `, +${String(rest)} more` : '')
                systemDirExcludeTooltip =
                    '<div style="max-width:360px">' +
                    '<div style="font-weight:600;margin-bottom:4px">Exclude system and build folders</div>' +
                    `<div style="color:var(--color-text-secondary)">${list}</div>` +
                    '</div>'
            })
            .catch(() => {})

        await tick()
        focusInput()
    })

    onDestroy(() => {
        notifyDialogClosed('search').catch(() => {})
        releaseSearchIndex().catch(() => {})
        unlistenReady?.()
        unlistenAutoApply?.()
        window.removeEventListener('keydown', handleEscapeCapture, true)
        if (debounceTimer) clearTimeout(debounceTimer)
        // Clean up any in-progress column drag
        document.removeEventListener('mousemove', handleColumnDragMove)
        document.removeEventListener('mouseup', handleColumnDragEnd)
        // State is intentionally NOT cleared here. Close + reopen preserves the user's last
        // query, filters, scope, results, and cursor. Explicit reset lives behind ⌘N.
    })

    /**
     * Schedules a debounced auto-apply search. Three gates layered on top of the timer:
     *   1. AI mode never auto-applies. AI calls cost money; the user must press Enter / ⌘Enter /
     *      click the ⏎ run button.
     *   2. `search.autoApply` (live-mirrored): when off, the user runs every search explicitly. The
     *      bar shows "Press Enter to search" so the contract is visible.
     *   3. IME composition: while a composition is in progress, we don't schedule. On
     *      `compositionend` the parent calls `scheduleSearch` again so the user gets one fire after
     *      composition completes, not multiple fires mid-character.
     * Constant: `SEARCH_AUTO_APPLY_DEBOUNCE_MS` (1 s) — bumped from the legacy 200 ms in M6.
     */
    function scheduleSearch(): void {
        if (debounceTimer) clearTimeout(debounceTimer)
        if (getMode() === 'ai') return
        if (!autoApplyEnabled) return
        if (imeComposing) return
        debounceTimer = setTimeout(() => {
            void executeSearch()
        }, SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    }

    /** Marks the start of an IME composition. Auto-apply is suppressed until `compositionend`. */
    function handleCompositionStart(): void {
        imeComposing = true
        if (debounceTimer) clearTimeout(debounceTimer)
    }

    /**
     * Marks the end of an IME composition. Resets the debounce timer so the user gets exactly one
     * auto-apply fire after the full composed character lands (when the gates from `scheduleSearch`
     * allow it).
     */
    function handleCompositionEnd(): void {
        imeComposing = false
        scheduleSearch()
    }

    /** Runs a search from the ⏎ button or Enter, dispatching to AI or non-AI based on mode. */
    function runFromButton(): void {
        if (inputsDisabled) return
        if (getMode() === 'ai') {
            runAiFromQuery()
        } else {
            void executeSearch()
        }
    }

    /**
     * Runs a search using the current state.
     *
     * `fromAiTranslation` is true only when called from `executeAiSearch()` (after the AI translation
     * has populated state). In that branch we keep the AI transparency strip's `lastAiPrompt` /
     * `lastAiCaveat` intact (they were just set). In every other branch (the user typed and the
     * debounce fired, the user pressed Enter in filename/regex mode, etc.) we clear the strip so it
     * doesn't outlive the AI search it belongs to.
     */
    async function executeSearch(fromAiTranslation = false): Promise<void> {
        if (debounceTimer) clearTimeout(debounceTimer)
        hasSearched = true
        if (!getIsIndexReady()) return

        setIsSearching(true)
        try {
            const query = buildSearchQuery()
            // Parse scope and merge into query if non-empty
            const scopeStr = getScope().trim()
            if (scopeStr) {
                const parsed = await parseSearchScope(scopeStr)
                if (parsed.includePaths.length > 0) query.includePaths = parsed.includePaths
                if (parsed.excludePatterns.length > 0) query.excludeDirNames = parsed.excludePatterns
            }
            const result = await searchFiles(query)
            setResults(result.entries)
            setTotalCount(result.totalCount)
            setCursorIndex(0)
            hoveredIndex = null
            // Track what was actually run so the "Press Enter to search" hint can detect drift.
            lastRunQuery = getQuery()
            if (!fromAiTranslation) {
                // A non-AI search completed cleanly. The AI transparency strip belongs to the
                // previous AI search, so we drop it here. AI runs go through `executeAiSearch`,
                // which sets the strip and then calls us with `fromAiTranslation = true`.
                setLastAiPrompt(null)
                setLastAiCaveat(null)
            }
        } catch {
            // IPC error: ignore silently
        } finally {
            setIsSearching(false)
        }
    }

    /** Applies AI-returned size filters to the UI state. Returns true if any were applied. */
    function applySizeFilters(display: { minSize?: number | null; maxSize?: number | null }): boolean {
        if (display.minSize == null && display.maxSize == null) return false
        if (display.minSize != null && display.maxSize != null) {
            setSizeFilter('between')
            const { value: minVal, unit: minUnit } = bytesToDisplaySize(display.minSize)
            setSizeValue(minVal)
            setSizeUnit(minUnit)
            const { value: maxVal, unit: maxUnit } = bytesToDisplaySize(display.maxSize)
            setSizeValueMax(maxVal)
            setSizeUnitMax(maxUnit)
        } else if (display.minSize != null) {
            setSizeFilter('gte')
            const { value, unit } = bytesToDisplaySize(display.minSize)
            setSizeValue(value)
            setSizeUnit(unit)
        } else if (display.maxSize != null) {
            setSizeFilter('lte')
            const { value, unit } = bytesToDisplaySize(display.maxSize)
            setSizeValue(value)
            setSizeUnit(unit)
        }
        return true
    }

    /** Applies AI-returned date filters to the UI state. Returns true if any were applied. */
    function applyDateFilters(display: { modifiedAfter?: string | null; modifiedBefore?: string | null }): boolean {
        if (display.modifiedAfter == null && display.modifiedBefore == null) return false
        if (display.modifiedAfter != null && display.modifiedBefore != null) {
            setDateFilter('between')
            setDateValue(display.modifiedAfter)
            setDateValueMax(display.modifiedBefore)
        } else if (display.modifiedAfter != null) {
            setDateFilter('after')
            setDateValue(display.modifiedAfter)
        } else if (display.modifiedBefore != null) {
            setDateFilter('before')
            setDateValue(display.modifiedBefore)
        }
        return true
    }

    /**
     * Populates filter fields from AI response. Returns the set of changed field names.
     * Also flips `mode` and overwrites `query` to reflect the AI's translation, so the user sees
     * exactly what was searched and can keep iterating on it. M4 will surface the original prompt
     * separately in the transparency strip.
     */
    function applyAiFilters(result: {
        display: {
            namePattern?: string | null
            patternType?: string | null
            minSize?: number | null
            maxSize?: number | null
            modifiedAfter?: string | null
            modifiedBefore?: string | null
        }
        query: {
            includePaths?: string[] | null
            excludeDirNames?: string[] | null
            caseSensitive?: boolean | null
            excludeSystemDirs?: boolean | null
        }
    }): SvelteSet<string> {
        const changed = new SvelteSet<string>()
        if (result.display.namePattern != null) {
            setQuery(result.display.namePattern)
            changed.add('query')
        }
        if (result.display.patternType === 'regex') {
            setMode('regex')
            changed.add('mode')
        } else if (result.display.patternType === 'glob') {
            setMode('filename')
            changed.add('mode')
        }
        if (result.query.caseSensitive != null) {
            setCaseSensitive(result.query.caseSensitive)
            changed.add('caseSensitive')
        }
        if (result.query.excludeSystemDirs === false) {
            setExcludeSystemDirs(false)
            changed.add('excludeSystemDirs')
        }
        if (applySizeFilters(result.display)) changed.add('size')
        if (applyDateFilters(result.display)) changed.add('date')

        if (result.query.includePaths?.length || result.query.excludeDirNames?.length) {
            const parts: string[] = []
            if (result.query.includePaths) parts.push(...result.query.includePaths)
            if (result.query.excludeDirNames) parts.push(...result.query.excludeDirNames.map((d: string) => `!${d}`))
            setScope(parts.join(', '))
            changed.add('scope')
        }
        return changed
    }

    /** Applies AI filters and briefly highlights the changed fields. */
    function applyAiFiltersWithHighlight(result: Parameters<typeof applyAiFilters>[0]): void {
        highlightedFields = applyAiFilters(result)
        setTimeout(() => {
            highlightedFields = new SvelteSet()
        }, 1500)
    }

    /** Focuses the first result row for keyboard navigation. */
    async function focusFirstResult(): Promise<void> {
        await tick()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte 5 bind:this lacks type info for exports
        searchResultsComponent?.scrollCursorIntoView()
    }

    /** Runs AI translation for a given query text, populates filters, and searches. */
    async function executeAiSearch(queryText: string): Promise<void> {
        const trimmed = queryText.trim()
        if (!trimmed) return

        // Capture the original natural-language prompt BEFORE the translation overwrites `query`
        // and `mode`. This is what the transparency strip shows; without this line the prompt is
        // unrecoverable after the AI fills in the bar with its translated pattern.
        setLastAiPrompt(trimmed)
        setLastAiCaveat(null)

        let translateResult: Awaited<ReturnType<typeof translateSearchQuery>>
        try {
            translateResult = await translateSearchQuery(trimmed)
        } catch {
            // AI translation failed; bail out silently. Surfacing the error to the user lands in M5
            // alongside the empty state and example-query plumbing.
            return
        }

        applyAiFiltersWithHighlight(translateResult)
        setLastAiCaveat(translateResult.caveat ?? null)

        // Search using the translated query directly. `fromAiTranslation` keeps the transparency
        // strip alive across the subsequent searchFiles call.
        await executeSearch(true)
        await focusFirstResult()
    }

    function bytesToDisplaySize(bytes: number): { value: string; unit: 'KB' | 'MB' | 'GB' } {
        if (bytes >= 1024 * 1024 * 1024) {
            return { value: String(Math.round((bytes / (1024 * 1024 * 1024)) * 100) / 100), unit: 'GB' }
        }
        if (bytes >= 1024 * 1024) {
            return { value: String(Math.round((bytes / (1024 * 1024)) * 100) / 100), unit: 'MB' }
        }
        return { value: String(Math.round((bytes / 1024) * 100) / 100), unit: 'KB' }
    }

    /** Returns the chip slot for a given keyboard shortcut number (⌘1 / ⌘2 / ⌘3), or null. */
    function modeForShortcutNumber(n: number): SearchMode | null {
        // ⌘4 is reserved for Content when it ships; do not wire it now.
        if (aiEnabled) {
            if (n === 1) return 'ai'
            if (n === 2) return 'filename'
            if (n === 3) return 'regex'
        } else {
            if (n === 1) return 'filename'
            if (n === 2) return 'regex'
        }
        return null
    }

    function handleModeChange(newMode: SearchMode): void {
        if (getMode() === newMode) return
        setMode(newMode)
        // Switching mode preserves the typed query; only re-trigger auto-apply for non-AI modes.
        if (newMode !== 'ai') scheduleSearch()
    }

    function handleQueryInput(value: string): void {
        setQuery(value)
        scheduleSearch()
    }

    function inputHandler(setter: (v: string) => void, search = true) {
        return (e: Event) => {
            setter((e.target as HTMLInputElement).value)
            if (search) scheduleSearch()
        }
    }

    // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- T constrains the setter's param type to match the cast
    function selectHandler<T extends string>(setter: (v: T) => void, search = true) {
        return (e: Event) => {
            setter((e.target as HTMLSelectElement).value as T)
            if (search) scheduleSearch()
        }
    }

    /** Traps Tab focus within the dialog. Returns true if the event was handled. */
    function handleTabFocusTrap(e: KeyboardEvent): boolean {
        if (e.key !== 'Tab' || !dialogElement) return false
        const focusable = dialogElement.querySelectorAll<HTMLElement>(
            'input:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])',
        )
        if (focusable.length > 0) {
            const first = focusable[0]
            const last = focusable[focusable.length - 1]
            if (e.shiftKey && document.activeElement === first) {
                e.preventDefault()
                last.focus()
            } else if (!e.shiftKey && document.activeElement === last) {
                e.preventDefault()
                first.focus()
            }
        }
        return true
    }

    /** Returns true if the active element is the unified query input. */
    function isInQueryInput(): boolean {
        return document.activeElement === queryInputElement
    }

    /** Matches a plain modifier-key combo (one of cmd/alt, no others, no shift). */
    function matchKey(e: KeyboardEvent, key: string, mod: 'meta' | 'alt'): boolean {
        if (e.key !== key || e.shiftKey) return false
        return mod === 'meta' ? e.metaKey && !e.altKey : e.altKey && !e.metaKey
    }

    /** Clears all dialog state (⌘N "new search") and refocuses the query input. */
    function clearAndRefocus(): void {
        clearSearchState()
        lastRunQuery = null
        void tick().then(() => {
            focusInput()
        })
    }

    /** Runs an AI search from the current query; no-op when AI is off or the query is empty. */
    function runAiFromQuery(): void {
        if (!aiEnabled) return
        const trimmed = getQuery().trim()
        if (trimmed) void executeAiSearch(trimmed)
    }

    /** Handles ⌘1 / ⌘2 / ⌘3 mode switches. Returns true if handled. */
    function handleModeShortcut(e: KeyboardEvent): boolean {
        if (!e.metaKey || e.altKey || e.shiftKey) return false
        if (e.key < '1' || e.key > '9') return false
        const n = parseInt(e.key, 10)
        const target = modeForShortcutNumber(n)
        if (!target) return false
        e.preventDefault()
        handleModeChange(target)
        // Keep the input focused so the user can keep typing.
        focusInput()
        return true
    }

    /** Handles modifier-key shortcuts (⌘N, ⌥F, ⌥D, ⌘Enter, ⌘1-⌘3). Returns true if handled. */
    function handleModifierShortcuts(e: KeyboardEvent): boolean {
        // ⌘N: clear search state and start fresh. Captured here so the global ⌘N (new tab) doesn't
        // fire while the dialog is open. The dialog already calls stopPropagation on every keydown,
        // but this handler is also the source of truth for the in-dialog "new search" affordance.
        if (matchKey(e, 'n', 'meta')) {
            e.preventDefault()
            clearAndRefocus()
            return true
        }
        if (matchKey(e, 'f', 'alt')) {
            e.preventDefault()
            setScope(currentFolderPath)
            scheduleSearch()
            return true
        }
        if (matchKey(e, 'd', 'alt')) {
            e.preventDefault()
            setScope('')
            scheduleSearch()
            return true
        }
        if (matchKey(e, 'Enter', 'meta')) {
            e.preventDefault()
            runAiFromQuery()
            return true
        }
        // ⌘H toggles the recent-searches popover. The popover owns its own Esc, so users can
        // dismiss it without closing the dialog.
        if (matchKey(e, 'h', 'meta')) {
            e.preventDefault()
            if (recentPopoverOpen) {
                closeRecentPopover()
            } else {
                openRecentPopover()
            }
            return true
        }
        if (handleModeShortcut(e)) return true
        return false
    }

    /** Handles arrow key navigation in the results list. */
    function handleArrowNav(e: KeyboardEvent): void {
        if (results.length === 0) return
        e.preventDefault()
        if (e.key === 'ArrowDown') {
            setCursorIndex(Math.min(getCursorIndex() + 1, results.length - 1))
        } else {
            setCursorIndex(Math.max(getCursorIndex() - 1, 0))
        }
        hoveredIndex = null
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte 5 bind:this lacks type info for exports
        searchResultsComponent?.scrollCursorIntoView()
    }

    function handleKeyDown(e: KeyboardEvent): void {
        e.stopPropagation()

        if (handleTabFocusTrap(e)) return
        if (handleModifierShortcuts(e)) return

        switch (e.key) {
            case 'Escape':
                e.preventDefault()
                onClose()
                break
            case 'ArrowDown':
            case 'ArrowUp':
                // Ignore arrow keys when focus is on a mode chip; the chip row owns ←/→ for chip
                // nav, and ArrowUp/Down shouldn't fight that.
                if (isInQueryInput() || document.activeElement?.closest('.results-container')) {
                    handleArrowNav(e)
                }
                break
            case 'Enter':
                e.preventDefault()
                handleEnterKey()
                break
        }
    }

    /** Handles plain Enter key based on the active mode and what's focused. */
    function handleEnterKey(): void {
        if (isInQueryInput()) {
            if (getMode() === 'ai') {
                runAiFromQuery()
            } else {
                void executeSearch()
            }
            return
        }
        if (cursorIndex < results.length) {
            onNavigate(results[cursorIndex].path)
            return
        }
        void executeSearch()
    }

    function handleResultClick(index: number): void {
        if (index < results.length) {
            onNavigate(results[index].path)
        }
    }

    function handleOverlayClick(e: MouseEvent): void {
        if (e.target === e.currentTarget) {
            onClose()
        }
    }
</script>

<div
    class="search-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="search-dialog-title"
    tabindex="-1"
    onclick={handleOverlayClick}
    onkeydown={handleKeyDown}
>
    <div class="search-dialog" bind:this={dialogElement}>
        <h2 id="search-dialog-title" class="sr-only">Search files</h2>

        <SearchBar
            bind:inputElement={queryInputElement}
            {query}
            {mode}
            disabled={inputsDisabled}
            aiHighlight={highlightedFields.has('query')}
            {showRunHint}
            onInput={handleQueryInput}
            onRun={runFromButton}
            onCompositionStart={handleCompositionStart}
            onCompositionEnd={handleCompositionEnd}
        />

        <SearchModeChips {mode} {aiEnabled} disabled={inputsDisabled} onSelect={handleModeChange} />

        {#if lastAiPrompt}
            <AiTransparencyStrip aiPrompt={lastAiPrompt} caveat={lastAiCaveat ?? ''} />
        {/if}

        <SearchFilterChips
            {caseSensitive}
            {scope}
            {excludeSystemDirs}
            {currentFolderPath}
            {sizeFilter}
            {sizeValue}
            {sizeUnit}
            {sizeValueMax}
            {sizeUnitMax}
            {dateFilter}
            {dateValue}
            {dateValueMax}
            {systemDirExcludeTooltip}
            {highlightedFields}
            disabled={inputsDisabled}
            onInput={inputHandler}
            onSelect={selectHandler}
            onToggleCaseSensitive={() => {
                setCaseSensitive(!getCaseSensitive())
                scheduleSearch()
            }}
            onToggleExcludeSystemDirs={() => {
                setExcludeSystemDirs(!getExcludeSystemDirs())
                scheduleSearch()
            }}
            onSetScope={setScope}
            {scheduleSearch}
        />

        <SearchResults
            bind:this={searchResultsComponent}
            bind:hoveredIndex
            {results}
            {cursorIndex}
            {isIndexAvailable}
            {isIndexReady}
            {isSearching}
            {hasSearched}
            {query}
            {sizeFilter}
            {dateFilter}
            {scanning}
            {entriesScanned}
            {totalCount}
            {indexEntryCount}
            {gridTemplate}
            iconCacheVersion={iconVersion}
            {aiEnabled}
            onResultClick={handleResultClick}
            onColumnDragStart={(col: string, e: MouseEvent) => {
                handleColumnDragStart(col as keyof typeof colWidths, e)
            }}
            onPickExample={pickExample}
        />

        <div bind:this={footerRef}>
            <RecentSearchesFooter
                entries={recentEntries}
                disabled={inputsDisabled}
                onPick={activateHistoryEntry}
                onRemove={removeHistoryEntry}
                onOpenAll={openRecentPopover}
            />
        </div>

        {#if footerRef}
            <RecentSearchesPopover
                anchor={footerRef}
                open={recentPopoverOpen}
                entries={recentEntries}
                onClose={closeRecentPopover}
                onPick={activateHistoryEntry}
                onRemove={removeHistoryEntry}
            />
        {/if}
    </div>
</div>

<style>
    .search-overlay {
        position: fixed;
        inset: 0;
        background: var(--color-overlay-light);
        display: flex;
        justify-content: center;
        align-items: flex-start;
        padding-top: 10vh;
        z-index: var(--z-modal);
    }

    .search-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-lg);
        width: 1080px;
        display: flex;
        flex-direction: column;
        box-shadow: var(--shadow-lg);
        overflow: hidden;
    }

    /* Visually hidden but accessible to screen readers */
    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        margin: -1px;
        overflow: hidden;
        clip-path: inset(50%);
        white-space: nowrap;
        border: 0;
    }
</style>
