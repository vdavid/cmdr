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
     *
     * This is the orchestrator: overlay, mount/unmount, keyboard dispatch, search
     * execution, state wiring to child components via props/callbacks.
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
    import { getSetting } from '$lib/settings'
    import { isScanning, getEntriesScanned } from '$lib/indexing'
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
        getCaveat,
        setCaveat,
        buildSearchQuery,
        resetSearchState,
        type PatternType,
    } from './search-state.svelte'
    import AiSearchRow from './AiSearchRow.svelte'
    import SearchInputArea from './SearchInputArea.svelte'
    import SearchResults from './SearchResults.svelte'

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
    let searchResultsComponent: SearchResults | undefined = $state()
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
    const caveatText = $derived(getCaveat())
    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())

    /** Whether AI search is enabled (provider configured and index available). */
    const aiEnabled = $derived(getSetting('ai.provider') !== 'off' && isIndexAvailable)
    /** Whether inputs/filters should be disabled (index not available or still scanning with no index). */
    const inputsDisabled = $derived(!isIndexAvailable)

    let aiError = $state('')
    let highlightedFields = new SvelteSet<string>()
    /** True once the user has triggered at least one search (so we can distinguish "no query yet" from "0 results"). */
    let hasSearched = $state(false)

    // Subscribe to icon cache version for reactivity
    const iconVersion = $derived($iconCacheVersion)

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
        query: {
            includePaths?: string[]
            excludeDirNames?: string[]
            caseSensitive?: boolean
            excludeSystemDirs?: boolean
        }
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
        const query = queryText.trim()
        if (!query) return

        aiError = ''
        setCaveat('')
        const provider = getSetting('ai.provider')
        const providerLabel = provider === 'local' ? 'local LLM' : provider

        // Translate query via LLM
        setAiStatus(`Calling ${providerLabel}...`)
        let translateResult: Awaited<ReturnType<typeof translateSearchQuery>>
        try {
            translateResult = await translateSearchQuery(query)
        } catch (e: unknown) {
            aiError = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
            setAiStatus('')
            return
        }

        applyAiFiltersWithHighlight(translateResult)
        setCaveat(translateResult.caveat ?? '')

        // Search using the translated query directly
        setAiStatus('Searching...')
        await executeSearch()
        setAiStatus('')
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

    function togglePatternType(): void {
        setPatternType(getPatternType() === 'glob' ? 'regex' : 'glob')
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

        {#if aiEnabled}
            <AiSearchRow
                bind:inputElement={aiPromptInputElement}
                {aiPrompt}
                onPromptInput={inputHandler(setAiPrompt, false)}
                onAiSearch={(query: string) => void executeAiSearch(query)}
                disabled={inputsDisabled}
                {caveatText}
                {aiStatus}
                {aiError}
            />
        {/if}

        <SearchInputArea
            bind:patternInputElement
            {namePattern}
            {patternType}
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
            onSearch={() => void executeSearch()}
            onTogglePatternType={togglePatternType}
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
            {namePattern}
            {sizeFilter}
            {dateFilter}
            {scanning}
            {entriesScanned}
            {totalCount}
            {indexEntryCount}
            {gridTemplate}
            iconCacheVersion={iconVersion}
            onResultClick={handleResultClick}
            onColumnDragStart={(col: string, e: MouseEvent) => {
                handleColumnDragStart(col as keyof typeof colWidths, e)
            }}
        />
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
