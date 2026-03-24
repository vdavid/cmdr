<script lang="ts">
    /**
     * SearchResults — Column headers + results list + all states + status bar.
     *
     * Displays search results with resizable columns, handles all result states
     * (unavailable, loading, searching, empty, populated), and shows a status bar.
     */
    import { tick } from 'svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { formatBytes } from '$lib/tauri-commands'
    import type { SearchResultEntry } from '$lib/tauri-commands'

    interface Props {
        results: SearchResultEntry[]
        cursorIndex: number
        hoveredIndex: number | null
        isIndexAvailable: boolean
        isIndexReady: boolean
        isSearching: boolean
        hasSearched: boolean
        namePattern: string
        sizeFilter: string
        dateFilter: string
        scanning: boolean
        entriesScanned: number
        totalCount: number
        indexEntryCount: number
        gridTemplate: string
        iconCacheVersion: number
        onResultClick: (index: number) => void
        onColumnDragStart: (col: string, e: MouseEvent) => void
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        results,
        cursorIndex,
        hoveredIndex = $bindable(), // eslint-disable-line @typescript-eslint/no-useless-default-assignment -- $bindable() marker, not a default
        isIndexAvailable,
        isIndexReady,
        isSearching,
        hasSearched,
        namePattern,
        sizeFilter,
        dateFilter,
        scanning,
        entriesScanned,
        totalCount,
        indexEntryCount,
        gridTemplate,
        iconCacheVersion: _iconVersionProp,
        onResultClick,
        onColumnDragStart,
    }: Props = $props()
    /* eslint-enable prefer-const */

    let resultsContainer: HTMLDivElement | undefined = $state()

    // Subscribe to icon cache version for reactivity
    const _iconVersion = $derived($iconCacheVersion)

    function getIconUrl(iconId: string): string | undefined {
        void _iconVersion
        void _iconVersionProp
        return getCachedIcon(iconId)
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

    /** Scrolls the cursor row into view. Called by the parent after cursor changes. */
    export function scrollCursorIntoView(): void {
        void tick().then(() => {
            const cursor = resultsContainer?.querySelector('.result-row.is-under-cursor')
            cursor?.scrollIntoView({ block: 'nearest' })
        })
    }
</script>

<!-- Column headers -->
<div class="column-header" style:grid-template-columns={gridTemplate}>
    <span class="col-label col-icon"></span>
    <span class="col-label">
        Name
        <span
            class="col-resize-handle"
            role="separator"
            onmousedown={(e: MouseEvent) => {
                onColumnDragStart('name', e)
            }}
        ></span>
    </span>
    <span class="col-label">
        Path
        <span
            class="col-resize-handle"
            role="separator"
            onmousedown={(e: MouseEvent) => {
                onColumnDragStart('path', e)
            }}
        ></span>
    </span>
    <span class="col-label col-right">
        Size
        <span
            class="col-resize-handle"
            role="separator"
            onmousedown={(e: MouseEvent) => {
                onColumnDragStart('size', e)
            }}
        ></span>
    </span>
    <span class="col-label col-right">
        Modified
        <span
            class="col-resize-handle"
            role="separator"
            onmousedown={(e: MouseEvent) => {
                onColumnDragStart('modified', e)
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
                    Scan in progress{entriesScanned > 0 ? ` (${formatEntryCount(entriesScanned)} entries)` : ''}...
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
                    onResultClick(index)
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

<style>
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
</style>
