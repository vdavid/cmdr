<script lang="ts">
    /**
     * SelectionDialog: thin Selection-specific wrapper around the shared `QueryDialog`.
     *
     * Mirrors `lib/search/SearchDialog.svelte`'s shape — the Selection feature is the
     * second consumer of `QueryDialog`, not a fork. The wrapper owns:
     *
     *   - Building the `QueryDialogConfig` for Selection (title, max width, modes,
     *     primary action "Select these files" / "Deselect these files").
     *   - The matcher and the folder-name snapshot used as AI context.
     *   - Calling `commands.translateSelectionQuery` and applying the AI's filter writes.
     *   - The recent-selections history store + IPC writeback.
     *
     * QueryDialog owns everything else: overlay, keyboard contract, IME guard, auto-apply,
     * `lastDialogEvent` lifecycle, title bar, chip strip, AI prompt strip, results table,
     * recent-items footer + popover, and the empty state.
     *
     * Apply-on-commit semantics: the matcher runs against the SNAPSHOT taken at dialog
     * open (current pane listing). Pressing ⏎ runs the matcher fresh against that
     * snapshot and hands the matched indices to `applyIndicesToFocusedPane`. We do NOT
     * mutate the focused pane's selection while the user is previewing — apply happens
     * only on commit, matching Total Commander's "+" / "-" behaviour.
     */
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import type { FileEntry } from '$lib/file-explorer/types'
    import type { SearchResultEntry, SelectionHistoryEntry } from '$lib/tauri-commands'
    import {
        translateSelectionQuery,
        addRecentSelection as addRecentSelectionIpc,
        removeRecentSelection as removeRecentSelectionIpc,
        getRecentSelections as fetchRecentSelections,
        trackEvent,
    } from '$lib/tauri-commands'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import QueryDialog from '$lib/query-ui/QueryDialog.svelte'
    import type {
        QueryDialogConfig,
        QueryDialogFilterChipsExtras,
        AiTranslateResult,
    } from '$lib/query-ui/query-dialog-config'
    import {
        createQueryFilterState,
        type QueryFilterState,
        bytesToSize,
    } from '$lib/query-ui/query-filter-state.svelte'
    import {
        chipTooltip,
        modeName,
        formatAge,
    } from '$lib/query-ui/recent-items/recent-items-utils'
    import type {
        RecentItemAdapter,
        RecentItemKey,
    } from '$lib/query-ui/recent-items/recent-items-types'
    import { sampleFolderNames } from './folder-sampler'
    import {
        matchEntries,
        type SelectionMatchQuery,
        type SizePredicate,
        type DatePredicate,
        type MatchAccessors,
    } from './selection-matching'
    import {
        recentSelectionsStore,
        loadRecentSelections,
        getRecentSelectionsList,
        setRecentSelectionsList,
        applySelectionHistoryEntry,
    } from './selection-history-state.svelte'

    interface Props {
        /** `'add'` for "Select files…", `'remove'` for "Deselect files…". */
        mode: 'add' | 'remove'
        /**
         * Snapshot of the focused pane's entries at dialog open. We DON'T re-snapshot
         * on focused-pane change (rare, mouse-click on the other pane) per the plan
         * G15 contract: the user opened the dialog on a folder, they're filtering
         * that folder.
         */
        entries: FileEntry[]
        /** Cursor index inside `entries` at dialog open. Drives the AI folder sample's middle bucket. */
        cursorIndex: number
        /** True when the focused pane is a `search-results://` snapshot (R7 banner). */
        isSnapshotPane: boolean
        /** Commit handler: apply the matched indices to the focused pane (add / remove). */
        onCommit: (indices: number[], mode: 'add' | 'remove') => void
        onClose: () => void
    }

    const { mode, entries, cursorIndex, isSnapshotPane, onCommit, onClose }: Props = $props()

    // Selection has its OWN core state — separate factory instance from Search's. Two
    // dialogs can never leak into each other.
    const selectionQueryState: QueryFilterState = createQueryFilterState({ defaultMode: 'filename' })

    // Live AI-provider mirror. Selection's AI is cloud-only (small local models can't
    // reliably handle a 200+-name folder sample). Hide the AI chip unless cloud is set.
    let aiProvider = $state<string>(getSetting('ai.provider'))
    let unlistenAiProvider: (() => void) | undefined
    const aiEnabled = $derived(aiProvider === 'cloud')

    // Snapshot of the folder names at dialog open. Used by the AI context callback;
    // doesn't refresh on focused-pane change (G15).
    const folderNamesSnapshot: string[] = entries.map((e) => e.name)

    // R7 banner: snapshot panes match against the full friendly path, not the basename.
    const noticeBanner = isSnapshotPane
        ? 'Matching what is shown in the list (the full path).'
        : undefined

    /**
     * Pre-translate (and re-export) the AI translation result into the matcher's
     * predicate shape. Mirrors Search's `applySizeFilters` / `applyDateFilters` but
     * writes to Selection's own state instance.
     */
    function applySizeFromAi(min: number | null, max: number | null): boolean {
        if (min == null && max == null) return false
        if (min != null && max != null) {
            selectionQueryState.setSizeFilter('between')
            const lo = bytesToSize(min)
            const hi = bytesToSize(max)
            selectionQueryState.setSizeValue(lo.value)
            selectionQueryState.setSizeUnit(lo.unit)
            selectionQueryState.setSizeValueMax(hi.value)
            selectionQueryState.setSizeUnitMax(hi.unit)
        } else if (min != null) {
            selectionQueryState.setSizeFilter('gte')
            const lo = bytesToSize(min)
            selectionQueryState.setSizeValue(lo.value)
            selectionQueryState.setSizeUnit(lo.unit)
        } else if (max != null) {
            selectionQueryState.setSizeFilter('lte')
            const hi = bytesToSize(max)
            selectionQueryState.setSizeValue(hi.value)
            selectionQueryState.setSizeUnit(hi.unit)
        }
        return true
    }

    function applyDateFromAi(after: string | null, before: string | null): boolean {
        if (after == null && before == null) return false
        if (after != null && before != null) {
            selectionQueryState.setDateFilter('between')
            selectionQueryState.setDateValue(after)
            selectionQueryState.setDateValueMax(before)
        } else if (after != null) {
            selectionQueryState.setDateFilter('after')
            selectionQueryState.setDateValue(after)
        } else if (before != null) {
            selectionQueryState.setDateFilter('before')
            selectionQueryState.setDateValue(before)
        }
        return true
    }

    /**
     * AI translation: hands the prompt plus a sampled folder listing to the Rust
     * cloud-only IPC and applies the result. QueryDialog owns the prompt + caveat
     * writes per the ownership contract.
     */
    async function translateAi(prompt: string): Promise<AiTranslateResult | null> {
        const sample = sampleFolderNames(folderNamesSnapshot, cursorIndex)
        let result: Awaited<ReturnType<typeof translateSelectionQuery>>
        try {
            result = await translateSelectionQuery(prompt, sample)
        } catch {
            return null
        }
        const changed = new SvelteSet<string>()
        if (result.pattern != null && result.pattern.trim()) {
            const kind: 'glob' | 'regex' = result.kind === 'regex' ? 'regex' : 'glob'
            // Core's `recordAiTranslation` writes the AI pattern into the matching
            // hand-typed buffer so a later mode switch (⌘1 / ⌘2 / ⌘3) restores it.
            // Selection has no extras module, so this is the only AI write.
            // Clear the OTHER kind's buffer first: `buildMatchQuery` in AI mode picks
            // whichever buffer has content (regex first, glob second), so a stale
            // value from a previous AI run of a different kind would shadow the
            // new pattern. The hand-typed value the user wrote in the non-AI mode
            // chip is preserved on a per-session basis only when the user actually
            // typed it there — AI's own previous output isn't worth keeping around.
            const otherKind: 'filename' | 'regex' = kind === 'regex' ? 'filename' : 'regex'
            selectionQueryState.setHandTypedBuffer(otherKind, '')
            selectionQueryState.recordAiTranslation({ pattern: result.pattern, kind })
            changed.add('pattern')
        }
        // Size + date filters: writing only when AI returns a non-null value means a
        // previous AI run's filter would otherwise leak into the next. Reset both
        // chips to `any` first so each AI run paints from a clean slate. The user's
        // own manual filter edits between AI runs are still wiped by this; that's the
        // right call — a user who runs AI again expects the AI's filter set, not a
        // merge with their last manual tweak.
        selectionQueryState.setSizeFilter('any')
        selectionQueryState.setDateFilter('any')
        if (applySizeFromAi(result.sizeMin, result.sizeMax)) changed.add('size')
        if (applyDateFromAi(result.modifiedAfter, result.modifiedBefore)) changed.add('date')
        return {
            caveat: result.caveat,
            highlightedFields: Array.from(changed),
        }
    }

    /**
     * Builds a `SelectionMatchQuery` from current state. AI mode hands the pattern
     * via the hand-typed buffer for the kind the AI produced (filename buffer for
     * glob, regex buffer for regex). Filename mode reads from the bar; regex mode
     * reads from the bar.
     */
    function buildMatchQuery(): SelectionMatchQuery | null {
        const m = selectionQueryState.getMode()
        let pattern: string
        let kind: 'glob' | 'regex'
        if (m === 'regex') {
            pattern = selectionQueryState.getQuery()
            kind = 'regex'
        } else if (m === 'filename') {
            pattern = selectionQueryState.getQuery()
            kind = 'glob'
        } else {
            // AI mode: the bar shows the natural-language prompt; the matcher needs
            // the AI's produced pattern. We stash it in handTyped[filename|regex]
            // via `recordAiTranslation`; pick whichever has content.
            const aiGlob = selectionQueryState.getHandTypedBuffer('filename')
            const aiRegex = selectionQueryState.getHandTypedBuffer('regex')
            if (aiRegex && aiRegex.trim()) {
                pattern = aiRegex
                kind = 'regex'
            } else if (aiGlob && aiGlob.trim()) {
                pattern = aiGlob
                kind = 'glob'
            } else {
                return null
            }
        }
        if (!pattern.trim()) return null

        const size = readSizePredicate()
        const date = readDatePredicate()
        const q: SelectionMatchQuery = {
            pattern,
            kind,
            caseSensitive: selectionQueryState.getCaseSensitive(),
        }
        if (size) q.size = size
        if (date) q.date = date
        return q
    }

    function readSizePredicate(): SizePredicate | undefined {
        const f = selectionQueryState.getSizeFilter()
        if (f === 'any') return undefined
        const hf = selectionQueryState.readHistoryFilters()
        if (f === 'gte') return hf.sizeMin != null ? { kind: 'gte', min: hf.sizeMin } : undefined
        if (f === 'lte') return hf.sizeMax != null ? { kind: 'lte', max: hf.sizeMax } : undefined
        // between
        return { kind: 'between', min: hf.sizeMin ?? undefined, max: hf.sizeMax ?? undefined }
    }

    function readDatePredicate(): DatePredicate | undefined {
        const f = selectionQueryState.getDateFilter()
        if (f === 'any') return undefined
        const hf = selectionQueryState.readHistoryFilters()
        const after = hf.modifiedAfter ? Math.floor(new Date(hf.modifiedAfter).getTime() / 1000) : undefined
        const before = hf.modifiedBefore
            ? Math.floor(new Date(hf.modifiedBefore).getTime() / 1000)
            : undefined
        if (f === 'after') return after != null ? { kind: 'after', after } : undefined
        if (f === 'before') return before != null ? { kind: 'before', before } : undefined
        return { kind: 'between', after, before }
    }

    /**
     * Adapter from `FileEntry` (pane snapshot) into the `SearchResultEntry` shape
     * `QueryResults` expects. Selection's preview list shows the matching entries
     * from the focused pane; the rendering reuses the same row component as Search.
     */
    function entryToResult(e: FileEntry): SearchResultEntry {
        return {
            name: e.name,
            path: e.path,
            parentPath: e.parentPath ?? '',
            isDirectory: e.isDirectory,
            size: e.size ?? null,
            modifiedAt: e.modifiedAt ?? null,
            iconId: e.iconId,
        }
    }

    /** Last matched-indices set from the most recent `runQuery`. Pinned so `primaryAction.handler` can commit. */
    let lastMatchedIndices: number[] = []

    /**
     * Drops the synthetic `..` parent entry from the matched set. For regular
     * panes with a parent dir, `getEntriesSnapshot` prepends a synthetic entry
     * named `..` at index 0 to keep indices aligned with the pane's selection
     * state. A pattern like `*` matches it; the underlying `applyIndices`
     * already skips it on commit (`hasParent` gate), but the preview list and
     * the matched-count must drop it too so the user sees an honest count and
     * the row never appears in the results table.
     */
    function dropParentIndex(idxs: number[]): number[] {
        if (idxs.length === 0) return idxs
        if (idxs[0] === 0 && entries.length > 0 && entries[0].name === '..') {
            return idxs.slice(1)
        }
        return idxs
    }

    /**
     * Runs the matcher against the dialog-time snapshot and returns the matched
     * entries adapted into `SearchResultEntry`. QueryDialog handles writing state.
     */
    function runQuery(): Promise<{ entries: SearchResultEntry[]; totalCount: number }> {
        const q = buildMatchQuery()
        if (!q) {
            lastMatchedIndices = []
            return Promise.resolve({ entries: [], totalCount: 0 })
        }
        const accessors: MatchAccessors = {
            getNameFor: (i) => entries[i].name,
            getSizeFor: (i) => entries[i].size,
            getMtimeFor: (i) => entries[i].modifiedAt,
        }
        const idxs = dropParentIndex(matchEntries(accessors, entries.length, q))
        lastMatchedIndices = idxs
        const adapted = idxs.map((i) => entryToResult(entries[i]))
        return Promise.resolve({ entries: adapted, totalCount: adapted.length })
    }

    /**
     * Primary action: commit the matched indices and close. Re-runs the matcher
     * one more time to pick up any tweak the user made since the last preview
     * pass (R7 G15 snapshot-mutation note: the snapshot is the SAME entries array
     * we captured at dialog open, but the user may have tweaked the query right
     * before pressing Enter).
     */
    function commitMatches(): void {
        const q = buildMatchQuery()
        let indices = lastMatchedIndices
        if (q) {
            const accessors: MatchAccessors = {
                getNameFor: (i) => entries[i].name,
                getSizeFor: (i) => entries[i].size,
                getMtimeFor: (i) => entries[i].modifiedAt,
            }
            indices = dropParentIndex(matchEntries(accessors, entries.length, q))
        }
        // Persist to recent selections. Selection's "add" gate is the commit, mirroring
        // Search's "Open in pane" gate (recents are signal-rich, not keystroke-noisy).
        void persistRecent(indices.length)
        // PII-free analytics: the dialog committed. Only the match-mode enum and the add/remove
        // action cross; never the pattern.
        void trackEvent('select_files_used', { mode: selectionQueryState.getMode(), action: mode })
        onCommit(indices, mode)
        onClose()
    }

    /** Persists the current run into recent-selections (best-effort; UI doesn't block). */
    async function persistRecent(matchCount: number): Promise<void> {
        const m = selectionQueryState.getMode()
        // For AI entries, the query in the history is the natural-language prompt
        // (matches Search's convention). For non-AI, it's the bar's typed pattern.
        const aiPrompt = selectionQueryState.getLastAiPrompt()
        const query =
            m === 'ai' && aiPrompt ? aiPrompt : selectionQueryState.getQuery()
        if (!query.trim()) return
        const entry: SelectionHistoryEntry = {
            id: crypto.randomUUID(),
            timestamp: Date.now(),
            mode: m,
            query,
            filters: selectionQueryState.readHistoryFilters(),
            caseSensitive: selectionQueryState.getCaseSensitive(),
            matchCount,
        }
        try {
            await addRecentSelectionIpc(entry)
            // Refresh the in-memory list so the footer sees the new entry.
            const fresh = await fetchRecentSelections()
            setRecentSelectionsList(fresh)
        } catch {
            // Silent: a missing recent-selection isn't worth blocking the commit.
        }
    }

    /** Activates a recent-selection: load it into state and trigger a fresh run on next tick. */
    function activateHistoryEntry(entry: SelectionHistoryEntry): void {
        applySelectionHistoryEntry(selectionQueryState, entry)
        // The dialog's `runOnMount` $effect picks this up and dispatches to AI / non-AI
        // based on the restored mode.
        selectionQueryState.setRunOnMount(true)
    }

    /** Removes a recent-selection entry. Optimistic update; refetches after the IPC. */
    function removeHistoryEntry(entry: SelectionHistoryEntry): void {
        setRecentSelectionsList(getRecentSelectionsList().filter((e) => e.id !== entry.id))
        void removeRecentSelectionIpc(entry.id).then(async () => {
            try {
                setRecentSelectionsList(await fetchRecentSelections())
            } catch {
                // Already fell back to the optimistic snapshot; nothing to do.
            }
        })
    }

    /**
     * Mid-dialog AI-provider fallback. If the user's provider switches off while
     * the dialog is open AND we're on AI mode, drop to filename and hand the
     * pending AI prompt to the filename buffer so the user keeps their work.
     * Mirrors Search's existing fallback in `QueryDialog.svelte` (`config.aiEnabled`
     * effect), but Selection has a tighter gate (`provider === 'cloud'`), so we
     * handle it here so the prompt is preserved.
     */
    $effect(() => {
        if (!aiEnabled && selectionQueryState.getMode() === 'ai') {
            const prompt = selectionQueryState.getQuery()
            // Carry the prompt into the filename buffer so the user can refine
            // (or just delete) it without losing the words they typed. Set BEFORE
            // `switchMode` so `switchMode`'s buffer restore picks up the prompt.
            if (prompt) selectionQueryState.setHandTypedBuffer('filename', prompt)
            selectionQueryState.switchMode('filename')
        }
    })

    // ─────────────────────────────────────────────────────────────────────────
    // Recent-items adapter
    // ─────────────────────────────────────────────────────────────────────────

    /**
     * Per-chip view: Selection's entry shape lacks `scope` and `excludeSystemDirs`
     * so we reuse Search's helpers by adapting up to the wider shape. The chip
     * itself never reads the missing fields (they're noise in the tooltip).
     */
    const recentAdapter: RecentItemAdapter<SelectionHistoryEntry> = (entry) => {
        const widened = {
            ...entry,
            scope: '',
            excludeSystemDirs: true,
            resultCount: entry.matchCount,
        }
        return {
            label: entry.query,
            tooltip: chipTooltip(widened),
            mode: entry.mode,
            ageLabel: formatAge(entry.timestamp),
            ariaLabel: `Apply recent ${modeName(entry.mode)} selection: ${entry.query}`,
        }
    }
    const recentKey: RecentItemKey<SelectionHistoryEntry> = (entry) => entry.id

    // ─────────────────────────────────────────────────────────────────────────
    // Lifecycle: AI-provider subscription. Recent-selections load goes through
    // the config's `onLoadHistory` hook (QueryDialog calls it once on mount).
    // ─────────────────────────────────────────────────────────────────────────

    onMount(() => {
        unlistenAiProvider = onSpecificSettingChange('ai.provider', (_id, value: unknown) => {
            aiProvider = typeof value === 'string' ? value : 'off'
        })
    })

    onDestroy(() => {
        unlistenAiProvider?.()
        unlistenAiProvider = undefined
    })

    // ─────────────────────────────────────────────────────────────────────────
    // QueryDialogConfig: $derived so live changes (AI provider toggle, etc.)
    // propagate into QueryDialog without remounting.
    // ─────────────────────────────────────────────────────────────────────────

    // Selection has no scope / excludeSystemDirs / AI pattern chip. We pass
    // empty values / no-op handlers because `FilterChips.svelte` reads the
    // props unconditionally (gated by visibility) and TypeScript wants them
    // typed even though `scopeChipVisible: false` keeps them invisible.
    const filterChipsExtras: QueryDialogFilterChipsExtras = $derived({
        caseSensitive: selectionQueryState.getCaseSensitive(),
        scope: '',
        excludeSystemDirs: true,
        searchableFolder: { path: null, disabled: true, disabledReason: '' },
        systemDirExcludeTooltip: '',
        aiPattern: null,
        onToggleCaseSensitive: () => {
            selectionQueryState.setCaseSensitive(!selectionQueryState.getCaseSensitive())
        },
        onToggleExcludeSystemDirs: () => {},
        onSetScope: () => {},
        onClearAiPattern: () => {},
    })

    const primaryLabel = $derived(mode === 'add' ? 'Select these files' : 'Deselect these files')
    const title = $derived(mode === 'add' ? 'Select files' : 'Deselect files')

    const config: QueryDialogConfig<SelectionHistoryEntry> = $derived({
        title,
        dialogType: mode === 'add' ? 'selection-add' : 'selection-remove',
        maxWidth: 'min(720px, 60vw)',

        state: selectionQueryState,

        aiEnabled,
        inputsDisabled: false,

        visibleChips: { size: true, date: true, scope: false, pattern: true },
        showPathColumn: false,

        runHintCopy: 'Press Enter to filter',

        historyStore: recentSelectionsStore,
        recentItems: {
            adapter: recentAdapter,
            keyFn: recentKey,
            leadingLabel: 'Recent selections:',
            trailingLabel: 'All selections… ⌘H',
            trailingTooltipText: 'Open the recent-selections popover',
            trailingShortcut: '⌘H',
            ariaRegionLabel: 'Recent selections',
            ariaAllButtonLabel: 'Open all recent selections',
            filterPlaceholder: 'Filter recent selections',
            emptyMessage: 'No matching recent selections',
            popoverAriaLabel: 'Recent selections',
            listboxAriaLabel: 'Recent selections',
        },
        onLoadHistory: async () => {
            await loadRecentSelections()
        },

        emptyState: {
            // QueryDialog reads these to render the empty-state "Try…" block.
            // The plan seeds AI + paired non-AI examples; refined here per § "Empty state copy".
            examples: aiEnabled
                ? [
                      { label: 'all image files', mode: 'ai', query: 'all image files' },
                      { label: 'logs newer than a week', mode: 'ai', query: 'logs newer than a week' },
                      { label: 'files bigger than 5 MB', mode: 'ai', query: 'files bigger than 5 MB' },
                  ]
                : [
                      { label: '*.{jpg,png,gif}', mode: 'filename', query: '*.{jpg,png,gif}' },
                      { label: '*.log', mode: 'filename', query: '*.log' },
                      { label: '*backup*', mode: 'filename', query: '*backup*' },
                  ],
        },

        filterChipsExtras,

        scanning: false,
        entriesScanned: 0,
        indexEntryCount: entries.length,
        isIndexAvailable: true,
        isIndexReady: true,

        noticeBanner,

        runQuery,
        translateAi: aiEnabled ? translateAi : undefined,

        primaryAction: {
            label: primaryLabel,
            shortcutHint: '⏎',
            tooltip: `${primaryLabel} in the focused pane`,
            ariaLabel: primaryLabel,
            handler: commitMatches,
        },
        // Selection has no secondary action — ⏎ commits, ⌥⏎ also commits via the
        // primary handler path (QueryDialog routes ⌥⏎ to the primary action; Search
        // uses ⌥⏎ for "Show all in main window", Selection reuses it for parity).

        onPickPath: () => {
            // Selection runs against the focused pane's current folder — there's no
            // "navigate to ancestor" path to surface from the result row, so the
            // path-pill click is a no-op.
        },
        onPickExample: () => {},
        onRowMenu: () => {
            // No row-level context menu in Selection's preview list; the row's
            // representation is purely informational.
        },
        onActivateRecent: activateHistoryEntry,
        onRemoveRecent: removeHistoryEntry,

        onClose,

        // Selection has no per-dialog backend lifecycle (no index prepare/release).
    })
</script>

<QueryDialog {config} />
