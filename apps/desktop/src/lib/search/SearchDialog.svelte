<script lang="ts">
    /**
     * SearchDialog - Whole-drive file search overlay.
     *
     * Follows the command palette pattern (custom overlay, not ModalDialog).
     * Searches the in-memory index by filename (wildcards), size, and date.
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
        onSearchIndexReady,
        formatBytes,
    } from '$lib/tauri-commands'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
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
        getIsAiMode,
        setIsAiMode,
        getAiStatus,
        setAiStatus,
        buildSearchQuery,
        resetSearchState,
        type SizeFilter,
        type DateFilter,
        type SizeUnit,
    } from './search-state.svelte'

    interface Props {
        /** Called when user selects a result: receives the full path */
        onNavigate: (path: string) => void
        /** Called when dialog is closed */
        onClose: () => void
    }

    const { onNavigate, onClose }: Props = $props()

    let inputElement: HTMLInputElement | undefined = $state()
    let dialogElement: HTMLDivElement | undefined = $state()
    let resultsContainer: HTMLDivElement | undefined = $state()
    let hoveredIndex = $state<number | null>(null)
    let debounceTimer: ReturnType<typeof setTimeout> | undefined
    let unlistenReady: UnlistenFn | undefined

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
    const isAiMode = $derived(getIsAiMode())
    const aiStatus = $derived(getAiStatus())
    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())

    const showAiButton = $derived(getSetting('ai.provider') !== 'off' && isIndexAvailable)
    /** Whether inputs/filters should be disabled (index not available or still scanning with no index). */
    const inputsDisabled = $derived(!isIndexAvailable)

    let aiError = $state('')
    let highlightedFields = new SvelteSet<string>()

    // Subscribe to icon cache version for reactivity
    const _iconVersion = $derived($iconCacheVersion)

    function getIconUrl(iconId: string): string | undefined {
        void _iconVersion
        return getCachedIcon(iconId)
    }

    onMount(async () => {
        notifyDialogOpened('search').catch(() => {})

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

        await tick()
        inputElement?.focus()
    })

    onDestroy(() => {
        notifyDialogClosed('search').catch(() => {})
        releaseSearchIndex().catch(() => {})
        unlistenReady?.()
        if (debounceTimer) clearTimeout(debounceTimer)
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
        if (!getIsIndexReady()) return

        setIsSearching(true)
        try {
            const query = buildSearchQuery()
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

    function toggleAiMode(): void {
        const newMode = !getIsAiMode()
        setIsAiMode(newMode)
        aiError = ''
        setAiStatus('')
        if (!newMode) {
            // Switching back to manual mode — clear AI status
            setNamePattern('')
        }
        inputElement?.focus()
    }

    /** Applies AI-returned size filters to the UI state. Returns true if any were applied. */
    function applySizeFilters(display: { minSize?: number; maxSize?: number }): boolean {
        if (display.minSize === undefined && display.maxSize === undefined) return false
        if (display.minSize !== undefined && display.maxSize !== undefined) {
            setSizeFilter('between')
            const { value: minVal, unit: minUnit } = bytesToDisplaySize(display.minSize)
            setSizeValue(minVal)
            setSizeUnit(minUnit)
            const { value: maxVal, unit: maxUnit } = bytesToDisplaySize(display.maxSize)
            setSizeValueMax(maxVal)
            setSizeUnitMax(maxUnit)
        } else if (display.minSize !== undefined) {
            setSizeFilter('gte')
            const { value, unit } = bytesToDisplaySize(display.minSize)
            setSizeValue(value)
            setSizeUnit(unit)
        } else if (display.maxSize !== undefined) {
            setSizeFilter('lte')
            const { value, unit } = bytesToDisplaySize(display.maxSize)
            setSizeValue(value)
            setSizeUnit(unit)
        }
        return true
    }

    /** Applies AI-returned date filters to the UI state. Returns true if any were applied. */
    function applyDateFilters(display: { modifiedAfter?: string; modifiedBefore?: string }): boolean {
        if (display.modifiedAfter === undefined && display.modifiedBefore === undefined) return false
        if (display.modifiedAfter !== undefined && display.modifiedBefore !== undefined) {
            setDateFilter('between')
            setDateValue(display.modifiedAfter)
            setDateValueMax(display.modifiedBefore)
        } else if (display.modifiedAfter !== undefined) {
            setDateFilter('after')
            setDateValue(display.modifiedAfter)
        } else if (display.modifiedBefore !== undefined) {
            setDateFilter('before')
            setDateValue(display.modifiedBefore)
        }
        return true
    }

    async function executeAiSearch(): Promise<void> {
        const query = getNamePattern().trim()
        if (!query) return

        aiError = ''
        const provider = getSetting('ai.provider')
        const providerLabel = provider === 'local' ? 'local LLM' : provider
        setAiStatus(`Calling ${providerLabel}...`)

        try {
            const result = await translateSearchQuery(query)
            setAiStatus('Building query...')

            // Populate filter fields from AI response
            const changed = new SvelteSet<string>()

            if (result.display.namePattern !== undefined) {
                setNamePattern(result.display.namePattern)
                changed.add('name')
            }
            if (applySizeFilters(result.display)) changed.add('size')
            if (applyDateFilters(result.display)) changed.add('date')

            // Brief highlight animation on changed fields
            highlightedFields = changed
            setTimeout(() => {
                highlightedFields = new SvelteSet()
            }, 1500)

            // Now run the actual search
            setAiStatus('Searching...')
            if (getIsIndexReady()) {
                const searchQuery = buildSearchQuery()
                const searchResult = await searchFiles(searchQuery)
                setResults(searchResult.entries)
                setTotalCount(searchResult.totalCount)
                setCursorIndex(0)
            }

            setAiStatus('')
        } catch (e: unknown) {
            aiError = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
            setAiStatus('')
        }
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

    function handleNameInput(e: Event): void {
        const target = e.target as HTMLInputElement
        setNamePattern(target.value)
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

    /** Handles Enter key in the search dialog. */
    function handleEnterKey(): void {
        if (isAiMode) {
            void executeAiSearch()
        } else if (cursorIndex < results.length) {
            onNavigate(results[cursorIndex].path)
        } else {
            // Bypass debounce — run search immediately
            void executeSearch()
        }
    }

    function handleKeyDown(e: KeyboardEvent): void {
        e.stopPropagation()

        if (handleTabFocusTrap(e)) return

        // ⌘L toggles AI mode
        if (e.key === 'l' && e.metaKey && !e.shiftKey && !e.altKey) {
            e.preventDefault()
            if (showAiButton) toggleAiMode()
            return
        }

        switch (e.key) {
            case 'Escape':
                e.preventDefault()
                onClose()
                break
            case 'ArrowDown':
                e.preventDefault()
                setCursorIndex(Math.min(getCursorIndex() + 1, results.length - 1))
                hoveredIndex = null
                scrollCursorIntoView()
                break
            case 'ArrowUp':
                e.preventDefault()
                setCursorIndex(Math.max(getCursorIndex() - 1, 0))
                hoveredIndex = null
                scrollCursorIntoView()
                break
            case 'Enter':
                e.preventDefault()
                handleEnterKey()
                break
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

    function formatSize(bytes: number | undefined): string {
        if (bytes === undefined) return ''
        return formatBytes(bytes)
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
        if (!isIndexReady) {
            if (indexEntryCount > 0) {
                return `Loading index (${formatEntryCount(indexEntryCount)} entries)...`
            }
            return 'Loading index...'
        }
        if (isSearching) return 'Searching...'
        if (totalCount === 0 && !namePattern.trim() && sizeFilter === 'any' && dateFilter === 'any') {
            return `Ready (${formatEntryCount(indexEntryCount)} entries indexed)`
        }
        if (totalCount === 0) return 'No results'
        return `${String(results.length)} of ${totalCount.toLocaleString()} results`
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
        <!-- Input row -->
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
                bind:this={inputElement}
                type="text"
                class="name-input"
                placeholder={isAiMode
                    ? "Describe what you're looking for..."
                    : 'Filename pattern (use * and ? as wildcards)'}
                value={namePattern}
                oninput={handleNameInput}
                disabled={inputsDisabled}
                aria-label={isAiMode ? 'Natural language search query' : 'Filename pattern'}
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
            />
            {#if showAiButton}
                <button
                    class="ai-button"
                    class:ai-active={isAiMode}
                    onclick={toggleAiMode}
                    title={isAiMode ? 'Switch to manual mode' : 'Ask AI (⌘L)'}
                >
                    {isAiMode ? 'Manual' : 'Ask AI'}
                </button>
            {/if}
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
            {:else if results.length === 0 && isIndexReady && !isSearching && (namePattern.trim() || sizeFilter !== 'any' || dateFilter !== 'any')}
                <div class="no-results">No files found</div>
            {:else}
                {#each results as entry, index (entry.path)}
                    <div
                        class="result-row"
                        class:is-under-cursor={index === cursorIndex}
                        class:is-hovered={hoveredIndex === index && index !== cursorIndex}
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
        background: rgba(0, 0, 0, 0.5);
        backdrop-filter: blur(2px);
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
        width: 680px;
        display: flex;
        flex-direction: column;
        box-shadow: var(--shadow-lg);
        overflow: hidden;
    }

    /* Input row */
    .input-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-sm) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        background: var(--color-bg-primary);
        gap: var(--spacing-sm);
    }

    .search-icon {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .name-input {
        flex: 1;
        font-size: var(--font-size-md);
        border: none;
        background: transparent;
        color: var(--color-text-primary);
        outline: none;
        min-width: 0;
    }

    .name-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .ai-button {
        flex-shrink: 0;
        padding: 2px var(--spacing-sm);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        cursor: pointer;
        white-space: nowrap;
    }

    .ai-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .ai-button.ai-active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
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
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .filter-input {
        font-size: var(--font-size-sm);
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
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
        padding: 2px 4px;
        transition: background 1.5s ease-out;
    }

    /* Results list */
    .results-container {
        overflow-y: auto;
        max-height: 400px;
    }

    .no-results {
        padding: var(--spacing-lg);
        text-align: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
    }

    .result-row {
        display: grid;
        grid-template-columns: 20px 1fr auto auto;
        gap: var(--spacing-xs);
        align-items: center;
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        cursor: pointer;
    }

    .result-row.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    .result-row.is-hovered {
        background: rgba(255, 255, 255, 0.06);
    }

    .result-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        font-size: 14px;
        line-height: 1;
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: 14px;
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
        max-width: 250px;
        text-align: right;
    }

    .result-size {
        color: var(--color-text-secondary);
        white-space: nowrap;
        min-width: 60px;
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

    /* Light mode support */
    @media (prefers-color-scheme: light) {
        .result-row.is-hovered {
            background: rgba(0, 0, 0, 0.04);
        }
    }
</style>
