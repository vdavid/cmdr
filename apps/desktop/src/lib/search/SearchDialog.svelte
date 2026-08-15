<script lang="ts">
    /**
     * SearchDialog: thin Search-specific wrapper around the shared `QueryDialog`.
     *
     * Dialog orchestration lives in
     * [`lib/query-ui/QueryDialog.svelte`](../query-ui/QueryDialog.svelte): overlay, keyboard
     * contract, IME guard, auto-apply gates, `deriveEnterAction` ownership, `lastDialogEvent`
     * lifecycle, title bar, chip strip, AI prompt strip, results table, recent-items dropdown,
     * empty state.
     *
     * The Search-specific glue is one module per job, and this file only wires them into a
     * `QueryDialogConfig`:
     *
     *   - `search-lifecycle.svelte.ts`: index prepare / release, the `search-index-ready`
     *     listener, the per-target readiness gate, the system-dir exclude tooltip.
     *   - `search-runners.ts`: the one-shot `runQuery` and the live `streamingSource`, the
     *     query builder they share, and the coverage note both write.
     *   - `ai-translate.ts`: the translate IPC and every filter write its answer makes.
     *   - `coverage-cta.svelte.ts`: what may be OFFERED over a coverage gap.
     *   - `snapshot-promotion.ts`: "Show all in main window", and the recent-search writes.
     *   - `recent-search-adapter.ts`: how a history entry renders, and pick / remove.
     */
    import { onMount, onDestroy } from 'svelte'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { showFileContextMenu, type HistoryEntry, type SearchResultEntry } from '$lib/tauri-commands'
    import { resolveDefaultScope, defaultScopeLabel } from './searchable-folder'
    import type { ScopePresets } from '$lib/query-ui/query-dialog-config'
    import { tString } from '$lib/intl/messages.svelte'
    import { ROOT_VOLUME_ID } from '$lib/indexing'
    import {
        searchQueryState,
        clearSearchState,
        clearAiPattern,
        getQuery,
        getCaseSensitive,
        setCaseSensitive,
        getScope,
        setScope,
        getExcludeSystemDirs,
        setExcludeSystemDirs,
        getCountOnly,
        setCountOnly,
        getLastAiPattern,
        getLastAiPatternKind,
    } from './search-state.svelte'
    import QueryDialog from '$lib/query-ui/QueryDialog.svelte'
    import ImageSearchResults from './ImageSearchResults.svelte'
    import CoverageNote from './CoverageNote.svelte'
    import { translateAi } from './ai-translate'
    import { createSearchLifecycle } from './search-lifecycle.svelte'
    import { createSearchRunners } from './search-runners'
    import { createCoverageCta } from './coverage-cta.svelte'
    import { persistRecentSearch, promoteResultsToPane } from './snapshot-promotion'
    import {
        activateHistoryEntry,
        removeHistoryEntry,
        searchRecentAdapter,
        searchRecentKey,
    } from './recent-search-adapter'
    import type { SearchTargetVolume } from './search-target-volume'
    import { getBadgeStatus } from '$lib/feature-status'
    import type {
        QueryDialogConfig,
        QueryDialogFilterChipsExtras,
    } from '$lib/query-ui/query-dialog-config'
    import { loadRecentSearches, recentSearchesStore } from './recent-searches-state.svelte'
    import { setSearchReopener } from './walk-handoff-state.svelte'
    import type { LiveRunView } from '$lib/query-ui/query-stream'

    interface Props {
        /** Called when user selects a result: receives the full path. */
        onNavigate: (path: string) => void
        /** Called when dialog is closed. */
        onClose: () => void
        /**
         * The focused pane's two scope presets: its current folder (the Search-in popover's
         * `Use current folder` button, AND what an empty scope box means) and the volume that
         * folder lives on (`This volume`, the widest a search can go). When the pane is a
         * `search-results://` snapshot with no real folder behind it, `currentFolder` is
         * `null`, the button renders disabled with its tooltip, and the default falls back to
         * the volume. See `lib/search/searchable-folder.ts`.
         */
        scopePresets: ScopePresets
        /**
         * Called when the user activates "Show all in main window" (⌥⏎ or footer click).
         * Receives the freshly-created snapshot id; the host
         * (`+page.svelte` → `DualPaneExplorer`) routes the active pane to
         * `search-results://<id>`. The wrapper closes itself; the handler doesn't need to.
         */
        onShowAllInMainWindow?: (snapshotId: string) => void
        /**
         * Reopens this dialog after it closed. Wired to the toast's "Reopen search"
         * button, for the case the dialog can't handle itself: "Open in pane" left a
         * walk running, and by the time someone wants back in, this component is gone
         * and only the host still has the flag.
         */
        onReopen?: () => void
        /**
         * Routes into the Full Disk Access setup (the onboarding wizard's step 1,
         * reused rather than duplicated). Offered from the coverage note when a walk
         * was REFUSED a folder and Cmdr doesn't have the permission yet; the host
         * owns it because the wizard lives above this dialog and this dialog is on
         * its way out when it fires.
         */
        onGrantFullDiskAccess?: () => void
        /**
         * The ONE volume this session covers: the focused pane's current volume. It names
         * the arena the readiness gate waits for, the drive the coverage note speaks
         * about, and the media index the image-OCR grid queries (so browsing a NAS
         * surfaces its photos and browsing local surfaces local), plus the mount root that
         * turns index-relative hits back into openable OS paths. Defaults to the local
         * root, the same fallback the scope makes.
         */
        searchVolume?: SearchTargetVolume
    }

    const {
        onNavigate,
        onClose,
        scopePresets,
        onShowAllInMainWindow,
        onReopen,
        onGrantFullDiskAccess,
        searchVolume = { volumeId: ROOT_VOLUME_ID, mountRoot: '/', isNetwork: false },
    }: Props = $props()

    // Live mirror of the AI provider setting. Drives `aiEnabled` reactively so toggling
    // in the settings window flips the AI chip in real time without reopening the dialog.
    let aiProvider = $state<string>(getSetting('ai.provider'))
    let unlistenAiProvider: (() => void) | undefined

    /**
     * Where a search runs when the user hasn't set a scope: the focused pane's current
     * folder, or its volume when there's no real folder behind the pane. Derived, never
     * written into `scope` state — which is what keeps a defaulted scope out of saved
     * recent searches (see `snapshot-promotion.ts`).
     */
    const defaultScope = $derived(resolveDefaultScope(scopePresets))

    /**
     * The live run in flight, for the ONE thing that needs it after the dialog is
     * done with it: handing the walk to a pane and letting it keep going. Plain, not
     * `$state` — it changes on every batch, and making it reactive would rebuild the
     * dialog's whole config ten times a second for a value nothing renders.
     */
    let liveRun: { runId: string; view: LiveRunView } | null = null

    /**
     * The run this dialog handed to a pane, held right here so the close can name it.
     *
     * ❌ Don't replace this with a lookup at teardown time. It was one, and in the
     * running app the lookup answered `null` while every unit test passed: the close
     * cancelled the very walk the pane was being fed by, the pane froze at whatever had
     * arrived, and the toast went on saying "still searching" over it. A plain local set
     * on the way out has no resolution or ordering to get wrong.
     */
    let handedOffRun: string | null = null

    const lifecycle = createSearchLifecycle({
        getSearchVolumeId: () => searchVolume.volumeId,
        getHandedOffRunId: () => handedOffRun,
    })

    const runners = createSearchRunners({
        getDefaultScopePath: () => defaultScope.path,
        onRunState: (state) => {
            liveRun = state
        },
    })

    const coverage = createCoverageCta({
        getGrantFullDiskAccess: () => onGrantFullDiskAccess,
        closeDialog: () => {
            onClose()
        },
    })

    const aiEnabled = $derived(aiProvider !== 'off' && lifecycle.isIndexAvailable)
    const inputsDisabled = $derived(!lifecycle.isIndexAvailable)

    /**
     * "Show all in main window" (⌥⏎): promote the results into a pane, hand the id to the
     * host, and close. State is preserved (the module-level `$state` survives unmount), so
     * reopening with ⌘F lands the user back on the same results.
     */
    function showAllInMainWindow(): void {
        const promotion = promoteResultsToPane(liveRun)
        if (!promotion) return
        if (promotion.handedOffRunId !== null) handedOffRun = promotion.handedOffRunId
        onShowAllInMainWindow?.(promotion.snapshotId)
        onClose()
    }

    /**
     * "Go to file" (⏎ / click / button when results are present): persist the search,
     * then close the dialog and route the active pane to the cursor row. The host's
     * `onNavigate(path)` handles closing the dialog, navigating to the parent folder, and
     * focusing the file (pushing a history entry).
     */
    function goToCursorFile(entry: SearchResultEntry): void {
        persistRecentSearch()
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

    // QueryDialog already wrote the chip's query + mode into state and triggered the
    // run. Search has no per-chip side effects, so this hook is a no-op for now.
    const pickExample = (): void => {}

    /** Open an image tile: route the active pane to the file, same exit as a result click. */
    function openImage(path: string): void {
        onNavigate(path)
    }

    onMount(() => {
        // Live-mirror `ai.provider` so the AI chip appears / disappears in real time when
        // the user changes the provider in the settings window.
        unlistenAiProvider = onSpecificSettingChange('ai.provider', (_id, value: unknown) => {
            aiProvider = typeof value === 'string' ? value : 'off'
        })
        // How the handed-off walk's toast gets back here. Registered rather than
        // passed, because by the time the button is pressed this component is gone and
        // only the host still has the flag that mounts it.
        setSearchReopener(onReopen ?? null)
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
        countOnly: getCountOnly(),
        scopePresets,
        defaultScope: { path: defaultScope.path, label: defaultScopeLabel(defaultScope.kind) },
        systemDirExcludeTooltip: lifecycle.systemDirExcludeTooltip,
        aiPattern: getLastAiPattern(),
        aiPatternKind: getLastAiPatternKind(),
        onToggleCaseSensitive: () => {
            setCaseSensitive(!getCaseSensitive())
        },
        onToggleExcludeSystemDirs: () => {
            setExcludeSystemDirs(!getExcludeSystemDirs())
        },
        onToggleCountOnly: () => {
            setCountOnly(!getCountOnly())
        },
        onSetScope: setScope,
        onClearAiPattern: clearAiPattern,
    })

    const config: QueryDialogConfig<HistoryEntry> = $derived({
        title: tString('search.dialog.title'),
        badge: getBadgeStatus('search'),
        dialogType: 'search',
        width: 'min(1080px, 80vw)',

        state: searchQueryState,

        aiEnabled,
        inputsDisabled,

        visibleChips: { size: true, date: true, scope: true, pattern: true },
        showPathColumn: true,

        runHintCopy: tString('search.runHint'),
        // The run button voices what Enter does that the debounce won't: reach past the
        // index into folders that aren't indexed yet (Decision 7).
        runTitleOverride: tString('search.runTitle'),

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
            indexEntryCount: lifecycle.indexEntryCount,
        },

        // Why this answer is short, rendered right above the results it qualifies.
        // Search-only: Selection matches a pane listing, so it has no coverage question.
        resultsNotice: coverageNotice,

        // The "text in images" OCR grid, rendered below the filename results. Search-only
        // (Selection passes no `resultsExtra`); the snippet owns its own data + lifecycle.
        resultsExtra: imageResults,

        filterChipsExtras,

        scanning: lifecycle.scanning,
        entriesScanned: lifecycle.entriesScanned,
        indexEntryCount: lifecycle.indexEntryCount,
        isIndexAvailable: lifecycle.isIndexAvailable,
        isIndexReady: lifecycle.isIndexReady,

        runQuery: runners.runQuery,
        streamingSource: runners.streamingSource,
        translateAi,

        primaryAction: {
            label: tString('search.action.showAll.label'),
            shortcutHint: '⌥⏎',
            tooltip: tString('search.action.showAll.tooltip'),
            ariaLabel: tString('search.action.showAll.label'),
            handler: showAllInMainWindow,
        },
        secondaryAction: {
            label: tString('search.action.goToFile.label'),
            shortcutHint: '⏎',
            tooltip: tString('search.action.goToFile.tooltip'),
            ariaLabel: tString('search.action.goToFile.label'),
            handler: goToCursorFile,
        },

        onPickPath: pickPath,
        onPickExample: pickExample,
        onRowMenu: openRowMenu,
        onActivateRecent: activateHistoryEntry,
        onRemoveRecent: removeHistoryEntry,

        onClose,

        onMount: lifecycle.setup,
        onDestroy: lifecycle.teardown,

        // ⌘N clears core + extras together (the Search facade). Search's facade is
        // the canonical reset surface; using `state.clearCore()` alone would leave
        // scope / excludeSystemDirs / AI label dangling.
        onClearState: clearSearchState,
    })
</script>

{#snippet coverageNotice()}
    <CoverageNote
        note={coverage.note}
        driveName={coverage.driveName}
        isNetwork={coverage.isNetwork}
        isIndexing={coverage.isIndexing}
        onIndexDrive={coverage.indexDrive}
        onSilenceDrive={coverage.silenceDrive}
        onGrantFullDiskAccess={coverage.grantFullDiskAccess}
    />
{/snippet}

{#snippet imageResults()}
    <ImageSearchResults
        query={getQuery()}
        volumeId={searchVolume.volumeId}
        mountRoot={searchVolume.mountRoot}
        isNetwork={searchVolume.isNetwork}
        active={true}
        onOpen={openImage}
    />
{/snippet}

<QueryDialog {config} />
