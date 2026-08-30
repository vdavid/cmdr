<script lang="ts">
    /**
     * SearchResults: Column headers + results list + all states + status bar.
     *
     * The table uses CSS grid with the Path column as the single flex track (`1fr`).
     * Name shrink-wraps to the rows currently on screen (see § "Name column" below) and
     * mid-truncates (`useShortenMiddle`); Path renders via `PathPills` with overflow-aware collapse;
     * Size and Modified shrink-wrap to their content and sit comfortably apart (we
     * give them a generous gap via the grid `column-gap` declaration). There is no
     * actions column: the row's own right-click (`oncontextmenu` → `onRowMenu`) opens
     * the native context menu, which is the whole of what a per-row `…` button offered.
     *
     * Cursor model (single cursor): both mouse hover and keyboard arrows move the
     * same accent-colored cursor (`cursorIndex`). There is NO separate "hovered"
     * background — hovering a row writes to `cursorIndex` via `onHover`. The cursor
     * loops top<->bottom on arrow nav (handled by the parent dialog). This mirrors
     * the volume switcher's hover-syncs-cursor pattern.
     *
     * Name column: a measured pixel width, handed to BOTH grid containers as one inline
     * `grid-template-columns` string so they can't drift. Full contract, and the argument
     * for why the measurement can't oscillate: DETAILS.md § Name column shrink-wrap.
     */
    import { onDestroy, tick } from 'svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import Icon from '$lib/ui/Icon.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import Button from '$lib/ui/Button.svelte'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import Size from '$lib/ui/Size.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import { createPretextMeasure } from '$lib/utils/shorten-middle'
    import { computeNameColumnWidth, visibleRowRange } from './name-column-width'
    import EmptyState from './EmptyState.svelte'
    import PathPills from './PathPills.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        createAnnouncementThrottle,
        livePhaseLabel,
        liveWaitElapsed,
        liveStatusLine,
        liveWalkProgress,
        type LiveRunView,
        type QueryStreamPhase,
    } from './query-stream'
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
        /**
         * Count-only mode (Search-only): the backend returned just a total, no rows. Replaces
         * the results list with a prominent count. Defaults to `false` (Selection never sets it).
         */
        countOnly?: boolean
        /**
         * Turns count-only off and re-runs, wired to the "Show results" button under the
         * count. Optional: Selection has no count-only mode, so it omits this and the
         * button never renders. Flipping the flag alone would leave a stale count on
         * screen (the count-only run returned no rows), so the handler MUST re-run too.
         */
        onShowResults?: () => void
        /**
         * The live run's phase, counters, and end state, or `null` when the last run
         * wasn't a streaming one. Rows arrive over time under a live run, so it also
         * decides whether `isSearching` replaces the list with a spinner (it must not,
         * once there are rows to look at). Search wires it; Selection never streams.
         */
        live?: LiveRunView | null
        /** Stops the running live search. Absent when there's nothing to stop. */
        onStopLive?: () => void
        iconCacheVersion: number
        /** True when AI mode is available (provider on + index ready). Drives the empty-state chip set. */
        aiEnabled: boolean
        /**
         * Whether to render the Path column (header + cell). Search renders it `true` so the
         * cross-folder results table can show each row's parent folder; Selection renders it
         * `false` (one folder, so the column would always be empty). Defaults to `true`.
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
        /** Path-pill click: the parent navigates to `ancestorPath` and closes the dialog. */
        onPickPath: (ancestorPath: string) => void
        /** Right-click on a row. Parent routes to the native context-menu factory. */
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
        countOnly = false,
        onShowResults,
        live = null,
        onStopLive,
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

    /**
     * The status-bar sentence, or `''` when the content area is already saying it. The bar
     * collapses on `''` (see the `.status-bar.is-empty` rule) rather than rendering an empty
     * bordered strip: content is the source of truth, and a bar with nothing in it reads as
     * broken. Every new content-area state has to return `''` here.
     */
    function getStatusText(): string {
        if (!isIndexAvailable) {
            if (scanning && entriesScanned > 0) {
                return tString('queryUi.results.scanningWithCount', { countText: formatEntryCount(entriesScanned) })
            }
            if (scanning) return tString('queryUi.results.scanning')
            return tString('queryUi.results.indexUnavailable')
        }
        if (isIndexReady) {
            // A live run counts in the status bar rather than the content area: its rows
            // are already there and the content area is theirs. `''` means the run
            // covered its ground with nothing left to qualify, so the ordinary result
            // line below is the honest one.
            if (live) {
                const liveLine = liveStatusLine(live, results.length)
                if (liveLine) return liveLine
            }
            // D3: status bar stays empty while the content area shows the spinner.
            // D4: status bar stays empty while the content area shows the criteria list.
            // Both states surface their info in the content; no duplication here.
            if (isSearching) return ''
            // Count-only shows the total prominently in the content area, so the status
            // bar stays empty (same content-is-source-of-truth rule as the spinner states).
            if (showingCount) return ''
            if (!hasSearched || (!query.trim() && sizeFilter === 'any' && dateFilter === 'any')) {
                return tString('queryUi.results.indexReadyStatus', { countText: formatEntryCount(indexEntryCount) })
            }
            if (totalCount === 0) return ''
            return tString('queryUi.results.resultCount', {
                shownText: String(results.length),
                totalText: formatInteger(totalCount),
            })
        }
        // Index loading: the content area shows the "Loading drive index..." spinner,
        // so the status bar stays empty to avoid duplication. (R4: same rule as D3 / D4
        // for the searching / no-results states — content is the source of truth.)
        return ''
    }

    /**
     * Per D4: the no-results content area lists the active criteria as a bulleted list under
     * "No files match these criteria:". Pure derivation from the already-passed-in props.
     */
    function buildCriteria(): string[] {
        const out: string[] = []
        const q = query.trim()
        if (q) out.push(tString('queryUi.results.criteria.query', { query: q }))
        if (sizeFilter !== 'any') out.push(tString('queryUi.results.criteria.size'))
        if (dateFilter !== 'any') out.push(tString('queryUi.results.criteria.modified'))
        return out
    }

    /** A live run is still going, so rows and counts are still arriving. */
    const streaming = $derived(live !== null && live.running)

    /**
     * A live run with nothing to render yet, which is when the content area belongs to
     * the phase spinner: no rows for a list search, and the phases before any counting
     * has happened for a count-only one (its "0 so far" is meaningless until the run
     * has ground of its own to count over).
     */
    const countOnlyHasNothingToShow = (phase: QueryStreamPhase): boolean =>
        phase === 'resolvingCoverage' || phase === 'waitingForAnotherWalk'
    const liveWaiting = $derived(
        live !== null && streaming && (countOnly ? countOnlyHasNothingToShow(live.phase) : results.length === 0),
    )

    // True only when the `{:else}` branch below actually renders option rows. `role="listbox"`
    // requires `option` children, so it must NOT be set during the searching / loading / empty
    // states (which replace the rows with a spinner or message) even when `results` still holds
    // a stale set. Gating on `results.length > 0` alone tripped axe's `aria-required-children`.
    // `isSearching` is TRUE for a live run's whole life, and its rows are the point, so the
    // spinner only owns the area while nothing has arrived (`liveWaiting`).
    const showingRows = $derived(
        isIndexAvailable && isIndexReady && (!isSearching || streaming) && !countOnly && results.length > 0,
    )

    // Count-only shows a bare total instead of rows. Renders once a search has run (including a
    // 0-match run), so an active count-only query that matches nothing reads "0 results", not the
    // no-match criteria list. Before the first run it falls through to the empty state.
    const showingCount = $derived(
        isIndexAvailable &&
            isIndexReady &&
            (!isSearching || streaming) &&
            !liveWaiting &&
            countOnly &&
            hasSearched &&
            (query.trim() !== '' || sizeFilter !== 'any' || dateFilter !== 'any'),
    )

    /**
     * Count-only over a live walk is a LOWER BOUND: the walk is still counting, and a
     * run that ended short stopped counting where it stopped. Either way the exact
     * sentence would be a confident lie, so the "so far" one takes over.
     */
    const countIsProvisional = $derived(live !== null && (live.running || live.incomplete))

    const statusText = $derived(getStatusText())
    /** The walk's own progress, beside the count. Empty unless a walk is what's running. */
    const walkProgress = $derived(live === null ? '' : liveWalkProgress(live))

    // A waiting run reports no count and no folder of its own, so an elapsed reading is
    // the only thing on screen that moves. Ticking is scoped to the waiting phase: no
    // other phase reads `waitNow`, so nothing else re-renders on it.
    let waitNow = $state(Date.now())
    const isWaiting = $derived(live !== null && live.running && live.phase === 'waitingForAnotherWalk')
    $effect(() => {
        if (!isWaiting) return
        waitNow = Date.now()
        const timer = setInterval(() => (waitNow = Date.now()), 1000)
        return () => { clearInterval(timer); }
    })
    const waitElapsed = $derived(live === null ? '' : liveWaitElapsed(live, waitNow))

    /**
     * What the status bar's live region actually says. A live run emits a batch every
     * 100 ms; announcing each one floods a screen reader with numbers, and an axe audit
     * sees nothing wrong with it. So the region gets a throttled copy while the visible
     * text updates freely, plus one guaranteed announcement when the run ends.
     */
    const announcer = createAnnouncementThrottle()
    let announcement = $state('')
    $effect(() => {
        if (announcer.offer(statusText, !streaming)) announcement = announcer.text
    })

    // ── Name column shrink-wrap ────────────────────────────────────────────────
    //
    // See the § "Name column" note at the top of this file for the layout contract and
    // why the measurement can't feed back into itself.

    /**
     * The Name grid track. Starts as the pre-measurement fallback (identical to the fixed
     * track this replaced), so a browser without canvas — or the tick before pretext lands
     * — renders exactly what it used to. The effect below swaps in a pixel width.
     */
    let nameTrack = $state('minmax(80px, 22ch)')
    /** Live scroll offset of `.results-container`, the first half of "which rows are visible". */
    let scrollTop = $state(0)
    /** Live client height of `.results-container`, the second half. */
    let viewportHeight = $state(0)
    /** Pixel-accurate measurer built at the ROW's font; null until pretext resolves. */
    let measureName = $state<((text: string) => number) | null>(null)
    /** Off for the first measured width (else the dialog opens with the column sliding in). */
    let animateNameTrack = $state(false)
    /** The font the current `measureName` was built for; a change (text size) rebuilds it. */
    let measuredFont = ''
    let firstWidthApplied = false
    let viewportObserver: ResizeObserver | undefined

    /**
     * Full grid template, handed to the header and every row as ONE inline string: two grid
     * containers can't be trusted to resolve the same tracks alike (`ch` already bit us —
     * see `.column-header`'s font-size).
     */
    const gridTemplate = $derived(
        showPathColumn
            ? `24px ${nameTrack} minmax(120px, 1fr) 10ch 16ch`
            : // No Path column (Selection): there's nothing to hand the freed width to, so
              // Name absorbs it as the flex track instead of shrink-wrapping and leaving a gap.
              '24px minmax(80px, 1fr) 10ch 16ch',
    )

    function readFont(node: HTMLElement): string {
        const style = getComputedStyle(node)
        return style.font || `${style.fontSize} ${style.fontFamily}`
    }

    /**
     * Builds (or rebuilds) the measurer from a real rendered name cell's font. Keying on the
     * computed font string means a text-size change re-measures on its own.
     */
    async function ensureMeasure(nameEl: HTMLElement): Promise<void> {
        const font = readFont(nameEl)
        if (font === measuredFont) return
        // Remember the attempt (success or failure) so we don't retry per scroll tick, and
        // drop the old measurer: it was built for a font we're no longer rendering.
        measuredFont = font
        measureName = null
        try {
            const pretext = await import('@chenglou/pretext')
            const candidate = createPretextMeasure(font, pretext)
            // Probe before adopting: pretext needs Canvas 2D and only fails on first use.
            candidate('0')
            measureName = candidate
        } catch {
            // No canvas, or the chunk failed to load: stay on the fallback track rather
            // than throwing on every render.
            measureName = null
        }
    }

    function handleResultsScroll(e: Event): void {
        scrollTop = (e.currentTarget as HTMLElement).scrollTop
    }

    /** Keeps `viewportHeight` live. Height is width-independent here, so this can't loop. */
    $effect(() => {
        const el = resultsContainer
        if (!el) return
        viewportHeight = el.clientHeight
        const observer = new ResizeObserver(() => {
            viewportHeight = el.clientHeight
        })
        observer.observe(el)
        viewportObserver = observer
        return () => {
            observer.disconnect()
            viewportObserver = undefined
        }
    })

    /**
     * Re-measures the Name track. Dependencies are read up front and are ALL independent of
     * `nameTrack`, which this effect never reads back — that's what rules out a loop.
     */
    $effect(() => {
        const container = resultsContainer
        const rows = results
        const withPath = showPathColumn
        const rowsShowing = showingRows
        const top = scrollTop
        const viewport = viewportHeight
        const measure = measureName
        if (!container || !withPath || !rowsShowing || rows.length === 0) return

        const rowEl = container.querySelector<HTMLElement>('.result-row')
        const nameEl = rowEl?.querySelector<HTMLElement>('.result-name')
        if (!rowEl || !nameEl) return
        void ensureMeasure(nameEl)
        if (!measure) return

        // Row height comes from the DOM but is driven by font + padding only: every cell is
        // `white-space: nowrap`, so it cannot change with the width we're about to set.
        const { start, end } = visibleRowRange(top, viewport, rowEl.getBoundingClientRect().height, rows.length)
        const names: string[] = []
        for (let i = start; i < end; i++) names.push(rows[i].name)

        nameTrack = `${String(
            computeNameColumnWidth({
                names,
                headerLabel: tString('queryUi.results.col.name'),
                measure,
            }),
        )}px`

        if (!firstWidthApplied) {
            firstWidthApplied = true
            requestAnimationFrame(() => {
                animateNameTrack = true
            })
        }
    })

    onDestroy(() => {
        viewportObserver?.disconnect()
    })

    /** Scrolls the cursor row into view. Called by the parent after cursor changes. */
    export function scrollCursorIntoView(): void {
        void tick().then(() => {
            const cursor = resultsContainer?.querySelector('.result-row.is-under-cursor')
            cursor?.scrollIntoView({ block: 'nearest' })
        })
    }
