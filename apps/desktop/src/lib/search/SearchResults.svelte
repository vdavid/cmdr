<script lang="ts">
    /**
     * SearchResults: Column headers + results list + all states + status bar.
     *
     * Per the search-fixup brief, the table uses CSS grid with the Path column as the
     * single flex track (`1fr`). Name has a measured max width and mid-truncates
     * (`useShortenMiddle`); Path renders via `PathPills` with overflow-aware collapse;
     * Size and Modified shrink-wrap to their content and sit comfortably apart (we
     * give them a generous gap via the grid `column-gap` declaration). The Actions
     * column holds the per-row `…` menu and the matching header label.
     *
     * Cursor model (single cursor): both mouse hover and keyboard arrows move the
     * same accent-colored cursor (`cursorIndex`). There is NO separate "hovered"
     * background — hovering a row writes to `cursorIndex` via `onHover`. The cursor
     * loops top<->bottom on arrow nav (handled by the parent dialog). This mirrors
     * the volume switcher's hover-syncs-cursor pattern.
     */
    import { tick } from 'svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import Size from '$lib/ui/Size.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import EmptyState from './EmptyState.svelte'
    import PathPills from './PathPills.svelte'
    import SearchRowMenu from './SearchRowMenu.svelte'
    import type { SearchMode } from './search-state.svelte'

    interface Props {
        results: SearchResultEntry[]
        cursorIndex: number
        isIndexAvailable: boolean
        isIndexReady: boolean
        isSearching: boolean
        hasSearched: boolean
        /** Current query text. Used to differentiate "no query yet" from "0 results found". */
        query: string
        sizeFilter: string
        dateFilter: string
        scanning: boolean
        entriesScanned: number
        totalCount: number
        indexEntryCount: number
        iconCacheVersion: number
        /** True when AI mode is available (provider on + index ready). Drives the empty-state chip set. */
        aiEnabled: boolean
        onResultClick: (index: number) => void
        /**
         * Called when the user moves the mouse over a row. The dialog uses this to
         * move the accent-colored cursor so mouse + keyboard share one cursor.
         */
        onHover: (index: number) => void
        /** Called when the user clicks an example chip in the empty state. */
        onPickExample: (chip: { mode: SearchMode; query: string }) => void
        /**
         * Called when the user clicks a path-pill ancestor segment. Parent navigates the
         * active pane to `ancestorPath` and closes the dialog (per §3.8).
         */
        onPickPath: (ancestorPath: string) => void
        /**
         * Called when the user opens the row context menu (right-click on a row, or click
         * on the row's `…` button). Parent routes to the native context-menu factory.
         */
        onRowMenu: (entry: SearchResultEntry) => void
    }

    const {
        results,
        cursorIndex,
        isIndexAvailable,
        isIndexReady,
        isSearching,
        hasSearched,
        query,
        sizeFilter,
        dateFilter,
        scanning,
        entriesScanned,
        totalCount,
        indexEntryCount,
        iconCacheVersion: _iconVersionProp,
        aiEnabled,
        onResultClick,
        onHover,
        onPickExample,
        onPickPath,
        onRowMenu,
    }: Props = $props()

    let resultsContainer: HTMLDivElement | undefined = $state()

    // Subscribe to icon cache version for reactivity
    const _iconVersion = $derived($iconCacheVersion)

    function getIconUrl(iconId: string): string | undefined {
        void _iconVersion
        void _iconVersionProp
        return getCachedIcon(iconId)
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
            if (!hasSearched || (!query.trim() && sizeFilter === 'any' && dateFilter === 'any')) {
                return `Index ready (${formatEntryCount(indexEntryCount)} entries)`
            }
            if (totalCount === 0) return 'No results'
            return `${String(results.length)} of ${totalCount.toLocaleString()} results`
        }
        // Index loading: only show status if user has triggered a search
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

<!-- Column headers. Path is the flex column (1fr); Size + Modified shrink-wrap.
     The Actions column on the right matches the row's `…` button slot. Header
     cells use the same grid template as the rows so columns line up. -->
<div class="column-header">
    <span class="col-label col-icon" aria-hidden="true"></span>
    <span class="col-label">Name</span>
    <span class="col-label">Path</span>
    <span class="col-label col-right">Size</span>
    <span class="col-label col-right">Modified</span>
    <span class="col-label col-actions">Actions</span>
</div>

<!-- Results list. `role="listbox"` only applies when option rows are rendered; empty/loading/
     unavailable states are bare text containers so axe doesn't flag aria-required-children. -->
