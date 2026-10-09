<script lang="ts">
    /**
     * SearchResults: Column headers + results list + all states + status bar.
     *
     * The rows are the house `ColumnList` (measured tracks shared by header and rows, the
     * single cursor, virtual scrolling); this component owns the states around it and the
     * cells. Name mid-truncates (`useShortenMiddle`); Path renders via `PathPills` with
     * overflow-aware collapse; Size and Modified shrink-wrap to their widest cell. There is
     * no actions column: the row's own right-click (`onRowMenu`) opens the native context
     * menu, which is the whole of what a per-row `…` button offered.
     *
     * Cursor model (single cursor): hover writes `cursorIndex` via `onHover`, so mouse and
     * keyboard share one accent-colored cursor; the parent dialog owns the arrows and loops
     * top<->bottom. Column widths: `result-column-widths.ts` and DETAILS.md § Column widths.
     */
    import { tick } from 'svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { dependOn } from '$lib/utils/reactivity'
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
    import ColumnList from '$lib/ui/ColumnList.svelte'
    import {
        columnListProps,
        type ColumnListApi,
        type ColumnListCellContext,
        type ColumnListColumn,
    } from '$lib/ui/column-list-types'
    import { resultColumnWidths } from './result-column-widths'
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
        /**
         * How to NAME the ground the run covered, for the no-results criteria list ("Downloads",
         * or a typed scope verbatim). Search wires it; Selection passes `null` (one folder, so
         * there's no scope to voice) and the bullet doesn't render. See `scope-summary.ts` for
         * why the bullet exists at all.
         */
        scopeSummary?: string | null
        /**
         * Re-runs the search across the whole volume, wired to the "Search this volume instead"
         * button under the criteria list. Absent when there's nowhere wider to go (the run
         * already covered the volume) or no scope at all (Selection), and the button then
         * doesn't render. Like `onShowResults`, the handler MUST re-run: widening the scope
         * without one leaves the same empty list on screen.
         */
        onWidenToVolume?: () => void
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
        /** Cloud without "Allow cloud AI": the empty state trades its AI prompts for the reason. */
        aiBlocked?: boolean
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
        scopeSummary = null,
        onWidenToVolume,
        scanning,
        entriesScanned,
        totalCount,
        indexEntryCount,
        countOnly = false,
        onShowResults,
        live = null,
        onStopLive,
        iconCacheVersion: iconVersionProp,
        aiEnabled,
        aiBlocked = false,
        showPathColumn = true,
        onResultClick,
        onHover,
        onPickExample,
        emptyExamples,
        onPickPath,
        onRowMenu,
    }: Props = $props()

    let columnList: ColumnListApi | undefined = $state()

    // Subscribe to icon cache version for reactivity
    const iconVersion = $derived($iconCacheVersion)

    function getIconUrl(iconId: string): string | undefined {
        dependOn(iconVersion, iconVersionProp)
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
                return tString('queryUi.results.scanningWithCount', {
                    countText: formatEntryCount(entriesScanned),
                    count: entriesScanned,
                })
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
                return tString('queryUi.results.indexReadyStatus', {
                    countText: formatEntryCount(indexEntryCount),
                    count: indexEntryCount,
                })
            }
            if (totalCount === 0) return ''
            return tString('queryUi.results.resultCount', {
                shownText: String(results.length),
                totalText: formatInteger(totalCount),
                total: totalCount,
            })
        }
        // Index loading: the content area shows the "Loading drive index…" spinner,
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
        // Last, and unconditional for a scoped consumer: it's the criterion the user didn't
        // choose, so it's the one they're least likely to suspect, and the most likely answer
        // to "why did this find nothing".
        if (scopeSummary) out.push(tString('queryUi.results.criteria.scope', { scope: scopeSummary }))
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

    // True only when the list renders rows. `ColumnList` is a `role="listbox"`, which requires
    // `option` children, so it must NOT render during the searching / loading / empty states
    // (which replace the rows with a spinner or message) even when `results` still holds a
    // stale set. `isSearching` is TRUE for a live run's whole life, and its rows are the
    // point, so the spinner only owns the area while nothing has arrived (`liveWaiting`).
    const showingRows = $derived(
        isIndexAvailable && isIndexReady && (!isSearching || streaming) && !countOnly && results.length > 0,
    )

    const columns = $derived.by<ColumnListColumn<SearchResultEntry>[]>(() => {
        const widths = resultColumnWidths(showPathColumn)
        const name: ColumnListColumn<SearchResultEntry> = {
            id: 'name',
            label: tString('queryUi.results.col.name'),
            ...widths.name,
            emphasis: true,
            class: 'result-name',
            cell: nameCell,
        }
        const path: ColumnListColumn<SearchResultEntry> = {
            id: 'path',
            label: tString('queryUi.results.col.path'),
            header: pathHeader,
            ...widths.path,
            tone: 'tertiary',
            class: 'result-path',
            cell: pathCell,
        }
        return [
            { id: 'icon', label: '', ...widths.icon, clip: false, class: 'result-icon', cell: iconCell },
            name,
            ...(showPathColumn ? [path] : []),
            {
                id: 'size',
                label: tString('queryUi.results.col.size'),
                ...widths.size,
                align: 'end',
                tone: 'secondary',
                class: 'result-size',
                cell: sizeCell,
            },
            {
                id: 'modified',
                label: tString('queryUi.results.col.modified'),
                ...widths.modified,
                align: 'end',
                tone: 'tertiary',
                class: 'result-modified',
                cell: modifiedCell,
            },
        ]
    })

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

    /** Scrolls the cursor row into view. Called by the parent after cursor changes. */
    export function scrollCursorIntoView(): void {
        void tick().then(() => {
            columnList?.scrollIndexIntoView(cursorIndex)
        })
    }