</script>

<!-- The bolded match total inside the count-only sentence. `children` carries the
     already-formatted number from the message, so the locale decides where it sits. -->
{#snippet countTotal(children: import('svelte').Snippet)}<strong class="count-only-number"
        >{@render children()}</strong
    >{/snippet}

<!-- The well wraps header + list + status bar so ONE element owns the rounded corners
     and clips the three square children inside them. It also carries the `flex: 1`
     that used to sit on `.results-container`, so the well (not the list alone) is
     what absorbs the dialog's spare height. -->
<div class="results-well">
    <!-- Column headers. Path is the flex column (1fr); Size + Modified are fixed `ch` tracks.
         Header cells use the same grid template as the rows so columns line up.

         Rendered ONLY when rows are (the `showingRows` predicate). Column labels over a
         spinner, a "no files match" list, the empty state, or a count-only total describe a
         table that isn't there, and they're the loudest thing in an otherwise quiet area.
         The seam they used to draw between the chip strip and the results is now the chip
         strip's own bottom hairline plus the surface flip. -->
    {#if showingRows}
        <div
            class="column-header"
            class:animate-track={animateNameTrack}
            style="grid-template-columns: {gridTemplate};"
        >
            <span class="col-label col-icon" aria-hidden="true"></span>
            <span class="col-label">{tString('queryUi.results.col.name')}</span>
            {#if showPathColumn}<span class="col-label col-path">{tString('queryUi.results.col.path')}</span>{/if}
            <span class="col-label col-right">{tString('queryUi.results.col.size')}</span>
            <span class="col-label col-right">{tString('queryUi.results.col.modified')}</span>
        </div>
    {/if}

    <!-- Results list. `role="listbox"` only applies when option rows are rendered; empty/loading/
         unavailable states are bare text containers so axe doesn't flag aria-required-children. -->
    <div
        class="results-container"
        bind:this={resultsContainer}
        onscroll={handleResultsScroll}
        role={showingRows ? 'listbox' : undefined}
        aria-label={showingRows ? tString('queryUi.results.listboxAria') : undefined}
    >
        {#if !isIndexAvailable}
            <div class="index-unavailable">
                <p class="unavailable-message">
                    {tString('queryUi.results.indexNotReady')}
                </p>
                {#if scanning}
                    <p class="unavailable-progress">
                        {entriesScanned > 0
                            ? tString('queryUi.results.scanProgressWithCount', {
                                  countText: formatEntryCount(entriesScanned),
                              })
                            : tString('queryUi.results.scanProgress')}
                    </p>
                {/if}
            </div>
        {:else if !isIndexReady && hasSearched}
            <div class="loading-state">
                <Spinner size="md" />
                <div class="loading-label">{tString('queryUi.results.loadingIndex')}</div>
            </div>
        {:else if liveWaiting && live}
            <!-- Three honest waits, not one spinner: working out what's already covered can
                 mean a multi-second index load on a big drive, reading the index is quick,
                 and walking what isn't indexed is unbounded. Saying which one you're in is
                 the difference between "slow" and "stuck". The counters and the way out
                 live in the status bar, so they don't move between here and there once the
                 first rows land. -->
            <div class="loading-state">
                <Spinner size="md" />
                <div class="loading-label">{livePhaseLabel(live.phase)}</div>
            </div>
        {:else if isSearching && !streaming}
            <!-- D1/D2: full result list area is replaced by the standard spinner +
                 "Searching..." label. No rows render while the fetch is in-flight,
                 since the previous result set is now stale relative to the new
                 query/filter state. -->
            <div class="loading-state">
                <Spinner size="md" />
                <div class="loading-label">{tString('queryUi.results.searching')}</div>
            </div>
        {:else if showingCount}
            <!-- Count-only: the search ran but the backend returned no rows, just a total. One
                 normal-size sentence with only the number in bold, and a way back to the list.
                 The `<total>` tag lets each locale put the number where its grammar wants it. -->
            <div class="count-only-summary" aria-live="polite">
                <p class="count-only-sentence">
                    <Trans
                        key={countIsProvisional
                            ? 'queryUi.results.countOnly.soFar'
                            : 'queryUi.results.countOnly.sentence'}
                        params={{ count: totalCount, countText: formatInteger(totalCount) }}
                        snippets={{ total: countTotal }}
                    />
                </p>
                {#if onShowResults}
                    <Button variant="secondary" onclick={onShowResults}>
                        {tString('queryUi.results.countOnly.showResults')}
                    </Button>
                {/if}
            </div>
        {:else if results.length === 0 && hasSearched && !isSearching && (query.trim() || sizeFilter !== 'any' || dateFilter !== 'any')}
            <!-- D4: structured no-results state. Heading + bulleted criteria list. -->
            <div class="no-results">
                <p class="no-results-heading">{tString('queryUi.results.noMatchHeading')}</p>
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
                    class:animate-track={animateNameTrack}
                    class:is-under-cursor={index === cursorIndex}
                    style="grid-template-columns: {gridTemplate};"
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
                            <span class="icon-fallback"><Icon name="folder" size={16} aria-hidden="true" /></span>
                        {:else}
                            <span class="icon-fallback"><Icon name="file" size={16} aria-hidden="true" /></span>
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
                </div>
            {/each}
        {/if}
    </div>

    <!-- Status bar. Always in the DOM so the `aria-live` region survives every state change and
         announces the next status; it collapses to nothing (no border, no padding, no height)
         whenever it has nothing to say, which keeps the results well from ending in an empty
         bordered strip while a search runs. Collapsing rather than unmounting also means the
         dialog's height doesn't jump when the bar has something to report again.

         While a live run streams, the bar is also its progress strip: the count, the walk's
         own progress, where it has got to, and the way to stop it. The `aria-live` region is
         the INNER span, carrying a throttled copy, because the visible numbers move ten times
         a second and a screen reader can't be asked to read that. -->
    <!-- `data-live-phase` names the phase a stalled run is sitting in, so a test (or a bug
         report) says WHICH wait it died in rather than "the button never enabled". The union
         is typed (`query-stream.ts`), which keeps readers off the localized status copy that
         `cmdr/no-error-string-match` forbids matching on. -->
    <div
        class="status-bar"
        class:is-empty={!statusText && !streaming}
        data-live-phase={live?.phase ?? 'idle'}
    >
        <span class="status-text">{statusText}</span>
        {#if walkProgress || waitElapsed}
            <span class="status-progress">{walkProgress || waitElapsed}</span>
        {/if}
        {#if streaming && live?.currentPath}
            <span
                class="status-path"
                aria-label={isWaiting
                    ? tString('queryUi.results.live.waitingOnPathAria', { path: live.currentPath })
                    : tString('queryUi.results.live.scanningAria', { path: live.currentPath })}
                use:useShortenMiddle={{ text: live.currentPath, preferBreakAt: '/', startRatio: 0.3 }}
            ></span>
        {/if}
        {#if streaming && onStopLive}
            <span class="status-stop" use:tooltip={tString('queryUi.results.live.stopTooltip')}>
                <Button variant="secondary" size="mini" onclick={onStopLive}>
                    {tString('queryUi.results.live.stop')}<ShortcutChip key="Esc" size="sm" />
                </Button>
            </span>
        {/if}
            <span class="sr-only" aria-live="polite">{announcement}</span>
        </div>
</div>

<style>
    /* Both containers get their `grid-template-columns` as one inline string from the
       `gridTemplate` derived (icon | name | path | size | modified), so they can't resolve
       the same tracks differently. Size and Modified stay fixed `ch` widths: don't switch
       them to `max-content`, or each row would resolve its own width from its own data
       ("Size" / "Modified" are narrower than `1.2 MB` / `Jan 12, 2026`) and the header would
       drift left of the row content. The measured Name track eases between widths;
       `.animate-track` is off for the first one so opening the dialog doesn't animate. */
    .column-header,
    .result-row {
        display: grid;
        column-gap: var(--spacing-md);
        align-items: center;
    }

    .column-header.animate-track,
    .result-row.animate-track {
        transition: grid-template-columns var(--transition-slow);
    }

    @media (prefers-reduced-motion: reduce) {
        .column-header.animate-track,
        .result-row.animate-track {
            transition: none;
        }
    }

    /* The results zone (header + list + status bar) is the ONLY part of the dialog with
       a surface of its own: `--color-bg-primary`, a recessed well against the panel's
       `--color-bg-dialog`. Everything above it sits on the panel. That flip IS the
       separation between "how do I narrow this" and "here's what I found"; the chip
       strip's bottom hairline and the footer's top hairline only sharpen it.

       The well is inset from the panel edge like every other block in the dialog, so it
       rounds its corners and clips the three square children to them. `overflow: hidden`
       does the clipping; the list inside keeps its own scrolling. */
    .results-well {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    /* The well's children inset by `--spacing-md`, their breathing room inside it; the
       dialog's own edge inset is `ModalDialog`'s and lands on the well. */
    .column-header {
        padding: var(--spacing-xs) var(--spacing-md);
        background: var(--color-bg-primary);
        /* The font-size MUST sit on the grid container, not just on `.col-label`: the
           `ch` tracks above resolve against the element that owns the grid. `.result-row`
           declares `--font-size-md` on itself, so a header left at the inherited root size
           resolved every `ch` track ~14% wider and pushed the whole right-hand side of the
           header out of line with the rows (the "Modified" label sat left of its dates).
           Keep the two declarations in lockstep. */
        font-size: var(--font-size-md);
        border-bottom: 1px solid var(--color-border-subtle);
        user-select: none;
    }

    .col-label {
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* The Path cell's content is a `PathPills` strip whose first pill carries
       `padding: 0 var(--spacing-xxs)`, so its text box starts inside the track. Inset the
       header label by the same amount and the two left edges line up. */
    .col-label.col-path {
        padding-left: var(--spacing-xxs);
    }

    .col-label.col-icon {
        width: 24px;
    }

    .col-label.col-right {
        text-align: right;
    }

    /* Results list. The `flex: 1 1 auto` child of the well, so it absorbs every bit of
       room the header and status bar leave. */
    .results-container {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        background: var(--color-bg-primary);
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

    /* Count-only summary: one body-size sentence with the total in bold, and a button
       back to the list. Centered in the results area, which holds nothing else here.
       Deliberately NOT a display-scale number: a lone huge digit reads as a dashboard
       stat, not as an answer to the question the user asked. */
    .count-only-summary {
        flex: 1 1 auto;
        min-height: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xl) var(--spacing-lg);
        text-align: center;
    }

    .count-only-sentence {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .count-only-number {
        font-weight: 600;
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
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
        padding: var(--spacing-xxs) var(--spacing-md);
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
        line-height: var(--font-line-height-flat);
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-fallback {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--color-text-secondary);
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

    /* Status bar closes the results zone: same surface as the list, separated by a
       hairline rather than a surface change (it reports ON the list, it isn't chrome). */
    .status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-md);
        background: var(--color-bg-primary);
        border-top: 1px solid var(--color-border-subtle);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        flex-shrink: 0;
    }

    /* Nothing to report: zero it out. `overflow: hidden` clips the (empty) text box, the
       transparent border keeps the 1px so the surrounding boxes don't shift by a hairline
       when the text comes back, and the element stays rendered for `aria-live`. */
    .status-bar.is-empty {
        padding-block: 0;
        height: 0;
        border-top-color: transparent;
        overflow: hidden;
    }

    .status-text {
        user-select: none;
        white-space: nowrap;
    }

    /* The walk's own progress, quieter than the match count it rides beside. */
    .status-progress {
        user-select: none;
        white-space: nowrap;
        color: var(--color-text-tertiary);
    }

    /* Where the walk has got to. It takes whatever room is left and mid-truncates, so a
       deep path can't push the Stop button off the end of the bar. */
    .status-path {
        flex: 1 1 auto;
        min-width: 0;
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        white-space: nowrap;
        overflow: hidden;
    }

    /* Pinned right: the way out of a search that's taking too long stays in one place
       whether or not there's a path to show. */
    .status-stop {
        margin-left: auto;
        display: inline-flex;
        align-items: center;
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
