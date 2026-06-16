<script lang="ts">
    /**
     * SearchResults: Column headers + results list + all states + status bar.
     *
     * The table uses CSS grid with the Path column as the single flex track (`1fr`).
     * Name has a measured max width and mid-truncates
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
    import { formatInteger } from '$lib/intl/number-format'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import Size from '$lib/ui/Size.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import EmptyState from './EmptyState.svelte'
    import PathPills from './PathPills.svelte'
    import SearchRowMenu from './SearchRowMenu.svelte'
    import type { SearchMode } from './query-filter-state.svelte'

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
        /**
         * Whether to render the Path column (header + cell). Search renders this `true` so the
         * cross-folder results table can show each row's parent folder. Selection renders it
         * `false` because Selection operates on a single folder; the path column would always
         * be empty. Defaults to `true` for backward compatibility with existing Search usage.
         */
        showPathColumn?: boolean
        onResultClick: (index: number) => void
        /**
         * Called when the user moves the mouse over a row. The dialog uses this to
         * move the accent-colored cursor so mouse + keyboard share one cursor.
         */
        onHover: (index: number) => void
        /** Called when the user clicks an example chip in the empty state. */
        onPickExample: (chip: { mode: SearchMode; query: string }) => void
        /**
         * Consumer-provided example chips for the empty state. Forwarded to
         * `EmptyState`. When omitted, EmptyState renders Search-flavoured defaults.
         * Selection passes its own set ("all image files", etc.) here.
         */
        emptyExamples?: Array<{ label: string; mode: SearchMode; query: string }>
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
        showPathColumn = true,
        onResultClick,
        onHover,
        onPickExample,
        emptyExamples,
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
            // D3: status bar stays empty while the content area shows the spinner.
            // D4: status bar stays empty while the content area shows the criteria list.
            // Both states surface their info in the content; no duplication here.
            if (isSearching) return ''
            if (!hasSearched || (!query.trim() && sizeFilter === 'any' && dateFilter === 'any')) {
                return `Index ready (${formatEntryCount(indexEntryCount)} entries)`
            }
            if (totalCount === 0) return ''
            return `${String(results.length)} of ${formatInteger(totalCount)} results`
        }
        // Index loading: the content area shows the "Loading drive index..." spinner,
        // so the status bar stays empty to avoid duplication. (R4: same rule as D3 / D4
        // for the searching / no-results states — content is the source of truth.)
        return ''
    }

    /**
     * Per D4: the no-results content area lists the active criteria as a bulleted
     * list under "No files match these criteria:". Pure derivation from the
     * already-passed-in props so it stays trivially testable.
     */
    function buildCriteria(): string[] {
        const out: string[] = []
        const q = query.trim()
        if (q) out.push(`Query: ${q}`)
        if (sizeFilter !== 'any') out.push(`Size filter active`)
        if (dateFilter !== 'any') out.push(`Modified filter active`)
        return out
    }

    // True only when the `{:else}` branch below actually renders option rows. `role="listbox"`
    // requires `option` children, so it must NOT be set during the searching / loading / empty
    // states (which replace the rows with a spinner or message) even when `results` still holds a
    // stale set. Gating on `results.length > 0` alone tripped axe's `aria-required-children` when
    // a reopened dialog re-ran (spinner showing, persisted results still in `results`).
    const showingRows = $derived(isIndexAvailable && isIndexReady && !isSearching && results.length > 0)

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
<div class="column-header" class:no-path={!showPathColumn}>
    <span class="col-label col-icon" aria-hidden="true"></span>
    <span class="col-label">Name</span>
    {#if showPathColumn}<span class="col-label">Path</span>{/if}
    <span class="col-label col-right">Size</span>
    <span class="col-label col-right">Modified</span>
    <span class="col-label col-actions">Actions</span>
</div>

<!-- Results list. `role="listbox"` only applies when option rows are rendered; empty/loading/
     unavailable states are bare text containers so axe doesn't flag aria-required-children. -->
<div
    class="results-container"
    bind:this={resultsContainer}
    role={showingRows ? 'listbox' : undefined}
    aria-label={showingRows ? 'Search results' : undefined}
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
            <Spinner size="md" />
            <div class="loading-label">Loading drive index...</div>
        </div>
    {:else if isSearching}
        <!-- D1/D2: full result list area is replaced by the standard spinner +
             "Searching..." label. No rows render while the fetch is in-flight,
             since the previous result set is now stale relative to the new
             query/filter state. -->
        <div class="loading-state">
            <Spinner size="md" />
            <div class="loading-label">Searching...</div>
        </div>
    {:else if results.length === 0 && hasSearched && !isSearching && (query.trim() || sizeFilter !== 'any' || dateFilter !== 'any')}
        <!-- D4: structured no-results state. Heading + bulleted criteria list. -->
        <div class="no-results">
            <p class="no-results-heading">No files match these criteria:</p>
            <ul class="no-results-criteria">
                {#each buildCriteria() as item (item)}
                    <li>{item}</li>
                {/each}
            </ul>
        </div>
    {:else if !hasSearched && !query.trim() && isIndexReady && sizeFilter === 'any' && dateFilter === 'any'}
        <EmptyState {aiEnabled} {indexEntryCount} examples={emptyExamples} onPick={onPickExample} />
    {:else}
        {#each results as entry, index (entry.path)}
            <div
                class="result-row"
                class:no-path={!showPathColumn}
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
                {#if showPathColumn}
                    <span class="result-path">
                        <PathPills path={entry.parentPath} onPick={onPickPath} />
                    </span>
                {/if}
                <span class="result-size">
                    <Size bytes={entry.size} />
                </span>
                <span class="result-modified">
                    <DateLabel modifiedAt={entry.modifiedAt} />
                </span>
                <!-- Actions column: per-row `…` menu. Always visible on every row
                     (discoverability beats visual quiet here). Header label aligns above. -->
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
       hard ceiling so very long names don't squeeze the path column to nothing.
       Size / Modified / Actions are pinned to fixed `ch` widths so the header
       row and data rows resolve the SAME widths, aligning the column boundary
       perfectly across rows. Don't switch them back to `max-content`: the
       header row's "Size" / "Modified" labels are narrower than typical row
       data (`1.2 MB`, `Jan 12, 2026`), so each row would resolve its own
       widths from its own data and the header text would drift left of the
       row content. */
    .column-header,
    .result-row {
        display: grid;
        grid-template-columns:
            24px /* icon */
            minmax(80px, 22ch) /* name (mid-truncates) */
            minmax(120px, 1fr) /* path (flex) */
            10ch /* size (right-aligned, fits "999.9 MB") */
            16ch /* modified (right-aligned, fits short and long date formats) */
            32px; /* actions (… button footprint) */

        column-gap: var(--spacing-md);
        align-items: center;
    }

    /* Selection (or any consumer that hides the Path column) drops the path track
       entirely; name absorbs the freed horizontal space via 1fr. */
    .column-header.no-path,
    .result-row.no-path {
        grid-template-columns:
            24px
            minmax(80px, 1fr)
            10ch
            16ch
            32px;
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
        font-size: var(--font-size-md);
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

    /* Vertical stack so the spinner sits above the label, matching the rest of
       the app's loading affordance (LoadingIcon). */
    .loading-state {
        padding: var(--spacing-xl) var(--spacing-lg);
        text-align: center;
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
    }

    .loading-label {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    /* No-results state: heading + bulleted criteria list. Compact left-aligned
       block centered horizontally so the bullets line up readably. */
    .no-results {
        padding: var(--spacing-lg);
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .no-results-heading {
        margin: 0;
        color: var(--color-text-primary);
    }

    .no-results-criteria {
        margin: 0;
        padding: 0 0 0 1.25em;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        text-align: left;
    }

    .no-results-criteria li {
        margin: 0;
    }

    .result-row {
        /* Vertical padding sits at --spacing-xxs (~4 px) instead of --spacing-xs
           (~8 px) to keep the row compact at the dialog's --font-size-md type.
           All cells vertically center via the grid's `align-items: center` rule above,
           so the look stays clean with the tighter padding. Rows aren't virtualized
           (search caps at 30, Selection lists one folder), so the height is content-
           driven: no row-height constant to keep in sync with the font. */
        padding: var(--spacing-xxs) var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    /* Single cursor: mouse hover and keyboard arrows both write to `cursorIndex`
       (see `onHover` in the row's `onmouseenter`), so there's no separate
       `.is-hovered` background. The accent-colored cursor follows whichever
       input the user reaches for (volume-switcher pattern). */
    .result-row.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    /* Under the cursor the muted columns (path / size / modified) read at full
       `--color-text-primary`: the tertiary / secondary tokens drop below WCAG AA
       on the lightest accent tints of the cursor bg (verified by the contrast
       checker, `scripts/check-a11y-contrast/query_dialog_states.go`). Full-contrast
       text on the active row is also the expected "this row is focused" read. */
    .result-row.is-under-cursor .result-path,
    .result-row.is-under-cursor .result-size,
    .result-row.is-under-cursor .result-modified {
        color: var(--color-text-primary);
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

    /* Actions column. The `…` button is always visible on every row (no
       hover-only fade) — discoverability matters more than visual quiet here. */
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
        font-size: var(--font-size-md);
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
        font-size: var(--font-size-md);
        margin: var(--spacing-xs) 0 0;
    }
</style>
