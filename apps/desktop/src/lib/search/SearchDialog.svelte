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
     *     `releaseSearchIndex` on destroy, plus the `search-index-ready` listener) and
     *     the PER-TARGET readiness gate built on it.
     *   - Owns the coverage note: what the last run couldn't cover, and the per-drive
     *     indexing offer that answers it.
     *   - Owns the "Open in pane" snapshot promotion path: minting an id, populating the
     *     snapshot store, pinning the last-attempt ref, persisting to recent searches,
     *     handing the id to the host.
     *   - Loads the system-dir exclude tooltip.
     *   - Provides recent-searches pick + remove handlers, including the IPC write-back on
     *     removal.
     *
     * QueryDialog owns everything else: overlay, keyboard contract, IME guard, auto-apply
     * gates, `deriveEnterAction` ownership, `lastDialogEvent` lifecycle, title bar, the
     * chip strip, the AI prompt strip, the results table, the recent-items dropdown, and
     * the empty state.
     */
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { applySizeFromAi, applyDateFromAi, applyTypeFromAi } from '$lib/query-ui/apply-ai-filters'
    import { typeFilterToIsDirectory } from '$lib/query-ui/query-filter-state.svelte'
    import {
        prepareSearchIndex,
        searchFiles,
        releaseSearchIndex,
        translateSearchQuery,
        parseSearchScope,
        getSystemDirExcludes,
        checkFullDiskAccessQuiet,
        onSearchIndexReady,
        showFileContextMenu,
        trackEvent,
        getRecentSearches as fetchRecentSearches,
        removeRecentSearch as removeRecentSearchIpc,
        addRecentSearch as addRecentSearchIpc,
        type HistoryEntry,
        type SearchResultEntry,
        type TranslateResult,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { isDriveSilenced, silenceDrive } from '$lib/indexing/drive-index-prefs'
    import { resolveDefaultScope, defaultScopeLabel } from './searchable-folder'
    import type { ScopePresets } from '$lib/query-ui/query-dialog-config'
    import { tString } from '$lib/intl/messages.svelte'
    import { isVolumeScanning, getEntriesScanned, ROOT_VOLUME_ID } from '$lib/indexing'
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
        getCountOnly,
        setCountOnly,
        getResults,
        getTotalCount,
        getLastAiPrompt,
        getLastAiLabel,
        getLastAiPattern,
        getLastAiPatternKind,
        getSizeFilter,
        getDateFilter,
        recordAiTranslation,
        isVolumeIndexReady,
        markVolumeIndexReady,
        getVolumeEntryCount,
        getPendingIndexVolumeId,
        setPendingIndexVolumeId,
        getIsIndexAvailable,
        setIsIndexAvailable,
        getCoverageNote,
        setCoverageNote,
    } from './search-state.svelte'
    import QueryDialog from '$lib/query-ui/QueryDialog.svelte'
    import ImageSearchResults from './ImageSearchResults.svelte'
    import CoverageNote from './CoverageNote.svelte'
    import {
        coverageNoteFrom,
        coverageNoteFromRun,
        isTargetIndexReady,
        offersFullDiskAccess,
    } from './coverage-note'
    import { createLiveSearchSource } from './live-search-source'
    import { rankLiveResults } from './live-ranking'
    import { indexUncoveredDrive } from './coverage-actions'
    import { describeVolume, type SearchTargetVolume } from './search-target-volume'
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
        rowMeta,
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
    import { handOffWalk, handedOffRunId } from './walk-handoff.svelte'
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

    // Index-readiness listener cleanup. Lives on the wrapper because the listener is
    // Search-specific (Selection has no whole-drive index).
    let unlistenReady: UnlistenFn | undefined

    // System-dir exclude tooltip (populated async on mount; renders the full exclude list).
    let systemDirExcludeTooltip = $state(tString('search.systemDirExclude.default'))

    // Live mirror of the AI provider setting. Drives `aiEnabled` reactively so toggling
    // in the settings window flips the AI chip in real time without reopening the dialog.
    let aiProvider = $state<string>(getSetting('ai.provider'))
    let unlistenAiProvider: (() => void) | undefined

    /**
     * Where a search runs when the user hasn't set a scope: the focused pane's current
     * folder, or its volume when there's no real folder behind the pane. Derived, never
     * written into `scope` state — which is what keeps a defaulted scope out of saved
     * recent searches (see `persistRecentSearch`).
     */
    const defaultScope = $derived(resolveDefaultScope(scopePresets))

    /**
     * The volume this run will land on, or `null` when only the backend can say.
     *
     * An unset scope resolves to the focused pane's folder, which is on the pane's own
     * volume — that's the default and the common case. A scope the user typed or clicked
     * in can point anywhere, and routing a path to a volume is the backend's job (an SMB
     * id keys on the address; cloud drives route to `root`), so the honest answer here is
     * "unknown", which `isTargetIndexReady` reads as "run it".
     */
    const targetVolumeId = $derived(getScope().trim() === '' ? searchVolume.volumeId : null)

    // Reactive readers off the Search state instance. Used by the derived config below.
    /**
     * Whether a search may run now. PER TARGET: waiting is only right while a pre-load
     * for THIS volume is in flight. Gating on root instead left a machine with no root
     * index unable to search at all, since no `search-index-ready` was ever coming
     * (`docs/specs/unindexed-search-plan.md` M1).
     */
    const isIndexReady = $derived(
        isTargetIndexReady({
            targetVolumeId,
            isVolumeReady: isVolumeIndexReady,
            pendingVolumeId: getPendingIndexVolumeId(),
        }),
    )
    /** The target's own indexed entry count: 0 (so the empty state stays quiet) until its arena lands. */
    const indexEntryCount = $derived(targetVolumeId === null ? 0 : getVolumeEntryCount(targetVolumeId))
    const isIndexAvailable = $derived(getIsIndexAvailable())
    // Search reads the LOCAL index, so its "building index" state keys on `root`
    // only — a network (SMB/MTP) scan must not flip the label while root's
    // `entriesScanned` stays 0.
    const scanning = $derived(isVolumeScanning(ROOT_VOLUME_ID))
    const entriesScanned = $derived(getEntriesScanned())
    const aiEnabled = $derived(aiProvider !== 'off' && isIndexAvailable)
    const inputsDisabled = $derived(!isIndexAvailable)
    const lastAiPattern = $derived(getLastAiPattern())

    // ─────────────────────────────────────────────────────────────────────────
    // Coverage honesty: what the last run couldn't cover, and the offer that
    // answers it. `runSearch` writes the note; everything below reads it.
    // ─────────────────────────────────────────────────────────────────────────

    const coverageNote = $derived(getCoverageNote())
    /**
     * How to name the drive a gap belongs to. Looked up by the volume the BACKEND
     * routed to, not the pane's: a typed scope can point at another drive, and offering
     * to index the wrong one would be worse than saying nothing.
     */
    const coverageDrive = $derived(describeVolume(getVolumes(), coverageNote?.volumeId ?? ''))
    /**
     * Whether to offer indexing. Only for an UNCOVERED gap (an unresolved path sits on a
     * drive that's already indexed, so there's nothing to turn on), only for a drive we
     * can name in the live volume list, and never for one the user silenced — the
     * per-drive silence is exactly the "stop offering me this" they already gave, and
     * without honoring it the dialog nags on every search of that drive forever. The
     * NOTE still renders: silencing the offer doesn't make the gap untrue.
     */
    const coverageCtaVolumeId = $derived(
        coverageNote &&
            coverageNote.uncoveredScopes.length > 0 &&
            coverageNote.volumeId !== '' &&
            !isDriveSilenced(coverageNote.volumeId)
            ? coverageNote.volumeId
            : null,
    )

    /** "Don't ask again" for this drive: the same persisted silence the first-connect prompt honors. */
    function silenceUncoveredDrive(): void {
        if (coverageNote?.volumeId) silenceDrive(coverageNote.volumeId)
    }

    /**
     * Whether Cmdr currently has Full Disk Access. Starts at `true` so nothing is
     * offered before the probe answers: an offer that arrives and then vanishes is
     * worse than one that arrives a moment late, and "already granted" is the state
     * in which the offer would be useless anyway.
     */
    let hasFullDiskAccess = $state(true)

    /**
     * Ask the OS, but only when the answer could change what's on screen: a run that
     * was refused a folder. `checkFullDiskAccessQuiet` and NOT `checkFullDiskAccess`
     * — the loud one fires a TCC-registration storm on every denial, and this runs
     * per search (`lib/onboarding/CLAUDE.md`).
     */
    $effect(() => {
        if ((coverageNote?.live?.permissionDenied.length ?? 0) === 0) return
        if (!isMacOS()) return
        void checkFullDiskAccessQuiet().then((granted) => {
            hasFullDiskAccess = granted
        })
    })

    /**
     * The Full Disk Access route, or `null` when granting it would change nothing
     * (`coverage-note.ts::offersFullDiskAccess`). Closing first is deliberate: the
     * wizard is the app's modal and this dialog is a modal over it, and the user who
     * presses this is going to System Settings and then restarting.
     */
    const grantFullDiskAccess = $derived(
        onGrantFullDiskAccess &&
            offersFullDiskAccess({ note: coverageNote, isMac: isMacOS(), hasFullDiskAccess })
            ? () => {
                  onClose()
                  onGrantFullDiskAccess()
              }
            : null,
    )

    /**
     * Adapter from Search's `HistoryEntry` shape into the generic `RecentItemView` the
     * recent-items dropdown consumes. The adapter is the only seam where Search-specific
     * fields (`scope`, `excludeSystemDirs`, `caseSensitive`, etc.) leak into the row's meta
     * line and tooltip. Selection's wrapper passes its own adapter against its narrower
     * entry shape.
     */
    const searchRecentAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
        label: entry.query,
        tooltip: chipTooltip(entry),
        mode: entry.mode,
        ageLabel: formatAge(entry.timestamp),
        metaLabel: rowMeta(entry),
        ariaLabel: tString('search.recent.runAria', { mode: modeName(entry.mode), query: entry.query }),
    })
    const searchRecentKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

    /** Recovers the structured pattern kind ('glob' | 'regex' | null) from the AI display string. */
    function patternKindFromDisplay(patternType: string | null | undefined): 'glob' | 'regex' | null {
        if (patternType === 'regex') return 'regex'
        if (patternType === 'glob') return 'glob'
        return null
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
        // Hand the AI the user's current type as context so it can keep or change it.
        const currentType = typeFilterToIsDirectory(searchQueryState.getTypeFilter())
        const result = await translateSearchQuery(prompt, currentType)
        return {
            caveat: result.caveat,
            highlightedFields: applyAiTranslationToState(result),
        }
    }

    /**
     * Paints a translate result onto the Search state and returns the names of the chips that
     * changed (for the QueryDialog highlight flash). Split out of `translateAi`, and further
     * split into pattern-write vs filter-write halves, to keep each under the cognitive-complexity
     * ceiling.
     */
    function applyAiTranslationToState(result: TranslateResult): string[] {
        const changed = new SvelteSet<string>()
        applyAiPatternAndToggles(result, changed)
        applyAiSharedFilters(result.display, changed)
        return Array.from(changed)
    }

    /** Writes the produced pattern (+ label), case sensitivity, and the system-dir toggle. */
    function applyAiPatternAndToggles(result: TranslateResult, changed: SvelteSet<string>): void {
        const { display, query } = result
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
        if (applyScopeFromAi(query.includePaths ?? null, query.excludeDirNames ?? null)) changed.add('scope')
    }

    /** Writes the shared Size / Modified / Type filters via the cross-consumer helpers. */
    function applyAiSharedFilters(display: TranslateResult['display'], changed: SvelteSet<string>): void {
        // Reset size + date to `any` before applying the AI's bounds. `applySizeFromAi` /
        // `applyDateFromAi` no-op when the AI returns no bound, so without this a previous run's
        // size/date filter would silently leak into a run that didn't return one. Selection does
        // the same; the contract lives in `apply-ai-filters.ts`. The user's own manual filter edit
        // between runs is wiped too, which is the right call (running AI again means "give me the
        // AI's filter set", not a merge with a stale manual tweak).
        searchQueryState.setSizeFilter('any')
        searchQueryState.setDateFilter('any')
        if (applySizeFromAi(searchQueryState, display.minSize ?? null, display.maxSize ?? null))
            changed.add('size')
        if (applyDateFromAi(searchQueryState, display.modifiedAfter ?? null, display.modifiedBefore ?? null))
            changed.add('date')
        // Type: leave-alone-if-null. The AI got the current type as context in `translateAi`;
        // it returns `isDirectory` only when it wants to change it, so a null leaves the user's
        // choice intact. Deliberately NOT reset-first like size/date (see `apply-ai-filters.ts`).
        if (applyTypeFromAi(searchQueryState, display.isDirectory ?? null)) changed.add('type')
    }

    /**
     * Builds the run's payload: the bar + filters + AI pattern off the Search state via
     * `buildSearchQuery()`, plus the scope, whose parse is async and so can't live in
     * there. Shared by the one-shot path (`runSearch`) and the live one, which have to
     * ask the same question or an auto-applied answer and an Enter-run one would differ
     * for reasons nobody could see.
     */
    async function buildRunQuery(): Promise<ReturnType<typeof buildSearchQuery>> {
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
        // Parse the scope and merge it in. An EMPTY box isn't "everywhere" any more: a
        // search covers one volume at most, and the default rung of that ladder is the
        // focused pane's current folder, resolved here at run time so it follows the pane.
        const scopeStr = getScope().trim()
        if (scopeStr) {
            const parsed = await parseSearchScope(scopeStr)
            if (parsed.includePaths.length > 0) query.includePaths = parsed.includePaths
            if (parsed.excludePatterns.length > 0)
                query.excludeDirNames = parsed.excludePatterns
        } else {
            query.includePaths = [defaultScope.path]
        }
        return query
    }

    /** PII-free analytics: a search ran. Only the mode enum crosses; never the query/pattern. */
    function trackSearchRun(): void {
        void trackEvent('search_used', { mode: getMode() })
    }

    /**
     * The one-shot path: the index's answer, in one promise. Auto-apply takes this
     * (Decision 7 — a debounced live walk would start and abandon a walk per keystroke);
     * every run the user asked for takes `liveSearchSource` instead.
     */
    async function runSearch(): Promise<{ entries: SearchResultEntry[]; totalCount: number }> {
        // The note belongs to the run that produced it. Dropping it up front (rather than
        // only on the way out) means a run that throws can't leave the previous run's
        // caveat sitting under a fresh answer.
        setCoverageNote(null)
        const query = await buildRunQuery()
        const result = await searchFiles(query)
        // Coverage honesty: an empty answer with a structural reason says so, instead of
        // reading as "nothing matched" (`search/DETAILS.md` § Honesty).
        setCoverageNote(coverageNoteFrom(result))
        trackSearchRun()
        return { entries: result.entries, totalCount: result.totalCount }
    }

    /**
     * The live path: the index's half, then a walk over what the index can't answer for,
     * arriving in batches. Its coverage answer lands in the same note the one-shot path
     * writes, so a caveat still can't outlive the run that earned it.
     */
    /**
     * The live run in flight, for the ONE thing that needs it after the dialog is
     * done with it: handing the walk to a pane and letting it keep going. Plain, not
     * `$state` — it changes on every batch, and making it reactive would rebuild the
     * dialog's whole config ten times a second for a value nothing renders.
     */
    let liveRun: { runId: string; view: LiveRunView } | null = null

    const liveSearchSource = createLiveSearchSource({
        buildQuery: buildRunQuery,
        onRunState: (state) => {
            liveRun = state
        },
        onCoverage: (coverage) => {
            setCoverageNote(coverage === null ? null : coverageNoteFromRun(coverage))
        },
        rank: (entries) =>
            rankLiveResults(entries, {
                query: getMode() === 'ai' ? (getLastAiPattern() ?? getQuery()) : getQuery(),
                mode: getMode(),
                caseSensitive: getCaseSensitive(),
            }),
        onStarted: trackSearchRun,
    })

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

        // The one case where a walk outlives its dialog: the results are about to be
        // on screen in a pane, so the walk keeps going and its rows keep landing
        // there. Everything after this — the toast, the snapshot appends, handing the
        // run back if the dialog reopens — belongs to `walk-handoff.svelte.ts`.
        if (liveRun) {
            handOffWalk({ runId: liveRun.runId, snapshotId: id, label, view: liveRun.view })
        }

        persistRecentSearch()

        onShowAllInMainWindow?.(id)
        onClose()
    }

    /**
     * Persists the current search to recent searches. Called whenever the user acts on a
     * result, treating it as a signal-rich event worth remembering: "Show all in main
     * window" AND opening a single result ("Go to file"). Plain Enter / auto-apply runs
     * don't persist (they'd be keystroke noise). For AI mode the entry carries the
     * original natural-language prompt, not the translated pattern. Best-effort: a
     * persistence failure never blocks the open.
     *
     * A DEFAULTED scope is deliberately not persisted: `scope` is `''` until the user sets
     * one, so the entry records "wherever I was" rather than baking in a machine-specific
     * absolute path nobody chose. Replaying it later re-resolves against the pane you're
     * standing in then, which is what "search here" meant in the first place. It also keeps
     * the history dedupe key meaningful (one "report" entry, not one per folder visited).
     */
    function persistRecentSearch(): void {
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
            // Silent on history persistence failure: the open still proceeds.
        })
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

    /**
     * Recent-search pick: loads the history entry's query, mode, and filters into the live
     * dialog and stops there. It deliberately does NOT set `runOnMount` — picking is
     * navigation, so the user lands back in the field with the search ready to tweak, and
     * an AI entry never re-translates (and re-bills) on a keystroke. QueryDialog closes the
     * dropdown, refocuses the field, and hands `⏎` back to "run-search".
     */
    function activateHistoryEntry(entry: HistoryEntry): void {
        applyHistoryEntry(entry)
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
        // Listen for a volume's arena landing. The event NAMES its volume, so readiness
        // is recorded per volume and only the search that targets that one un-gates.
        unlistenReady = await onSearchIndexReady((volumeId: string, entryCount: number) => {
            markVolumeIndexReady(volumeId, entryCount)
            if (getPendingIndexVolumeId() === volumeId) setPendingIndexVolumeId(null)
            // Auto-run the pending search if the user already typed something AND this is
            // the volume they're searching (filename/regex only; AI mode always waits for
            // an explicit Enter / ⌘Enter).
            const pendingMode = getMode()
            if (
                volumeId === targetVolumeId &&
                pendingMode !== 'ai' &&
                (getQuery().trim() || getSizeFilter() !== 'any' || getDateFilter() !== 'any')
            ) {
                // Trigger via the runOnMount flag; QueryDialog's effect dispatches to
                // the non-AI runner since mode !== 'ai'.
                searchQueryState.setRunOnMount(true)
            }
        })

        try {
            // Root is the one volume that gets pre-loaded when the dialog opens. `loading`
            // is the backend's promise that an event is coming; without it, a machine with
            // no root index would wait for one that never arrives and never search at all.
            const result = await prepareSearchIndex()
            if (result.ready) {
                markVolumeIndexReady(ROOT_VOLUME_ID, result.entryCount)
                setPendingIndexVolumeId(null)
            } else {
                setPendingIndexVolumeId(result.loading ? ROOT_VOLUME_ID : null)
            }
        } catch {
            // Index not available: indexing disabled, not started, or backend unavailable.
            setPendingIndexVolumeId(null)
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
                    `<div style="font-weight:600;margin-bottom:4px">${escapeHtml(tString('search.systemDirExclude.heading'))}</div>` +
                    items +
                    '</div>'
            })
            .catch(() => {})
    }

    function teardownSearchLifecycle(): void {
        // A handed-off walk is the one run the close must NOT stop: its results are in
        // a pane and still growing. Every other run in flight is a query nobody is
        // reading any more.
        releaseSearchIndex(handedOffRunId()).catch(() => {})
        unlistenReady?.()
        unlistenReady = undefined
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
        systemDirExcludeTooltip,
        aiPattern: lastAiPattern,
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
        maxWidth: 'min(1080px, 80vw)',

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
            indexEntryCount,
        },

        // Why this answer is short, rendered right above the results it qualifies.
        // Search-only: Selection matches a pane listing, so it has no coverage question.
        resultsNotice: coverageNotice,

        // The "text in images" OCR grid, rendered below the filename results. Search-only
        // (Selection passes no `resultsExtra`); the snippet owns its own data + lifecycle.
        resultsExtra: imageResults,

        filterChipsExtras,

        scanning,
        entriesScanned,
        indexEntryCount,
        isIndexAvailable,
        isIndexReady,

        runQuery: runSearch,
        streamingSource: liveSearchSource,
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

        onMount: setupSearchLifecycle,
        onDestroy: teardownSearchLifecycle,

        // ⌘N clears core + extras together (the Search facade). Search's facade is
        // the canonical reset surface; using `state.clearCore()` alone would leave
        // scope / excludeSystemDirs / AI label dangling.
        onClearState: clearSearchState,
    })
    /** Open an image tile: route the active pane to the file, same exit as a result click. */
    function openImage(path: string): void {
        onNavigate(path)
    }
</script>

{#snippet coverageNotice()}
    <CoverageNote
        note={coverageNote}
        driveName={coverageDrive.name}
        isNetwork={coverageDrive.isNetwork}
        onIndexDrive={coverageCtaVolumeId === null
            ? null
            : () => {
                  void indexUncoveredDrive(coverageCtaVolumeId, coverageDrive.name)
              }}
        onSilenceDrive={silenceUncoveredDrive}
        onGrantFullDiskAccess={grantFullDiskAccess}
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