</script>

<!-- The bolded match total inside the count-only sentence. `children` carries the
     already-formatted number from the message, so the locale decides where it sits. -->
{#snippet countTotal(children: import('svelte').Snippet)}<strong class="count-only-number"
        >{@render children()}</strong
    >{/snippet}

{#snippet iconCell({ row }: ColumnListCellContext<SearchResultEntry>)}
    <span class="icon-box">
        {#if getIconUrl(row.iconId)}
            <img class="icon-img" src={getIconUrl(row.iconId)} alt="" width="16" height="16" />
        {:else if row.isDirectory}
            <span class="icon-fallback"><Icon name="folder" size={16} aria-hidden="true" /></span>
        {:else}
            <span class="icon-fallback"><Icon name="file" size={16} aria-hidden="true" /></span>
        {/if}
    </span>
{/snippet}

<!-- Mid-truncating name. `useShortenMiddle` measures with pretext and snaps to '.' so the
     extension stays visible. Tooltip shows the full name only when truncation happened. -->
{#snippet nameCell({ row }: ColumnListCellContext<SearchResultEntry>)}
    <span
        class="name-text"
        use:useShortenMiddle={{ text: row.name, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
    ></span>
{/snippet}

<!-- The Path cell's content is a `PathPills` strip whose first pill carries
     `padding: 0 var(--spacing-xxs)`, so its text box starts inside the track. The header
     label is inset by the same amount so the two left edges line up. -->
{#snippet pathHeader()}
    <span class="path-label">{tString('queryUi.results.col.path')}</span>
{/snippet}

{#snippet pathCell({ row }: ColumnListCellContext<SearchResultEntry>)}
    <PathPills path={row.parentPath} onPick={onPickPath} />
{/snippet}

{#snippet sizeCell({ row }: ColumnListCellContext<SearchResultEntry>)}
    <Size bytes={row.size} />
{/snippet}

{#snippet modifiedCell({ row }: ColumnListCellContext<SearchResultEntry>)}
    <DateLabel modifiedAt={row.modifiedAt} />
{/snippet}

<!-- The well wraps header + list + status bar so ONE element owns the rounded corners
     and clips the three square children inside them. It also carries the `flex: 1`
     that used to sit on `.results-container`, so the well (not the list alone) is
     what absorbs the dialog's spare height. -->
<div class="results-well">
    <!-- The list owns its column header, so the labels render ONLY with the rows (the
         `showingRows` predicate). Column labels over a spinner, a "no files match" list, the
         empty state, or a count-only total describe a table that isn't there, and they're the
         loudest thing in an otherwise quiet area. Every other state is a bare text container
         with no role, so axe doesn't flag `aria-required-children`. -->
    <div class="results-container" class:has-list={showingRows}>
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
                                  count: entriesScanned,
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
                 "Searching…" label. No rows render while the fetch is in-flight,
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
                {#if onWidenToVolume}
                    <!-- The scope bullet above names the constraint; this undoes it in one click.
                         ⌥V in the scope popover does the same, but nobody opens a popover to
                         explain an empty list. -->
                    <Button variant="secondary" onclick={onWidenToVolume}>
                        {tString('queryUi.results.widenToVolume')}
                    </Button>
                {/if}
            </div>
        {:else if !hasSearched && !query.trim() && isIndexReady && sizeFilter === 'any' && dateFilter === 'any'}
            <EmptyState {aiEnabled} {aiBlocked} {indexEntryCount} examples={emptyExamples} onPick={onPickExample} />
        {:else if showingRows}
            <ColumnList
                bind:this={columnList}
                {...columnListProps({
                    columns,
                    rows: results,
                    rowKey: (entry) => entry.path,
                    ariaLabel: tString('queryUi.results.listboxAria'),
                    cursorIndex,
                    onHover,
                    onRowClick: onResultClick,
                    onRowContextMenu: ({ row }) => {
                        onRowMenu(row)
                    },
                    headerClass: 'column-header',
                    rowClass: 'result-row',
                })}
            />
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

    /* The content area: the list, or the state that replaces it. The `flex: 1 1 auto`
       child of the well, so it absorbs every bit of room the status bar leaves. */
    .results-container {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        background: var(--color-bg-primary);
    }

    /* Rows: the list scrolls its own viewport under a pinned header, so the container
       hands it the height instead of scrolling itself. */
    .results-container.has-list {
        display: flex;
        flex-direction: column;
        overflow: hidden;
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

    /* Cell contents. The cells themselves (font, tone, alignment, the cursor recolor) are
       `ColumnList`'s. */
    .icon-box {
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

    /* A block, so `useShortenMiddle` reads the track's width rather than its own text's. */
    .name-text {
        display: block;
        overflow: hidden;
        white-space: nowrap;
    }

    .path-label {
        padding-left: var(--spacing-xxs);
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