<div
    class="results-container"
    bind:this={resultsContainer}
    role={results.length > 0 ? 'listbox' : undefined}
    aria-label={results.length > 0 ? 'Search results' : undefined}
>
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
    {:else if results.length === 0 && hasSearched && !isSearching && (query.trim() || sizeFilter !== 'any' || dateFilter !== 'any')}
        <div class="no-results">No files found</div>
    {:else if !hasSearched && !query.trim() && isIndexReady && sizeFilter === 'any' && dateFilter === 'any'}
        <EmptyState {aiEnabled} {indexEntryCount} onPick={onPickExample} />
    {:else}
        {#each results as entry, index (entry.path)}
            <div
                class="result-row"
                class:is-under-cursor={index === cursorIndex}
                onclick={() => {
                    onResultClick(index)
                }}
                oncontextmenu={(e) => {
                    e.preventDefault()
                    onRowMenu(entry)
                }}
                onmouseenter={() => {
                    onHover(index)
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
                <!-- Mid-truncating name. `useShortenMiddle` measures with pretext
                     and snaps to '.' so the extension stays visible. Tooltip
                     shows the full name only when truncation actually happened. -->
                <span
                    class="result-name"
                    use:useShortenMiddle={{
                        text: entry.name,
                        preferBreakAt: '.',
                        startRatio: 0.7,
                        tooltipWhenTruncated: true,
                    }}
                ></span>
                <span class="result-path">
                    <PathPills path={entry.parentPath} onPick={onPickPath} />
                </span>
                <span class="result-size">
                    <Size bytes={entry.size} />
                </span>
                <span class="result-modified">
                    <DateLabel modifiedAt={entry.modifiedAt} />
                </span>
                <!-- Actions column: per-row `…` menu. Always visible on every row
                     (search-fixup brief item 2). Header label aligns above. -->
                <span class="result-actions">
                    <SearchRowMenu
                        onOpen={() => {
                            onRowMenu(entry)
                        }}
                    />
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
    /* Shared grid template. Path (1fr) absorbs the remaining width; Name has a
       hard ceiling so very long names don't squeeze the path column to nothing;
       Size + Modified shrink-wrap; Actions matches the `…` button footprint.
       The `column-gap` keeps Size and Modified visibly apart. */
    .column-header,
    .result-row {
        display: grid;
        grid-template-columns:
            24px /* icon */
            minmax(80px, 22ch) /* name (mid-truncates) */
            minmax(120px, 1fr) /* path (flex) */
            max-content /* size */
            max-content /* modified */
            max-content; /* actions */

        column-gap: var(--spacing-md);
        align-items: center;
    }

    /* Column headers sit on the dialog's secondary surface (matching the FullList header in the
       main pane), with a hairline below to land cleanly onto the results surface. */
    .column-header {
        padding: var(--spacing-xs) var(--spacing-lg);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-subtle);
        user-select: none;
    }

    .col-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
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

    .col-label.col-actions {
        text-align: right;
        min-width: 28px;
    }

    /* Results list */
    .results-container {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
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
        padding: var(--spacing-xs) var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    /* Single cursor: mouse hover and keyboard arrows both write to `cursorIndex`
       (see `onHover` in the row's `onmouseenter`), so there's no separate
       `.is-hovered` background. The accent-colored cursor follows whichever
       input the user reaches for. Per search-fixup-brief item 6. */
    .result-row.is-under-cursor {
        background: var(--color-accent-subtle);
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

    /* Name column: mid-truncation handled by `useShortenMiddle`; we just keep
       overflow hidden and the column track width capped (22ch) so very long
       names don't push Path off the edge. */
    .result-name {
        overflow: hidden;
        white-space: nowrap;
        font-weight: 500;
        min-width: 0;
    }

    .result-path {
        color: var(--color-text-tertiary);
        overflow: hidden;
        min-width: 0;
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

    /* Actions column. Per search-fixup-brief item 2, the `…` button is always
       visible on every row (no hover-only fade). */
    .result-actions {
        display: inline-flex;
        align-items: center;
        justify-content: flex-end;
        min-width: 28px;
    }

    /* Status bar uses the dialog's secondary surface; the surface change against the results
       list is the separator. A hairline border-top reinforces the seam without shouting. */
    .status-bar {
        padding: var(--spacing-xs) var(--spacing-lg);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-subtle);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        flex-shrink: 0;
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
