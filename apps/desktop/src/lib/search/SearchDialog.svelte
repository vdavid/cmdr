<script lang="ts">
    /**
     * SearchDialog: thin Search-specific wrapper around the shared `QueryDialog`.
     *
     * Dialog orchestration lives in
     * [`lib/query-ui/QueryDialog.svelte`](../query-ui/QueryDialog.svelte). This file owns
     * only the Search-specific glue:
     *
     *   - Builds the `QueryDialogConfig` for Search (title, max width, history store,
     *     filter chips extras, primary "Show all in main window" + secondary "Go to file"
     *     actions, AI translation IPC + filter writes, snapshot promotion).
     *   - Wires the whole-drive index lifecycle (`prepareSearchIndex` on mount,
     *     `releaseSearchIndex` on destroy, plus the `search-index-ready` listener).
     *   - Owns the "Open in pane" snapshot promotion path: minting an id, populating the
     *     snapshot store, pinning the last-attempt ref, persisting to recent searches,
     *     handing the id to the host.
     *   - Loads the system-dir exclude tooltip.
     *   - Provides recent-searches activate + remove handlers, including the IPC
     *     write-back on removal.
     *
     * QueryDialog owns everything else: overlay, keyboard contract, IME guard, auto-apply
     * gates, `deriveEnterAction` ownership, `lastDialogEvent` lifecycle, title bar, the
     * chip strip, the AI prompt strip, the results table, the recent-items footer +
     * popover, and the empty state.
     */
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { bytesToSize } from '$lib/query-ui/query-filter-state.svelte'
    import {
        prepareSearchIndex,
        searchFiles,
        releaseSearchIndex,
        translateSearchQuery,
        parseSearchScope,
        getSystemDirExcludes,
        onSearchIndexReady,
        showFileContextMenu,
        trackEvent,
        getRecentSearches as fetchRecentSearches,
        removeRecentSearch as removeRecentSearchIpc,
        addRecentSearch as addRecentSearchIpc,
        type HistoryEntry,
        type SearchResultEntry,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { isScanning, getEntriesScanned } from '$lib/indexing'
    import {
        searchQueryState,
        clearSearchState,
        clearAiPattern,
        buildSearchQuery,
        buildHistoryFilters,
        applyHistoryEntry,
        getQuery,
        getMode,
        getCaseSensitive,
        setCaseSensitive,
        getScope,
        setScope,
        getExcludeSystemDirs,
        setExcludeSystemDirs,
        getResults,
        getTotalCount,
        getLastAiPrompt,
        getLastAiLabel,
        getLastAiPattern,
        getLastAiPatternKind,
        getSizeFilter,
        setSizeFilter,
        setSizeValue,
        setSizeUnit,
        setSizeValueMax,
        setSizeUnitMax,
        getDateFilter,
        setDateFilter,
        setDateValue,
        setDateValueMax,
        recordAiTranslation,
        getIsIndexReady,
        setIsIndexReady,
        getIndexEntryCount,
        setIndexEntryCount,
        getIsIndexAvailable,
        setIsIndexAvailable,
    } from './search-state.svelte'
    import QueryDialog from '$lib/query-ui/QueryDialog.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import type {
        QueryDialogConfig,
        QueryDialogFilterChipsExtras,
        AiTranslateResult,
    } from '$lib/query-ui/query-dialog-config'
    import {
        loadRecentSearches,
        getRecentSearchesList,
        setRecentSearchesList,
        recentSearchesStore,
    } from './recent-searches-state.svelte'
    import {
        chipTooltip,
        modeName,
        formatAge,
    } from '$lib/query-ui/recent-items/recent-items-utils'
    import type {
        RecentItemAdapter,
        RecentItemKey,
    } from '$lib/query-ui/recent-items/recent-items-types'
    import {
        getOrCreate as createSnapshot,
        nextSnapshotId,
        setLastAttemptId,
        type SearchSnapshot,
    } from './snapshot-store.svelte'
    import { buildSnapshotLabel } from './snapshot-label'

    interface Props {
        /** Called when user selects a result: receives the full path. */
        onNavigate: (path: string) => void
        /** Called when dialog is closed. */
        onClose: () => void
        /**
         * Smart "current folder" for the Search-in popover's `Use current folder` button.
         * Round-2 D12: when the focused pane is a `search-results://` snapshot, the host
         * walks the pane's history back to the most recent real folder; when none is
         * available, this surfaces `disabled: true` plus a tooltip so the dialog can
         * render the button visibly disabled. See `lib/search/searchable-folder.ts`.
         */
        searchableFolder: {
            path: string | null
            disabled: boolean
            disabledReason: string
        }
        /**
         * Called when the user activates "Show all in main window" (⌥⏎ or footer click).
         * Receives the freshly-created snapshot id; the host
         * (`+page.svelte` → `DualPaneExplorer`) routes the active pane to
         * `search-results://<id>`. The wrapper closes itself; the handler doesn't need to.
         */
        onShowAllInMainWindow?: (snapshotId: string) => void
    }

    const { onNavigate, onClose, searchableFolder, onShowAllInMainWindow }: Props = $props()

    // Index-readiness listener cleanup. Lives on the wrapper because the listener is
    // Search-specific (Selection has no whole-drive index).
    let unlistenReady: UnlistenFn | undefined

    // System-dir exclude tooltip (populated async on mount; renders the full exclude list).
    let systemDirExcludeTooltip = $state('Excludes common system and build folders')

    // Live mirror of the AI provider setting. Drives `aiEnabled` reactively so toggling
    // in the settings window flips the AI chip in real time without reopening the dialog.
    let aiProvider = $state<string>(getSetting('ai.provider'))
    let unlistenAiProvider: (() => void) | undefined

    // Reactive readers off the Search state instance. Used by the derived config below.
    const isIndexReady = $derived(getIsIndexReady())
    const indexEntryCount = $derived(getIndexEntryCount())
    const isIndexAvailable = $derived(getIsIndexAvailable())
    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())
    const aiEnabled = $derived(aiProvider !== 'off' && isIndexAvailable)
    const inputsDisabled = $derived(!isIndexAvailable)
    const lastAiPattern = $derived(getLastAiPattern())

    /**
     * Adapter from Search's `HistoryEntry` shape into the generic `RecentItemView` the
     * `RecentItemsFooter` / `RecentItemsPopover` consume. The adapter is the only seam where
     * Search-specific fields (`scope`, `excludeSystemDirs`, `caseSensitive`, etc.) leak into
     * the chip's tooltip. Selection's wrapper passes its own adapter against its narrower
     * entry shape.
     */
    const searchRecentAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
        label: entry.query,
        tooltip: chipTooltip(entry),
        mode: entry.mode,
        ageLabel: formatAge(entry.timestamp),
        ariaLabel: `Run recent ${modeName(entry.mode)} search: ${entry.query}`,
    })
    const searchRecentKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

    /** Recovers the structured pattern kind ('glob' | 'regex' | null) from the AI display string. */
    function patternKindFromDisplay(patternType: string | null | undefined): 'glob' | 'regex' | null {
        if (patternType === 'regex') return 'regex'
        if (patternType === 'glob') return 'glob'
        return null
    }

    /** Applies AI-returned size filters to the Search state. Returns true if any were applied. */
    function applySizeFromAi(minSize: number | null, maxSize: number | null): boolean {
        if (minSize == null && maxSize == null) return false
        if (minSize != null && maxSize != null) {
            setSizeFilter('between')
            const lo = bytesToSize(minSize)
            const hi = bytesToSize(maxSize)
            setSizeValue(lo.value)
            setSizeUnit(lo.unit)
            setSizeValueMax(hi.value)
            setSizeUnitMax(hi.unit)
        } else if (minSize != null) {
            setSizeFilter('gte')
            const lo = bytesToSize(minSize)
            setSizeValue(lo.value)
            setSizeUnit(lo.unit)
        } else if (maxSize != null) {
            setSizeFilter('lte')
            const hi = bytesToSize(maxSize)
            setSizeValue(hi.value)
            setSizeUnit(hi.unit)
        }
        return true
    }

    /** Applies AI-returned date filters to the Search state. Returns true if any were applied. */
    function applyDateFromAi(after: string | null, before: string | null): boolean {
        if (after == null && before == null) return false
        if (after != null && before != null) {
            setDateFilter('between')
            setDateValue(after)
            setDateValueMax(before)
        } else if (after != null) {
            setDateFilter('after')
            setDateValue(after)
        } else if (before != null) {
            setDateFilter('before')
            setDateValue(before)
        }
        return true
    }

    /** Folds the AI's `includePaths` + `excludeDirNames` into one scope expression. Returns true if set. */
    function applyScopeFromAi(includePaths: string[] | null, excludeDirNames: string[] | null): boolean {
        if (!includePaths?.length && !excludeDirNames?.length) return false
        const parts: string[] = []
        if (includePaths) parts.push(...includePaths)
        if (excludeDirNames) parts.push(...excludeDirNames.map((d) => `!${d}`))
        setScope(parts.join(', '))
        return true
    }

    /**
     * Translates a natural-language prompt and applies the AI's filter writes: the Pattern
     * chip + label, size, date, scope, case sensitivity, and "hide boring folders". Returns
     * the caveat + highlighted-field list for QueryDialog to surface in the AI strip and
     * flash effect. Per QueryDialog's ownership contract, this does NOT write
     * `state.lastAiPrompt` / `state.lastAiCaveat` — QueryDialog owns both.
     *
     * Lets the typed IPC error throw: QueryDialog catches it and shows a specific toast
     * (quota, key rejected, timeout, empty answer, …) instead of failing silently.
     */
    async function translateAi(prompt: string): Promise<AiTranslateResult | null> {
        const result = await translateSearchQuery(prompt)
        const { display, query } = result
        const changed = new SvelteSet<string>()

        // Record the produced pattern in its own slot (the Pattern chip). The bar keeps the prompt.
        recordAiTranslation({
            pattern: display.namePattern ?? null,
            kind: patternKindFromDisplay(display.patternType),
            label: result.label ?? null,
        })
        if (display.namePattern != null) changed.add('pattern')

        if (query.caseSensitive != null) {
            setCaseSensitive(query.caseSensitive)
            changed.add('caseSensitive')
        }
        // The AI only ever turns OFF the default "hide boring folders" exclusion.
        if (query.excludeSystemDirs === false) {
            setExcludeSystemDirs(false)
            changed.add('excludeSystemDirs')
        }
        if (applySizeFromAi(display.minSize ?? null, display.maxSize ?? null)) changed.add('size')
        if (applyDateFromAi(display.modifiedAfter ?? null, display.modifiedBefore ?? null)) changed.add('date')
        if (applyScopeFromAi(query.includePaths ?? null, query.excludeDirNames ?? null)) changed.add('scope')

        return {
            caveat: result.caveat,
            highlightedFields: Array.from(changed),
        }
    }

    /**
     * Runs the Search query against the backend index. Reads the bar + filters + AI
     * pattern off the Search state; builds the payload via `buildSearchQuery()`; parses
     * the scope expression via `parseSearchScope` (async, so not part of buildSearchQuery);
     * and returns the result. QueryDialog owns the `results` / `totalCount` / `cursorIndex`
     * writes.
     */
    async function runSearch(): Promise<{ entries: SearchResultEntry[]; totalCount: number }> {
        const query = buildSearchQuery()
        // After an AI translation, the bar still shows the user's natural-language
        // prompt. The actual search must run against the AI's produced pattern, not
        // the prompt. Same for any AI-mode search where the user kept a pattern around.
        if (getMode() === 'ai') {
            const aiPattern = getLastAiPattern()
            const aiKind = getLastAiPatternKind()
            query.namePattern = aiPattern && aiPattern.trim() ? aiPattern : null
            query.patternType = aiKind === 'regex' ? 'regex' : 'glob'
        }
        // Parse scope and merge into query if non-empty.
        const scopeStr = getScope().trim()
        if (scopeStr) {
            const parsed = await parseSearchScope(scopeStr)
            if (parsed.includePaths.length > 0) query.includePaths = parsed.includePaths
            if (parsed.excludePatterns.length > 0)
                query.excludeDirNames = parsed.excludePatterns
        }
        const result = await searchFiles(query)
        // PII-free analytics: a search ran. Only the mode enum crosses; never the query/pattern.
        void trackEvent('search_used', { mode: getMode() })
        return { entries: result.entries, totalCount: result.totalCount }
    }

    /**
     * "Show all in main window" (⌥⏎).
     *
     * Promotes the current result set into a real pane view via the search-results
     * virtual volume. Steps:
     *
     *   1. Build a `SearchSnapshot` from the live dialog state.
     *   2. Mint a fresh snapshot id and store it.
     *   3. Pin the snapshot's refcount via `setLastAttemptId`.
     *   4. Persist a `HistoryEntry` via `add_recent_search` (the single sanctioned add
     *      point — auto-applies and Enter-runs don't push to recent searches).
     *   5. Hand the id to the host; the host routes the active pane to
     *      `search-results://<id>` and the pane's history push bumps the refcount.
     *   6. Close the dialog. State is preserved (the module-level $state survives
     *      unmount), so reopening with ⌘F lands the user back on the same results.
     */
    function showAllInMainWindow(): void {
        if (getResults().length === 0) return
        const id = nextSnapshotId()
        const label = buildSnapshotLabel({
            mode: getMode(),
            query: getQuery(),
            aiPrompt: getLastAiPrompt(),
            aiLabel: getLastAiLabel(),
        })
        // `HistoryFilters` (IPC type) uses `number | null` for absent fields; the
        // snapshot store uses `number | undefined`. Coerce so `null` doesn't sneak
        // into the snapshot's runtime shape.
        const hf = buildHistoryFilters()
        const snapshotFilters = {
            ...(hf.sizeMin != null ? { sizeMin: hf.sizeMin } : {}),
            ...(hf.sizeMax != null ? { sizeMax: hf.sizeMax } : {}),
            // Snapshot date filters intentionally omitted: the search-results pane
            // doesn't need them post-run (the snapshot stores the matched paths
            // directly, not the date predicate).
        }
        const snapshot: SearchSnapshot = {
            id,
            query: getQuery(),
            mode: getMode(),
            filters: snapshotFilters,
            scope: getScope(),
            caseSensitive: getCaseSensitive(),
            excludeSystemDirs: getExcludeSystemDirs(),
            entries: getResults(),
            totalCount: getTotalCount(),
            createdAt: Date.now(),
            label,
        }
        createSnapshot(id, snapshot)
        setLastAttemptId(id)

        // Persist to recent searches (the only call site that does this).
        const historyEntry: HistoryEntry = {
            id: crypto.randomUUID(),
            timestamp: Date.now(),
            mode: getMode(),
            query: getMode() === 'ai' ? (getLastAiPrompt() ?? getQuery()) : getQuery(),
            filters: buildHistoryFilters(),
            scope: getScope(),
            caseSensitive: getCaseSensitive(),
            excludeSystemDirs: getExcludeSystemDirs(),
            resultCount: getTotalCount(),
        }
        void addRecentSearchIpc(historyEntry).catch(() => {
            // Silent on history persistence failure: the snapshot still opens.
        })

        onShowAllInMainWindow?.(id)
        onClose()
    }

    /**
     * "Go to file" (⏎ when results are present): close the dialog and route the active
     * pane to the cursor row. The host's `onNavigate(path)` handles closing the dialog,
     * navigating to the parent folder, and focusing the file (pushing a history entry).
     */
    function goToCursorFile(entry: SearchResultEntry): void {
        onNavigate(entry.path)
    }

    /**
     * Per-row context menu: routes to the native menu factory. Reuses the same
     * `showFileContextMenu` IPC the file panes use.
     */
    function openRowMenu(entry: SearchResultEntry): void {
        void showFileContextMenu(entry.path, entry.name, entry.isDirectory, [entry.path]).catch(
            () => {
                // Silent: a missing menu is preferable to a stuck dialog.
            },
        )
    }

    /**
     * Path-pill click: route the active pane to the ancestor path and close the dialog.
     * Reuses the same `onNavigate` exit path as a result click so close + history-push
     * are handled uniformly.
     */
    function pickPath(ancestorPath: string): void {
        onNavigate(ancestorPath)
    }

    /**
     * Recent-search activation: applies the history entry's state into the live dialog,
     * then triggers a run. AI entries count the click as the explicit-trigger so they
     * re-translate.
     */
    function activateHistoryEntry(entry: HistoryEntry): void {
        applyHistoryEntry(entry)
        // QueryDialog drives the run via the `runOnMount` consumer in its $effect.
        // To trigger a fresh run from history, set runOnMount; QueryDialog will pick
        // it up and dispatch to AI or non-AI based on mode.
        searchQueryState.setRunOnMount(true)
    }

    /** Removes a recent search entry; backend write is async, we update the cache eagerly. */
    function removeHistoryEntry(entry: HistoryEntry): void {
        setRecentSearchesList(getRecentSearchesList().filter((e) => e.id !== entry.id))
        void removeRecentSearchIpc(entry.id).then(async () => {
            try {
                setRecentSearchesList(await fetchRecentSearches())
            } catch {
                // Already fell back to the optimistic snapshot; nothing to do.
            }
        })
    }

    // QueryDialog already wrote the chip's query + mode into state and triggered the
    // run. Search has no per-chip side effects, so this hook is a no-op for now.
    const pickExample = (): void => {}

    // ─────────────────────────────────────────────────────────────────────────
    // Search-specific lifecycle: index prepare / release, ready listener,
    // system-dir tooltip, AI-provider live subscription.
    // ─────────────────────────────────────────────────────────────────────────

    async function setupSearchLifecycle(): Promise<void> {
        // Listen for index ready event.
        unlistenReady = await onSearchIndexReady((entryCount: number) => {
            setIsIndexReady(true)
            setIndexEntryCount(entryCount)
            // Auto-run pending search if user already typed something (filename/regex
            // only; AI mode always waits for explicit Enter / ⌘Enter).
            const pendingMode = getMode()
            if (
                pendingMode !== 'ai' &&
                (getQuery().trim() || getSizeFilter() !== 'any' || getDateFilter() !== 'any')
            ) {
                // Trigger via the runOnMount flag; QueryDialog's effect dispatches to
                // the non-AI runner since mode !== 'ai'.
                searchQueryState.setRunOnMount(true)
            }
        })

        try {
            const result = await prepareSearchIndex()
            if (result.ready) {
                setIsIndexReady(true)
                setIndexEntryCount(result.entryCount)
            }
        } catch {
            // Index not available: indexing disabled, not started, or backend unavailable.
            setIsIndexAvailable(false)
        }

        // Persisted recent searches load (idempotent across the session).
        void loadRecentSearches()

        // R3 U6: load the full system-dir exclude list for the tooltip.
        function escapeHtml(s: string): string {
            return s
                .replace(/&/g, '&amp;')
                .replace(/</g, '&lt;')
                .replace(/>/g, '&gt;')
                .replace(/"/g, '&quot;')
                .replace(/'/g, '&#39;')
        }
        getSystemDirExcludes()
            .then((dirs) => {
                const items = dirs
                    .map(
                        (d) =>
                            `<div style="font-family:var(--font-mono);font-size:var(--font-size-xs);color:var(--color-text-secondary);">${escapeHtml(d)}</div>`,
                    )
                    .join('')
                systemDirExcludeTooltip =
                    '<div style="max-width:360px;max-height:60vh;overflow-y:auto;">' +
                    '<div style="font-weight:600;margin-bottom:4px">These folders are hidden:</div>' +
                    items +
                    '</div>'
            })
            .catch(() => {})
    }

    function teardownSearchLifecycle(): void {
        releaseSearchIndex().catch(() => {})
        unlistenReady?.()
        unlistenReady = undefined
    }

    onMount(() => {
        // Live-mirror `ai.provider` so the AI chip appears / disappears in real time when
        // the user changes the provider in the settings window.
        unlistenAiProvider = onSpecificSettingChange('ai.provider', (_id, value: unknown) => {
            aiProvider = typeof value === 'string' ? value : 'off'
        })
    })

    onDestroy(() => {
        unlistenAiProvider?.()
        unlistenAiProvider = undefined
    })

    // ─────────────────────────────────────────────────────────────────────────
    // The QueryDialogConfig. Rebuilt reactively so live changes in the inputs
    // (search state, settings, focused-pane changes) propagate to QueryDialog.
    // ─────────────────────────────────────────────────────────────────────────

    const filterChipsExtras: QueryDialogFilterChipsExtras = $derived({
        caseSensitive: getCaseSensitive(),
        scope: getScope(),
        excludeSystemDirs: getExcludeSystemDirs(),
        searchableFolder,
        systemDirExcludeTooltip,
        aiPattern: lastAiPattern,
        onToggleCaseSensitive: () => {
            setCaseSensitive(!getCaseSensitive())
        },
        onToggleExcludeSystemDirs: () => {
            setExcludeSystemDirs(!getExcludeSystemDirs())
        },
        onSetScope: setScope,
        onClearAiPattern: clearAiPattern,
    })

    const config: QueryDialogConfig<HistoryEntry> = $derived({
        title: 'Search',
        badge: getBadgeStatus('search'),
        dialogType: 'search',
        maxWidth: 'min(1080px, 80vw)',

        state: searchQueryState,

        aiEnabled,
        inputsDisabled,

        visibleChips: { size: true, date: true, scope: true, pattern: true },
        showPathColumn: true,

        runHintCopy: 'Press Enter to search',

        historyStore: recentSearchesStore,
        recentItems: {
            adapter: searchRecentAdapter,
            keyFn: searchRecentKey,
        },
        onLoadHistory: async () => {
            await loadRecentSearches()
        },

        emptyState: {
            // Examples + indexHint shapes are reserved for Selection consumers; Search
            // reads its examples + index count off QueryDialog's defaults today.
            examples: [],
            indexEntryCount,
        },

        filterChipsExtras,

        scanning,
        entriesScanned,
        indexEntryCount,
        isIndexAvailable,
        isIndexReady,

        runQuery: runSearch,
        translateAi,

        primaryAction: {
            label: 'Show all in main window',
            shortcutHint: '⌥⏎',
            tooltip: 'Open the search results in the active pane',
            ariaLabel: 'Show all in main window',
            handler: showAllInMainWindow,
        },
        secondaryAction: {
            label: 'Go to file',
            shortcutHint: '⏎',
            tooltip: 'Open the file in the active pane',
            ariaLabel: 'Go to file',
            handler: goToCursorFile,
        },

        onPickPath: pickPath,
        onPickExample: pickExample,
        onRowMenu: openRowMenu,
        onActivateRecent: activateHistoryEntry,
        onRemoveRecent: removeHistoryEntry,

        onClose,

        onMount: setupSearchLifecycle,
        onDestroy: teardownSearchLifecycle,

        // ⌘N clears core + extras together (the Search facade). Search's facade is
        // the canonical reset surface; using `state.clearCore()` alone would leave
        // scope / excludeSystemDirs / AI label dangling.
        onClearState: clearSearchState,
    })
</script>

<QueryDialog {config} />
