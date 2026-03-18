<script lang="ts">
    /**
     * SearchDialog - Whole-drive file search overlay.
     *
     * Follows the command palette pattern (custom overlay, not ModalDialog).
     * Searches the in-memory index by filename (wildcards), size, and date.
     *
     * Input layout:
     * - AI enabled: two rows — AI prompt row (top, focused) + pattern row (bottom)
     * - AI disabled: single pattern row with Search button
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
        formatBytes,
    } from '$lib/tauri-commands'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import type { UnlistenFn, PreflightContext } from '$lib/tauri-commands'
    import { getSetting } from '$lib/settings'
    import { isScanning, getEntriesScanned } from '$lib/indexing'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        getNamePattern,
        setNamePattern,
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
        getAiStatus,
        setAiStatus,
        getAiPrompt,
        setAiPrompt,
        getPatternType,
        setPatternType,
        getCaseSensitive,
        setCaseSensitive,
        getScope,
        setScope,
        getExcludeSystemDirs,
        setExcludeSystemDirs,
        getPreflightText,
        setPreflightText,
        getCaveat,
        setCaveat,
        buildSearchQuery,
        resetSearchState,
        type SizeFilter,
        type DateFilter,
        type SizeUnit,
        type PatternType,
    } from './search-state.svelte'

    interface Props {
        /** Called when user selects a result: receives the full path */
        onNavigate: (path: string) => void
        /** Called when dialog is closed */
        onClose: () => void
        /** Current directory path of the focused pane (for ⌥F scope shortcut) */
        currentFolderPath: string
    }

    const { onNavigate, onClose, currentFolderPath }: Props = $props()

    let aiPromptInputElement: HTMLInputElement | undefined = $state()
    let patternInputElement: HTMLInputElement | undefined = $state()
    let dialogElement: HTMLDivElement | undefined = $state()
    let resultsContainer: HTMLDivElement | undefined = $state()
    let hoveredIndex = $state<number | null>(null)
    let debounceTimer: ReturnType<typeof setTimeout> | undefined
    let unlistenReady: UnlistenFn | undefined
    let systemDirExcludeTooltip = $state('Excludes common system and build folders')

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
    const namePattern = $derived(getNamePattern())
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
    const aiStatus = $derived(getAiStatus())
    const aiPrompt = $derived(getAiPrompt())
    const patternType = $derived(getPatternType())
    const caseSensitive = $derived(getCaseSensitive())
    const scope = $derived(getScope())
    const excludeSystemDirs = $derived(getExcludeSystemDirs())
    const preflightText = $derived(getPreflightText())
    const caveatText = $derived(getCaveat())
    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())

    /** Whether AI search is enabled (provider configured and index available). */
    const aiEnabled = $derived(getSetting('ai.provider') !== 'off' && isIndexAvailable)
    /** Whether inputs/filters should be disabled (index not available or still scanning with no index). */
    const inputsDisabled = $derived(!isIndexAvailable)

    let aiError = $state('')
    let aiGeneration = 0
    let highlightedFields = new SvelteSet<string>()
    /** True once the user has triggered at least one search (so we can distinguish "no query yet" from "0 results"). */
    let hasSearched = $state(false)

    // Subscribe to icon cache version for reactivity
    const _iconVersion = $derived($iconCacheVersion)

    function getIconUrl(iconId: string): string | undefined {
        void _iconVersion
        return getCachedIcon(iconId)
    }

    /** Focuses the appropriate input based on whether AI is enabled. */
    function focusActiveInput(): void {
        if (aiEnabled) {
            aiPromptInputElement?.focus()
        } else {
            patternInputElement?.focus()
        }
    }

    /** Capture-phase Escape handler — fires before native elements (select, date picker) can consume the event. */
    function handleEscapeCapture(e: KeyboardEvent): void {
        if (e.key === 'Escape') {
            e.preventDefault()
            e.stopPropagation()
            onClose()
        }
    }

    onMount(async () => {
        notifyDialogOpened('search').catch(() => {})
        window.addEventListener('keydown', handleEscapeCapture, true)

        // Listen for index ready event
        unlistenReady = await onSearchIndexReady((entryCount: number) => {
            setIsIndexReady(true)
            setIndexEntryCount(entryCount)
            // Auto-run pending search if user already typed something
            if (getNamePattern().trim() || getSizeFilter() !== 'any' || getDateFilter() !== 'any') {
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
            // Index not available — indexing disabled, not started, or backend unavailable
            setIsIndexAvailable(false)
        }

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
        focusActiveInput()
    })

    onDestroy(() => {
        notifyDialogClosed('search').catch(() => {})
        releaseSearchIndex().catch(() => {})
        unlistenReady?.()
        window.removeEventListener('keydown', handleEscapeCapture, true)
        if (debounceTimer) clearTimeout(debounceTimer)
        // Clean up any in-progress column drag
        document.removeEventListener('mousemove', handleColumnDragMove)
        document.removeEventListener('mouseup', handleColumnDragEnd)
        resetSearchState()
    })

    function scheduleSearch(): void {
        if (debounceTimer) clearTimeout(debounceTimer)
        debounceTimer = setTimeout(() => {
            void executeSearch()
        }, 200)
    }

    async function executeSearch(): Promise<void> {
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
        } catch {
            // IPC error — ignore silently
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

    /** Populates filter fields from AI response. Returns the set of changed field names. */
    function applyAiFilters(result: {
        display: {
            namePattern?: string | null
            patternType?: string | null
            minSize?: number | null
            maxSize?: number | null
            modifiedAfter?: string | null
            modifiedBefore?: string | null
        }
        query: { includePaths?: string[]; excludeDirNames?: string[]; caseSensitive?: boolean }
    }): SvelteSet<string> {
        const changed = new SvelteSet<string>()
        if (result.display.namePattern != null) {
            setNamePattern(result.display.namePattern)
            changed.add('name')
        }
        if (result.display.patternType === 'regex' || result.display.patternType === 'glob') {
            setPatternType(result.display.patternType as PatternType)
            changed.add('patternType')
        }
        if (result.query.caseSensitive != null) {
            setCaseSensitive(result.query.caseSensitive)
            changed.add('caseSensitive')
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

    /** Shows search results in the UI and marks that a search has been performed. */
    function showResults(entries: Parameters<typeof setResults>[0], totalCount: number): void {
        setResults(entries)
        setTotalCount(totalCount)
        setCursorIndex(0)
        hasSearched = true
    }

    /** Focuses the first result row for keyboard navigation. */
    async function focusFirstResult(): Promise<void> {
        await tick()
        const firstResult = resultsContainer?.querySelector('.result-row') as HTMLElement | null
        firstResult?.focus()
    }

    /** Builds a preflight search query with scope and system dir exclusions applied. */
    async function buildPreflightQuery(): Promise<ReturnType<typeof buildSearchQuery>> {
        const preflightQuery = buildSearchQuery()
        preflightQuery.limit = 50
        const scopeStr = getScope().trim()
        if (scopeStr) {
            const parsed = await parseSearchScope(scopeStr)
            if (parsed.includePaths.length > 0) preflightQuery.includePaths = parsed.includePaths
            if (parsed.excludePatterns.length > 0) preflightQuery.excludeDirNames = parsed.excludePatterns
        }
        return preflightQuery
    }

    /** Phase 1: Calls the LLM for an initial broad translation. Returns null on failure or staleness. */
    async function aiPhase1BroadQuery(
        query: string,
        providerLabel: string,
        isStale: () => boolean,
    ): Promise<Awaited<ReturnType<typeof translateSearchQuery>> | null> {
        setAiStatus(`Calling ${providerLabel}...`)
        try {
            const result = await translateSearchQuery(query)
            if (isStale()) return null
            return result
        } catch (e: unknown) {
            if (isStale()) return null
            aiError = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
            setAiStatus('')
            setPreflightText('')
            return null
        }
    }

    /** Phase 2: Runs preflight search. Returns null on failure (falls back to pass 1 search). */
    async function aiPhase2Preflight(isStale: () => boolean): Promise<Awaited<ReturnType<typeof searchFiles>> | null> {
        setAiStatus('Running preflight...')
        try {
            const preflightQuery = await buildPreflightQuery()
            const result = await searchFiles(preflightQuery)
            if (isStale()) return null
            return result
        } catch {
            if (isStale()) return null
            setAiStatus('Searching...')
            await executeSearch()
            setAiStatus('')
            return null
        }
    }

    /** Phase 4: Refines query with preflight context and runs final search. */
    async function aiPhase4Refine(
        query: string,
        hitCount: number,
        entries: Awaited<ReturnType<typeof searchFiles>>['entries'],
        summaryPrefix: string,
        isStale: () => boolean,
    ): Promise<void> {
        setAiStatus('Refining query...')
        const preflightContext: PreflightContext = {
            totalCount: hitCount,
            sampleEntries: entries.slice(0, 50).map((e) => ({
                name: e.name,
                size: e.size,
                modifiedAt: e.modifiedAt,
                isDirectory: e.isDirectory,
            })),
        }

        let pass2Result
        try {
            pass2Result = await translateSearchQuery(query, preflightContext)
        } catch {
            if (isStale()) return
            setPreflightText(`${summaryPrefix} → ${hitCount.toLocaleString()} hits · Refinement failed`)
            setAiStatus('')
            return
        }
        if (isStale()) return

        applyAiFiltersWithHighlight(pass2Result)
        setCaveat(pass2Result.caveat ?? '')
        setPreflightText(`${summaryPrefix} → ${hitCount.toLocaleString()} hits · Refined`)

        setAiStatus('Searching...')
        await executeSearch()
        setAiStatus('')
        await focusFirstResult()
    }

    /** Runs the two-pass AI translation for a given query text, populates filters, and auto-runs search. */
    async function executeAiSearch(queryText: string): Promise<void> {
        const query = queryText.trim()
        if (!query) return

        const generation = ++aiGeneration
        const isStale = (): boolean => generation !== aiGeneration

        aiError = ''
        setPreflightText('')
        setCaveat('')
        const provider = getSetting('ai.provider')
        const providerLabel = provider === 'local' ? 'local LLM' : provider

        // Phase 1: Broad query (no preflight context)
        const pass1Result = await aiPhase1BroadQuery(query, providerLabel, isStale)
        if (!pass1Result) return

        const pass1Summary = pass1Result.preflightSummary ?? ''
        setPreflightText(pass1Summary ? `Preflight: ${pass1Summary}` : '')
        setCaveat(pass1Result.caveat ?? '')
        applyAiFiltersWithHighlight(pass1Result)

        // Phase 2: Run preflight search with broad filters
        const preflightResult = await aiPhase2Preflight(isStale)
        if (!preflightResult) return

        const hitCount = preflightResult.totalCount
        const summaryPrefix = pass1Summary ? `Preflight: ${pass1Summary}` : 'Preflight'

        // Phase 3: Check if refinement is needed
        if (hitCount === 0) {
            showResults(preflightResult.entries, hitCount)
            setPreflightText(`${summaryPrefix} → 0 hits`)
            setAiStatus('')
            return
        }

        if (hitCount <= 10) {
            showResults(preflightResult.entries, hitCount)
            setPreflightText(`${summaryPrefix} → ${String(hitCount)} hits`)
            setAiStatus('')
            await focusFirstResult()
            return
        }

        // Show intermediate results while refining
        showResults(preflightResult.entries, hitCount)
        setPreflightText(`${summaryPrefix} → ${hitCount.toLocaleString()} hits · Refining...`)

        // Phase 4–5: Refinement and final search
        await aiPhase4Refine(query, hitCount, preflightResult.entries, summaryPrefix, isStale)
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

    function togglePatternType(): void {
        setPatternType(getPatternType() === 'glob' ? 'regex' : 'glob')
        scheduleSearch()
    }

    function toggleCaseSensitive(): void {
        setCaseSensitive(!getCaseSensitive())
        scheduleSearch()
    }

    function handlePatternInput(e: Event): void {
        const target = e.target as HTMLInputElement
        setNamePattern(target.value)
        scheduleSearch()
    }

    function handleAiPromptInput(e: Event): void {
        const target = e.target as HTMLInputElement
        setAiPrompt(target.value)
    }

    function handleScopeInput(e: Event): void {
        const target = e.target as HTMLInputElement
        setScope(target.value)
        scheduleSearch()
    }

    function toggleExcludeSystemDirs(): void {
        setExcludeSystemDirs(!getExcludeSystemDirs())
        scheduleSearch()
    }

    function handleSizeFilterChange(e: Event): void {
        setSizeFilter((e.target as HTMLSelectElement).value as SizeFilter)
        scheduleSearch()
    }

    function handleSizeValueInput(e: Event): void {
        setSizeValue((e.target as HTMLInputElement).value)
        scheduleSearch()
    }

    function handleSizeUnitChange(e: Event): void {
        setSizeUnit((e.target as HTMLSelectElement).value as SizeUnit)
        scheduleSearch()
    }

    function handleSizeValueMaxInput(e: Event): void {
        setSizeValueMax((e.target as HTMLInputElement).value)
        scheduleSearch()
    }

    function handleSizeUnitMaxChange(e: Event): void {
        setSizeUnitMax((e.target as HTMLSelectElement).value as SizeUnit)
        scheduleSearch()
    }

    function handleDateFilterChange(e: Event): void {
        setDateFilter((e.target as HTMLSelectElement).value as DateFilter)
        scheduleSearch()
    }

    function handleDateValueInput(e: Event): void {
        setDateValue((e.target as HTMLInputElement).value)
        scheduleSearch()
    }

    function handleDateValueMaxInput(e: Event): void {
        setDateValueMax((e.target as HTMLInputElement).value)
        scheduleSearch()
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

    /** Returns true if the active element is the AI prompt input. */
    function isInAiPromptInput(): boolean {
        return document.activeElement === aiPromptInputElement
    }

    /** Returns true if the active element is the pattern input. */
    function isInPatternInput(): boolean {
        return document.activeElement === patternInputElement
    }

    /** Handles modifier-key shortcuts (⌥F, ⌥D, ⌘Enter). Returns true if handled. */
    function handleModifierShortcuts(e: KeyboardEvent): boolean {
        // ⌥F — set scope to current folder path
        if (e.altKey && !e.metaKey && !e.shiftKey && e.key === 'f') {
            e.preventDefault()
            setScope(currentFolderPath)
            scheduleSearch()
            return true
        }
        // ⌥D — clear scope (search entire drive)
        if (e.altKey && !e.metaKey && !e.shiftKey && e.key === 'd') {
            e.preventDefault()
            setScope('')
            scheduleSearch()
            return true
        }
        // ⌘Enter triggers AI search
        if (e.key === 'Enter' && e.metaKey && !e.shiftKey && !e.altKey) {
            e.preventDefault()
            if (!aiEnabled) return true
            const prompt = getAiPrompt().trim()
            if (prompt) void executeAiSearch(prompt)
            return true
        }
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
        scrollCursorIntoView()
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
                handleArrowNav(e)
                break
            case 'Enter':
                e.preventDefault()
                handleEnterKey()
                break
        }
    }

    /** Handles plain Enter key based on which input is focused. */
    function handleEnterKey(): void {
        if (isInAiPromptInput()) {
            // Enter in AI prompt row: run AI search
            void executeAiSearch(getAiPrompt())
        } else if (isInPatternInput()) {
            // Enter in pattern row: run manual search immediately
            void executeSearch()
        } else if (cursorIndex < results.length) {
            // Enter with results focused: navigate
            onNavigate(results[cursorIndex].path)
        } else {
            void executeSearch()
        }
    }

    function scrollCursorIntoView(): void {
        void tick().then(() => {
            const cursor = resultsContainer?.querySelector('.result-row.is-under-cursor')
            cursor?.scrollIntoView({ block: 'nearest' })
        })
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

    function formatSize(bytes: number | null | undefined): string {
        if (bytes == null) return ''
        return formatBytes(bytes)
    }

    /** Formats a unix timestamp (seconds) as YYYY-MM-DD. */
    function formatDate(timestamp: number | null | undefined): string {
        if (timestamp == null) return ''

        const d = new Date(timestamp * 1000)
        const year = d.getFullYear()
        const month = String(d.getMonth() + 1).padStart(2, '0')
        const day = String(d.getDate()).padStart(2, '0')
        return `${String(year)}-${month}-${day}`
    }

    function formatEntryCount(count: number): string {
        if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`
        if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`
        return String(count)
    }

    function getStatusText(): string {
        if (!isIndexAvailable) {
            if (scanning && entriesScanned > 0) {
                return `Scanning in progress (${formatEntryCount(entriesScanned)} entries)...`
            }
            if (scanning) return 'Scan in progress...'
            return 'Drive index not available'
        }
        if (isIndexReady) {
            if (isSearching) return 'Searching...'
            if (!hasSearched || (!namePattern.trim() && sizeFilter === 'any' && dateFilter === 'any')) {
                return `Index ready (${formatEntryCount(indexEntryCount)} entries)`
            }
            if (totalCount === 0) return 'No results'
            return `${String(results.length)} of ${totalCount.toLocaleString()} results`
        }
        // Index loading — only show status if user has triggered a search
        if (hasSearched) return 'Loading index...'
        return ''
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

        <!-- AI prompt row (visible when AI is enabled) -->
        {#if aiEnabled}
            <div class="input-row ai-prompt-row">
                <span class="row-label ai-label">AI</span>
                <input
                    bind:this={aiPromptInputElement}
                    type="text"
                    class="name-input"
                    placeholder="Describe what you're looking for..."
                    value={aiPrompt}
                    oninput={handleAiPromptInput}
                    disabled={inputsDisabled}
                    aria-label="Natural language search query"
                    spellcheck="false"
                    autocomplete="off"
                    autocapitalize="off"
                />
                <button
                    class="action-button ai-active"
                    onclick={() => void executeAiSearch(getAiPrompt())}
                    disabled={inputsDisabled || !aiPrompt.trim()}
                    title="Ask AI (⌘Enter)"
                >
                    Ask AI
                </button>
            </div>
            <!-- Preflight row (read-only status display) -->
            <div class="preflight-row">
                {#if preflightText}
                    <span class="preflight-label">Preflight:</span>
                    <span class="preflight-content">{preflightText.replace(/^Preflight:\s*/, '')}</span>
                {:else}
                    <span class="preflight-placeholder">AI will analyze results to refine your search</span>
                {/if}
            </div>
            {#if caveatText}
                <div class="caveat-row">{caveatText}</div>
            {/if}
        {/if}

        <!-- Pattern / search row (always visible) -->
        <div class="input-row">
            <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                <circle cx="6.5" cy="6.5" r="5" stroke="currentColor" stroke-width="1.5" />
                <line
                    x1="10.5"
                    y1="10.5"
                    x2="14.5"
                    y2="14.5"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                />
            </svg>
            <input
                bind:this={patternInputElement}
                type="text"
                class="name-input"
                class:ai-highlight={highlightedFields.has('name')}
                placeholder={patternType === 'regex'
                    ? 'Regular expression pattern'
                    : 'Filename pattern (use * and ? as wildcards)'}
                value={namePattern}
                oninput={handlePatternInput}
                disabled={inputsDisabled}
                aria-label="Filename pattern"
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
            />
            <button
                class="pattern-type-toggle"
                class:active={caseSensitive}
                class:ai-highlight={highlightedFields.has('caseSensitive')}
                onclick={toggleCaseSensitive}
                disabled={inputsDisabled}
                title={caseSensitive ? 'Case-sensitive' : 'Case-insensitive'}
                aria-label={caseSensitive ? 'Case-sensitive' : 'Case-insensitive'}
            >
                Aa
            </button>
            <button
                class="pattern-type-toggle"
                class:ai-highlight={highlightedFields.has('patternType')}
                onclick={togglePatternType}
                disabled={inputsDisabled}
                title="Toggle between glob and regex matching"
                aria-label="Pattern type: {patternType === 'regex' ? 'Regex' : 'Glob'}"
            >
                {patternType === 'regex' ? 'Regex' : 'Glob'}
            </button>
            <button
                class="action-button"
                onclick={() => void executeSearch()}
                disabled={inputsDisabled}
                title="Search (Enter)"
            >
                Search
            </button>
        </div>

        <!-- Scope row -->
        <div class="input-row">
            <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path
                    d="M2 4.5V12.5C2 13.05 2.45 13.5 3 13.5H13C13.55 13.5 14 13.05 14 12.5V6.5C14 5.95 13.55 5.5 13 5.5H8L6.5 3.5H3C2.45 3.5 2 3.95 2 4.5Z"
                    stroke="currentColor"
                    stroke-width="1.3"
                    fill="none"
                />
            </svg>
            <input
                type="text"
                class="name-input"
                class:ai-highlight={highlightedFields.has('scope')}
                placeholder="All folders"
                value={scope}
                oninput={handleScopeInput}
                disabled={inputsDisabled}
                aria-label="Search scope"
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
            />
            <div class="scope-info-wrapper">
                <button
                    class="scope-info-button"
                    use:tooltip={{
                        html:
                            '<div style="max-width:380px">' +
                            '<div style="font-weight:600;margin-bottom:4px">Search scope — which folders to search in</div>' +
                            '<div style="color:var(--color-text-secondary);margin-bottom:8px">Comma-separated paths. Use ! to exclude.</div>' +
                            '<table style="border-spacing:0;margin-bottom:8px;width:100%">' +
                            '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects</code></td><td style="color:var(--color-text-secondary)">Search in one folder</td></tr>' +
                            '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects, ~/Documents</code></td><td style="color:var(--color-text-secondary)">Search in multiple folders</td></tr>' +
                            '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>!node_modules, !.git</code></td><td style="color:var(--color-text-secondary)">Exclude folders by name</td></tr>' +
                            '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects, !node_modules</code></td><td style="color:var(--color-text-secondary)">Combine include and exclude</td></tr>' +
                            '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>!.*</code></td><td style="color:var(--color-text-secondary)">Exclude hidden folders</td></tr>' +
                            '</table>' +
                            '<div style="color:var(--color-text-secondary)">Wildcards * and ? work in folder names.<br>Use quotes or backslash to escape commas.</div>' +
                            '</div>',
                    }}
                    disabled={inputsDisabled}
                    aria-label="Scope syntax help"
                >
                    i
                </button>
            </div>
            <button
                class="pattern-type-toggle"
                class:active={excludeSystemDirs}
                onclick={toggleExcludeSystemDirs}
                disabled={inputsDisabled}
                use:tooltip={{ html: systemDirExcludeTooltip }}
                aria-label={excludeSystemDirs ? 'System folders excluded' : 'System folders included'}
            >
                Filter
            </button>
            <button
                class="pattern-type-toggle"
                onclick={() => {
                    setScope(currentFolderPath)
                    scheduleSearch()
                }}
                disabled={inputsDisabled}
                title="Scope to current folder (⌥F)"
                aria-label="Scope to current folder"
            >
                ⌥F
            </button>
            <button
                class="pattern-type-toggle"
                onclick={() => {
                    setScope('')
                    scheduleSearch()
                }}
                disabled={inputsDisabled}
                title="Search entire drive (⌥D)"
                aria-label="Search entire drive"
            >
                ⌥D
            </button>
        </div>

        <!-- AI status / error -->
        {#if aiStatus}
            <div class="ai-status">{aiStatus}</div>
        {/if}
        {#if aiError}
            <div class="ai-error">{aiError}</div>
        {/if}

        <!-- Filter row -->
        <div class="filter-row">
            <div class="filter-group" class:ai-highlight={highlightedFields.has('size')}>
                <label class="filter-label" for="size-filter">Size</label>
                <select
                    id="size-filter"
                    class="filter-select"
                    value={sizeFilter}
                    onchange={handleSizeFilterChange}
                    disabled={inputsDisabled}
                    aria-label="Size filter"
                >
                    <option value="any">any</option>
                    <option value="gte">&ge;</option>
                    <option value="lte">&le;</option>
                    <option value="between">between</option>
                </select>
                {#if sizeFilter !== 'any'}
                    <input
                        type="number"
                        class="filter-input size-input"
                        value={sizeValue}
                        oninput={handleSizeValueInput}
                        disabled={inputsDisabled}
                        aria-label="Minimum size value"
                        min="0"
                        step="any"
                    />
                    <select
                        class="filter-select unit-select"
                        value={sizeUnit}
                        onchange={handleSizeUnitChange}
                        disabled={inputsDisabled}
                        aria-label="Size unit"
                    >
                        <option value="KB">KB</option>
                        <option value="MB">MB</option>
                        <option value="GB">GB</option>
                    </select>
                {/if}
                {#if sizeFilter === 'between'}
                    <span class="filter-separator">–</span>
                    <input
                        type="number"
                        class="filter-input size-input"
                        value={sizeValueMax}
                        oninput={handleSizeValueMaxInput}
                        disabled={inputsDisabled}
                        aria-label="Maximum size value"
                        min="0"
                        step="any"
                    />
                    <select
                        class="filter-select unit-select"
                        value={sizeUnitMax}
                        onchange={handleSizeUnitMaxChange}
                        disabled={inputsDisabled}
                        aria-label="Maximum size unit"
                    >
                        <option value="KB">KB</option>
                        <option value="MB">MB</option>
                        <option value="GB">GB</option>
                    </select>
                {/if}
            </div>

            <div class="filter-group" class:ai-highlight={highlightedFields.has('date')}>
                <label class="filter-label" for="date-filter">Modified</label>
                <select
                    id="date-filter"
                    class="filter-select"
                    value={dateFilter}
                    onchange={handleDateFilterChange}
                    disabled={inputsDisabled}
                    aria-label="Date filter"
                >
                    <option value="any">any</option>
                    <option value="after">after</option>
                    <option value="before">before</option>
                    <option value="between">between</option>
                </select>
                {#if dateFilter !== 'any'}
                    <input
                        type="date"
                        class="filter-input date-input"
                        value={dateValue}
                        oninput={handleDateValueInput}
                        disabled={inputsDisabled}
                        aria-label="Date value"
                    />
                {/if}
                {#if dateFilter === 'between'}
                    <span class="filter-separator">–</span>
                    <input
                        type="date"
                        class="filter-input date-input"
                        value={dateValueMax}
                        oninput={handleDateValueMaxInput}
                        disabled={inputsDisabled}
                        aria-label="Maximum date value"
                    />
                {/if}
            </div>
        </div>

        <!-- Column headers -->
        <div class="column-header" style:grid-template-columns={gridTemplate}>
            <span class="col-label col-icon"></span>
            <span class="col-label">
                Name
                <span
                    class="col-resize-handle"
                    role="separator"
                    onmousedown={(e: MouseEvent) => {
                        handleColumnDragStart('name', e)
                    }}
                ></span>
            </span>
            <span class="col-label">
                Path
                <span
                    class="col-resize-handle"
                    role="separator"
                    onmousedown={(e: MouseEvent) => {
                        handleColumnDragStart('path', e)
                    }}
                ></span>
            </span>
            <span class="col-label col-right">
                Size
                <span
                    class="col-resize-handle"
                    role="separator"
                    onmousedown={(e: MouseEvent) => {
                        handleColumnDragStart('size', e)
                    }}
                ></span>
            </span>
            <span class="col-label col-right">
                Modified
                <span
                    class="col-resize-handle"
                    role="separator"
                    onmousedown={(e: MouseEvent) => {
                        handleColumnDragStart('modified', e)
                    }}
                ></span>
            </span>
        </div>

        <!-- Results list -->
        <div class="results-container" bind:this={resultsContainer} role="listbox" aria-label="Search results">
            {#if !isIndexAvailable}
                <div class="index-unavailable">
                    <p class="unavailable-message">
                        Drive index not ready. Search is available after the initial scan completes.
                    </p>
                    {#if scanning}
                        <p class="unavailable-progress">
                            Scan in progress{entriesScanned > 0
                                ? ` (${formatEntryCount(entriesScanned)} entries)`
                                : ''}...
                        </p>
                    {/if}
                </div>
            {:else if !isIndexReady && hasSearched}
                <div class="loading-state">
                    <span class="loading-pulse" aria-hidden="true"></span>
                    Loading drive index...
                </div>
            {:else if isSearching && results.length === 0}
                <div class="loading-state">
                    <span class="loading-pulse" aria-hidden="true"></span>
                    Searching...
                </div>
            {:else if results.length === 0 && hasSearched && !isSearching && (namePattern.trim() || sizeFilter !== 'any' || dateFilter !== 'any')}
                <div class="no-results">No files found</div>
            {:else}
                {#each results as entry, index (entry.path)}
                    <div
                        class="result-row"
                        class:is-under-cursor={index === cursorIndex}
                        class:is-hovered={hoveredIndex === index && index !== cursorIndex}
                        style:grid-template-columns={gridTemplate}
                        onclick={() => {
                            handleResultClick(index)
                        }}
                        onmouseenter={() => {
                            hoveredIndex = index
                        }}
                        onmouseleave={() => {
                            hoveredIndex = null
                        }}
                        role="option"
                        tabindex="-1"
                        aria-selected={index === cursorIndex}
                    >
                        <span class="result-icon">
                            {#if getIconUrl(entry.iconId)}
                                <img class="icon-img" src={getIconUrl(entry.iconId)} alt="" width="16" height="16" />
                            {:else if entry.isDirectory}
                                <span class="icon-emoji">📁</span>
                            {:else}
                                <span class="icon-emoji">📄</span>
                            {/if}
                        </span>
                        <span class="result-name" title={entry.name}>
                            {entry.name}
                        </span>
                        <span class="result-path" title={entry.parentPath}>
                            {entry.parentPath}
                        </span>
                        <span class="result-size">
                            {formatSize(entry.size)}
                        </span>
                        <span class="result-modified">
                            {formatDate(entry.modifiedAt)}
                        </span>
                    </div>
                {/each}
            {/if}
        </div>

        <!-- Status bar -->
        <div class="status-bar" aria-live="polite">
            <span class="status-text">{getStatusText()}</span>
        </div>
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
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        width: 900px;
        display: flex;
        flex-direction: column;
        box-shadow: var(--shadow-lg);
        overflow: hidden;
    }

    /* Input rows */
    .input-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-sm) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        background: var(--color-bg-primary);
        gap: var(--spacing-sm);
    }

    /* AI prompt row styling — subtle left accent border */
    .ai-prompt-row {
        border-left: 2px solid var(--color-accent);
        background: var(--color-bg-secondary);
        animation: slide-down 150ms ease-out;
    }

    @keyframes slide-down {
        from {
            max-height: 0;
            opacity: 0;
            padding-top: 0;
            padding-bottom: 0;
        }

        to {
            max-height: 60px;
            opacity: 1;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .ai-prompt-row {
            animation: none;
        }
    }

    .row-label {
        flex-shrink: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-weight: 500;
        user-select: none;
    }

    .ai-label {
        color: var(--color-accent);
    }

    .search-icon {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .name-input {
        flex: 1;
        font-size: var(--font-size-md);
        border: 1px solid transparent;
        background: transparent;
        color: var(--color-text-primary);
        outline: none;
        min-width: 0;
    }

    .name-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .name-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .name-input.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    /* Shared button style for Search and Ask AI */
    .action-button {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .action-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .action-button:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .action-button.ai-active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .pattern-type-toggle {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        font-size: var(--font-size-xs);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        font-family: var(--font-mono);
        min-width: 40px;
        text-align: center;
    }

    .pattern-type-toggle:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .pattern-type-toggle:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    .pattern-type-toggle.active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .pattern-type-toggle.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    /* Scope info button and tooltip */
    .scope-info-wrapper {
        position: relative;
        flex-shrink: 0;
    }

    .scope-info-button {
        width: 18px;
        height: 18px;
        border-radius: var(--radius-full);
        border: 1px solid var(--color-border);
        font-size: var(--font-size-xs);
        font-style: italic;
        font-family: var(--font-system);
        color: var(--color-text-tertiary);
        display: flex;
        align-items: center;
        justify-content: center;
        line-height: 1;
    }

    .scope-info-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .scope-info-button:not(:disabled):hover {
        border-color: var(--color-border-strong);
        color: var(--color-text-secondary);
    }

    /* Preflight row — read-only status between AI prompt and pattern row */
    .preflight-row {
        padding: var(--spacing-xs) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        border-left: 2px solid color-mix(in srgb, var(--color-accent) 60%, transparent);
        background: var(--color-bg-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .preflight-label {
        color: var(--color-text-tertiary);
    }

    .preflight-content {
        color: var(--color-text-primary);
    }

    .preflight-placeholder {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .caveat-row {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .ai-status {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .ai-error {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Filter row */
    .filter-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-xs) var(--spacing-md);
        gap: var(--spacing-lg);
        border-bottom: 1px solid var(--color-border-strong);
        flex-wrap: wrap;
    }

    .filter-group {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .filter-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .filter-select {
        font-size: var(--font-size-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .filter-select:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .filter-input {
        font-size: var(--font-size-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .filter-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .size-input {
        width: 70px;
    }

    .date-input {
        width: 130px;
    }

    .unit-select {
        width: auto;
    }

    .filter-separator {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .filter-group.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        padding: var(--spacing-xxs) var(--spacing-xs);
        transition: background 1.5s ease-out;
    }

    /* Column headers */
    .column-header {
        display: grid;
        gap: var(--spacing-xs);
        align-items: center;
        padding: var(--spacing-xxs) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        user-select: none;
    }

    .col-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        position: relative;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-label.col-icon {
        width: 24px;
    }

    .col-label.col-right {
        text-align: right;
    }

    .col-resize-handle {
        position: absolute;
        top: 0;
        right: -2px;
        width: 5px;
        height: 100%;
        cursor: col-resize;
    }

    .col-resize-handle:hover {
        background: var(--color-border-strong);
    }

    /* Results list */
    .results-container {
        overflow-y: auto;
        max-height: 400px;
    }

    .loading-state {
        padding: var(--spacing-lg);
        text-align: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
    }

    .loading-pulse {
        display: inline-block;
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: var(--color-text-tertiary);
        animation: pulse 1.2s ease-in-out infinite;
    }

    @keyframes pulse {
        0%,
        100% {
            opacity: 0.3;
        }

        50% {
            opacity: 1;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .loading-pulse {
            animation: none;
            opacity: 0.6;
        }
    }

    .no-results {
        padding: var(--spacing-lg);
        text-align: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
    }

    .result-row {
        display: grid;
        gap: var(--spacing-xs);
        align-items: center;
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .result-row.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    .result-row.is-hovered {
        background: var(--color-tint-hover);
    }

    .result-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        font-size: var(--font-size-md);
        line-height: 1;
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-md);
        line-height: 1;
    }

    .result-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-weight: 500;
    }

    .result-path {
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .result-size {
        color: var(--color-text-secondary);
        white-space: nowrap;
        text-align: right;
    }

    .result-modified {
        color: var(--color-text-tertiary);
        white-space: nowrap;
        text-align: right;
    }

    /* Status bar */
    .status-bar {
        padding: var(--spacing-xs) var(--spacing-md);
        border-top: 1px solid var(--color-border-strong);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .status-text {
        user-select: none;
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

    /* Index unavailable message */
    .index-unavailable {
        padding: var(--spacing-lg) var(--spacing-md);
        text-align: center;
    }

    .unavailable-message {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
        margin: 0;
    }

    .unavailable-progress {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: var(--spacing-xs) 0 0;
    }

    /* Light mode handled by --color-tint-hover token */
</style>
