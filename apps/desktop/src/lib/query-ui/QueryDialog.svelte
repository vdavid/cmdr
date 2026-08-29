<script lang="ts" generics="E = unknown">
    /**
     * QueryDialog: the shared orchestrator for filter-and-act-on dialogs.
     *
     * Search is the first consumer; Selection is the second. Everything that diverges
     * per consumer comes in via `QueryDialogConfig`; everything else lives here as the
     * one source of truth for both consumers' polish (keyboard contract, IME guard,
     * auto-apply gates, `deriveEnterAction` ownership swap, `lastDialogEvent` lifecycle,
     * the title bar, the chip strip, the results table, the recent-items dropdown, the
     * empty state, the notice banner).
     *
     * Ownership contracts (see `query-dialog-config.ts` for the long version):
     *   1. `state.lastDialogEvent` is written ONLY here (opened / query-edited /
     *      filter-edited / cursor-moved / results-arrived). Consumers must not touch it.
     *   2. `state.lastAiPrompt` and `state.lastAiCaveat` are written ONLY here.
     *      QueryDialog captures the prompt before calling `config.translateAi` and the
     *      caveat after it resolves.
     *   3. `state.results` / `state.totalCount` / `state.cursorIndex` are written ONLY
     *      here, after `config.runQuery` resolves.
     *
     * Chrome comes from `ModalDialog` (standard radius, two-hairline panel edge, shadow,
     * title bar + × button, focus trap, MCP registry, focus restore). This component opts
     * into `align="top"`, `fillBody`, `resizable`, `padded={false}`, `ownsKeyboard`, and
     * `closeOnOverlayClick`; see DETAILS.md § Chrome. `config.width` is the width the panel
     * OPENS at, not a cap: an edge drag goes wider, which is how a user reads a long path
     * that the result rows shorten.
     *
     * Layout (top → bottom), three zones separated by surface + hairline:
     *   Zone 1 "what to look for": QueryBar, then ModeChips + the Count-only switch,
     *          then the AiPromptStrip / notice banner when present. On `--color-bg-primary`.
     *   Zone 2 "the filters": FilterChips (Type / Pattern / Size / Modified / Search in).
     *          On `--color-bg-secondary`, hairline top and bottom: the band that separates
     *          "how do I narrow this" from "here's what I found".
     *   Zone 3 "the results": QueryResults (column headers + rows + states + status bar).
     *          Back on `--color-bg-primary`, so the list reads as its own surface.
     *   Then the footer: the primary / secondary action buttons, right-aligned.
     *
     * Recent items are the query field's own dropdown (`RecentItemsPopover` anchored to the
     * pill), not a footer strip. Openers: the field's chevron, `⌘H`, and ArrowDown in the
     * field when there's no result list to walk. Picking a row LOADS the entry and closes;
     * it doesn't run it (the user presses Enter when they're ready).
     *
     * This file keeps the wiring and the layout; four sibling modules hold the logic, each
     * with its own unit tests: `query-runner.svelte.ts` (running queries), `recent-popover.svelte.ts`
     * (the dropdown's open flag + focus restore), `query-shortcuts.ts` (modifier-key routing),
     * and `result-actions.ts` (what ⏎ / clicks / footer buttons do with the results).
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import { iconCacheVersion } from '$lib/icon-cache'
    import QueryBar from './QueryBar.svelte'
    import ModeChips from './ModeChips.svelte'
    import FilterChips from './filter-chips/FilterChips.svelte'
    import QueryResults from './QueryResults.svelte'
    import AiPromptStrip from './AiPromptStrip.svelte'
    import { buildAiSummary, resolveAiPattern } from './ai-summary'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import RecentItemsPopover from './recent-items/RecentItemsPopover.svelte'
    import { deriveEnterAction, type SearchMode } from './query-filter-state.svelte'
    import type { QueryDialogConfig } from './query-dialog-config'
    import { createQueryRunner, hasRunnableQuery, shouldShowRunHint } from './query-runner.svelte'
    import { createRecentPopover } from './recent-popover.svelte'
    import { routeModifierShortcut } from './query-shortcuts'
    import {
        activatePrimary,
        activatePrimaryOnResults,
        activateResultAt,
        activateSecondaryAtCursor,
        dispatchEnterAction,
    } from './result-actions'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-arguments -- E is the Svelte component generic; the explicit <E> binds the inference for callers like SearchDialog/SelectionDialog
        config: QueryDialogConfig<E>
    }

    /* eslint-disable prefer-const -- $props destructuring keeps types clean with const */
    let { config }: Props = $props()
    /* eslint-enable prefer-const */

    /** Shape of the `bind:this` ref for `QueryResults.svelte` — only the exported method we call. */
    interface QueryResultsAPI {
        scrollCursorIntoView(): void
    }

    let queryInputElement: HTMLInputElement | undefined = $state()
    let dialogElement: HTMLDivElement | undefined = $state()
    let queryResultsComponent: QueryResultsAPI | undefined = $state()
    /** The query field's pill frame; the recent-items dropdown anchors to it. */
    let queryFieldElement: HTMLElement | undefined = $state()
    let unlistenAutoApply: (() => void) | undefined

    /**
     * Live mirror of `search.autoApply`. Driven by `onSpecificSettingChange` so
     * toggling the setting in the settings window takes effect without reopening
     * the dialog. Same setting key for every consumer; AI mode never auto-applies
     * regardless (see `scheduleSearch`).
     */
    let autoApplyEnabled = $state<boolean>(getSetting('search.autoApply'))

    function focusInput(): void {
        queryInputElement?.focus()
    }

    /**
     * The run controller: the nothing-to-run guard, the auto-apply debounce + gates, the IME
     * guard, the AI round-trip, and the `hasSearched` / `highlightedFields` flags this
     * component renders off. `config` goes in as a GETTER: the consumer rebuilds it on every
     * reactive change, so a captured reference would freeze gates like `isIndexReady` at
     * their mount-time values.
     */
    const runner = createQueryRunner({
        getConfig: () => config,
        isAutoApplyEnabled: () => autoApplyEnabled,
        scrollCursorIntoView: () => {
            queryResultsComponent?.scrollCursorIntoView()
        },
    })

    /** The query field's own dropdown: the open flag plus its focus-restore rules. */
    const recent = createRecentPopover<E>({
        focusInput,
        getAnchor: () => queryFieldElement,
        onActivate: (entry) => {
            config.onActivateRecent(entry)
            // D8: hand ⏎ back to "run-search" so the very next Enter runs the loaded query.
            config.state.setLastDialogEvent('query-edited')
        },
    })

    // Reactive readers off the state instance.
    const query = $derived(config.state.getQuery())
    const mode = $derived(config.state.getMode())
    const results = $derived(config.state.getResults())
    const totalCount = $derived(config.state.getTotalCount())
    const cursorIndex = $derived(config.state.getCursorIndex())
    const isSearching = $derived(config.state.getIsSearching())
    const lastAiPrompt = $derived(config.state.getLastAiPrompt())
    const lastAiCaveat = $derived(config.state.getLastAiCaveat())
    const sizeFilter = $derived(config.state.getSizeFilter())
    const dateFilter = $derived(config.state.getDateFilter())

    /**
     * The AI-produced pattern for the transparency strip. The strip is the human-readable
     * mirror; the live chips stay the editable source of truth. Slot precedence and the
     * per-consumer reasoning live in `resolveAiPattern`.
     */
    const aiPattern = $derived(
        resolveAiPattern({
            extrasPattern: config.filterChipsExtras.aiPattern,
            extrasPatternKind: config.filterChipsExtras.aiPatternKind,
            regexBuffer: config.state.getHandTypedBuffer('regex'),
            globBuffer: config.state.getHandTypedBuffer('filename'),
        }),
    )

    /**
     * Structured, human-readable mirror of what the agent set: the produced pattern plus the
     * Size / Modified / Type filters. Rendered by `AiPromptStrip`; the live chips remain the
     * editable truth. Pure, so the rules are pinned in `ai-summary.test.ts`.
     */
    const aiSummary = $derived(
        buildAiSummary({
            pattern: aiPattern.pattern,
            patternKind: aiPattern.kind,
            sizeFilter: config.state.getSizeFilter(),
            sizeValue: config.state.getSizeValue(),
            sizeUnit: config.state.getSizeUnit(),
            sizeValueMax: config.state.getSizeValueMax(),
            sizeUnitMax: config.state.getSizeUnitMax(),
            dateFilter: config.state.getDateFilter(),
            dateValue: config.state.getDateValue(),
            dateValueMax: config.state.getDateValueMax(),
            typeFilter: config.state.getTypeFilter(),
            fileSizeFormat: getFileSizeFormat(),
        }),
    )

    /**
     * D8: which action `⏎` currently owns. The footer's secondary button reads
     * `<label> ⏎` only when `enterAction === 'go-to-file'`; the bar's run button
     * reads `Search ⏎` only when `enterAction === 'run-search'`. Exactly one of
     * them surfaces the hint at any time.
     */
    const enterAction = $derived(
        deriveEnterAction({
            lastEvent: config.state.getLastDialogEvent(),
            resultsCount: results.length,
        }),
    )

    const showRunHint = $derived(
        shouldShowRunHint({
            inputsDisabled: config.inputsDisabled,
            query,
            lastRunQuery: config.state.getLastRunQuery(),
            mode,
            autoApplyEnabled,
        }),
    )

    // Subscribe to icon cache version for reactivity.
    const iconVersion = $derived($iconCacheVersion)

    /**
     * Auto-mode fallback: when AI gets disabled mid-session and the dialog is on
     * AI mode, drop to filename so the user isn't stuck. We don't move them back to
     * AI when the provider returns; that's the user's call.
     */
    $effect(() => {
        if (!config.aiEnabled && config.state.getMode() === 'ai') {
            config.state.setMode('filename')
        }
    })

    /**
     * Single consumer for the `runOnMount` one-shot flag. Fires both on cold-open
     * (dialog mounts with the flag pre-set, e.g. MCP `open_search_dialog`) and on
     * hot-prefill (dialog already open when MCP lands new prefill). Clears the flag
     * first so downstream state writes can't re-trigger this effect.
     *
     * AI mode honors the explicit-trigger contract because the prefill caller's
     * `autoRun: true` counts as the explicit trigger (matching recent-search AI
     * click semantics).
     */
    $effect(() => {
        if (!config.state.getRunOnMount()) return
        config.state.setRunOnMount(false)
        // The prefill already cleared `results` / `cursorIndex`. Reset `hasSearched`
        // so the empty state (examples + index hint) is what the user sees until
        // the prefilled query runs.
        runner.setHasSearched(false)
        const trimmed = config.state.getQuery().trim()
        if (trimmed && config.state.getMode() === 'ai' && config.aiEnabled) {
            void runner.runAiSearch(trimmed)
        } else if (config.isIndexReady && hasRunnableQuery(config.state)) {
            void runner.executeQuery()
        }
        // Otherwise: prefill arrived but nothing to run. The dialog rests on the empty
        // state; the user hits Enter to fire when ready.
    })

    /** The status bar's Stop button. Same action Escape's first press takes. */
    function stopLiveRun(): void {
        runner.cancelLive()
    }

    /**
     * What Escape does, in order: hand it to an open popover, then STOP a running live
     * search, then close.
     *
     * The two-step exists because a live search is the one thing in this dialog that
     * takes minutes, and the reflex for "stop that" is Escape. Closing on the first
     * press would stop the walk too (the dialog's teardown cancels it), but it would
     * also take the results already on screen away, which is the opposite of what
     * someone pressing Escape at 40,000 folders wants.
     */
    function resolveEscape(): 'defer' | 'stopped' | 'close' {
        if (dialogElement?.querySelector('.ui-popover')) return 'defer'
        return runner.cancelLive() ? 'stopped' : 'close'
    }

    /**
     * Capture-phase Escape handler. Fires before the popover's bubble handler. When
     * a filter-chip popover (or the recent-items popover, which reuses the same
     * primitive) is open, Escape belongs to the popover, not the dialog: we defer
     * and let the popover's keydown close itself on the bubble.
     */
    function handleEscapeCapture(e: KeyboardEvent): void {
        if (e.key !== 'Escape') return
        const outcome = resolveEscape()
        if (outcome === 'defer') return
        e.preventDefault()
        e.stopPropagation()
        if (outcome === 'close') config.onClose()
    }

    onMount(async () => {
        // MCP open/close notification, the close registry, the focus trap, and focus
        // restore all belong to `ModalDialog` (we pass it `dialogId` + `onclose`).
        window.addEventListener('keydown', handleEscapeCapture, true)
        // D8: mark the dialog as freshly opened so ⏎ owns "run-search" by default
        // until the user edits the query/filters or results arrive.
        config.state.setLastDialogEvent('opened')

        // Reopen-with-results: when the surviving state holds a restorable session (a prior
        // run, plus a non-empty query or an active filter) re-derive it on mount so the user
        // sees the same results immediately, not the empty state. For non-AI modes we re-run
        // the query (cheap: Select re-derives against the freshly-snapshotted current folder,
        // which is MORE correct than showing rows from the old folder; Search re-hits the
        // index). AI mode never auto-runs (cloud cost): `hasSearched` was already seeded from
        // the prior run, so its persisted results render as-is without re-calling translate.
        //
        // A run the consumer kept alive across the last close wins over both: Search's
        // "Open in pane" leaves a walk feeding that pane, and re-running would SUPERSEDE
        // it — the pane would stop growing with nothing on screen saying why. So adopt
        // first, and only re-derive when there was nothing to adopt.
        const resumed = runner.resumeLive()
        if (
            !resumed &&
            config.state.getLastRunQuery() !== null &&
            config.state.getMode() !== 'ai' &&
            hasRunnableQuery(config.state)
        ) {
            config.state.setRunOnMount(true)
        }

        // Live-mirror `search.autoApply`. Shared key across consumers (no separate
        // `selection.autoApply` setting; the auto-apply contract is the same one).
        unlistenAutoApply = onSpecificSettingChange('search.autoApply', (value) => {
            autoApplyEnabled = value
        })

        // Load history (idempotent; only the first call hits the IPC).
        if (config.onLoadHistory) {
            try {
                await config.onLoadHistory()
            } catch {
                // Silent: empty history isn't an error condition.
            }
        }

        // Consumer-specific setup (Search: prepareSearchIndex, set up index-ready listener;
        // Selection: snapshot the focused pane).
        if (config.onMount) {
            try {
                await config.onMount()
            } catch {
                // Consumer is responsible for surfacing its own onMount failures.
            }
        }

        await tick()
        focusInput()
    })

    onDestroy(() => {
        if (config.onDestroy) {
            try {
                config.onDestroy()
            } catch {
                // Same rule as onMount: consumer surfaces its own failures.
            }
        }
        unlistenAutoApply?.()
        window.removeEventListener('keydown', handleEscapeCapture, true)
        runner.dispose()
        // State is intentionally NOT cleared. Close + reopen preserves the user's
        // query/filters/results/cursor. The only reset path is ⌘N inside the dialog.
    })

    /**
     * Count-only switch (zone 1, beside the mode chips). Flipping it changes what the
     * backend returns, so re-run: `scheduleSearch` keeps AI mode's explicit-trigger
     * contract intact (it no-ops there, and the AI run costs money).
     */
    function toggleCountOnly(): void {
        if (config.inputsDisabled) return
        config.filterChipsExtras.onToggleCountOnly?.()
        runner.scheduleSearch()
    }

    /**
     * "Show results" under the count-only total: turn count-only off AND re-run.
     * The re-run is NOT optional — a count-only run returned no rows, so flipping the
     * flag alone leaves the user staring at a stale number. It goes through
     * `runFromButton` (the same path as the bar's ⏎ button), not `scheduleSearch`,
     * which no-ops in AI mode and whenever `search.autoApply` is off.
     */
    function showResultsFromCount(): void {
        config.filterChipsExtras.onToggleCountOnly?.()
        runner.runFromButton()
    }

    /** Empty-state chip pick: load + run, mirroring the recent-search activation path. */
    function pickExample(chip: { mode: SearchMode; query: string }): void {
        config.state.setQuery(chip.query)
        config.state.setMode(chip.mode)
        if (chip.mode === 'ai') {
            if (config.aiEnabled) void runner.runAiSearch(chip.query)
        } else {
            void runner.executeQuery()
        }
        config.onPickExample(chip)
    }

    function handleQueryInput(value: string): void {
        config.state.setQueryFromUserInput(value)
        // D8: query edits hand ⏎ back to the bar's Search button.
        config.state.setLastDialogEvent('query-edited')
        runner.scheduleSearch()
    }

    function inputHandler(setter: (v: string) => void, search = true) {
        return (e: Event) => {
            setter((e.target as HTMLInputElement).value)
            // D8: filter inputs count as filter edits.
            config.state.setLastDialogEvent('filter-edited')
            if (search) runner.scheduleSearch()
        }
    }

    function handleModeChange(newMode: SearchMode): void {
        if (config.state.getMode() === newMode) return
        config.state.switchMode(newMode)
        // Switching mode preserves the typed query; only re-trigger auto-apply for non-AI modes.
        if (newMode !== 'ai') runner.scheduleSearch()
    }

    /** ⌘N, ⌥A/F/R, ⌥⏎, ⌘⏎/⇧⏎, ⌘H, ⌘1-9. The matching rules live in `query-shortcuts.ts`. */
    function handleModifierShortcuts(e: KeyboardEvent): boolean {
        return routeModifierShortcut(e, {
            aiEnabled: config.aiEnabled,
            onNewQuery: clearAndRefocus,
            onModeChange: handleModeChange,
            onFocusInput: focusInput,
            onToggleRecent: recent.toggle,
            onPrimaryAction: () => {
                activatePrimaryOnResults(config)
            },
        })
    }

    /**
     * ⌘N: consumer's reset hook (Search clears core + extras via its facade;
     * Selection can omit and inherit the core reset). We also clear the core's
     * `lastRunQuery` so the "Press Enter to search" hint resets cleanly.
     */
    function clearAndRefocus(): void {
        if (config.onClearState) {
            config.onClearState()
        } else {
            config.state.clearCore()
        }
        config.state.setLastRunQuery(null)
        runner.setHasSearched(false)
        void tick().then(() => { focusInput(); })
    }

    /**
     * Up / Down navigation through results. Loops top<->bottom.
     *
     * With no result list to walk, ArrowDown opens the recent-items dropdown instead — the
     * combobox gesture, landing on a key that was otherwise dead. Results win when they
     * exist: walking them is the more valuable use of ↓, and the chevron + `⌘H` open the
     * dropdown unconditionally.
     */
    function handleArrowNav(e: KeyboardEvent): void {
        const len = config.state.getResults().length
        if (len === 0) {
            if (e.key === 'ArrowDown' && !recent.isOpen && recentEntries.length > 0) {
                e.preventDefault()
                recent.open()
            }
            return
        }
        e.preventDefault()
        const cur = config.state.getCursorIndex()
        const next = e.key === 'ArrowDown' ? (cur + 1) % len : (cur - 1 + len) % len
        config.state.setCursorIndex(next)
        // D8: cursor moves keep ⏎ on "go-to-file" as the user browses the list.
        config.state.setLastDialogEvent('cursor-moved')
        queryResultsComponent?.scrollCursorIntoView()
    }

    /** Mouse hover writes the cursor so mouse + keyboard share one cursor (cursor model). */
    function handleHover(index: number): void {
        const r = config.state.getResults()
        if (index < 0 || index >= r.length) return
        if (config.state.getCursorIndex() !== index) {
            config.state.setCursorIndex(index)
            // D8: mouse hover counts as a cursor move for ⏎ ownership.
            config.state.setLastDialogEvent('cursor-moved')
        }
    }

    function handleKeyDown(e: KeyboardEvent): void {
        e.stopPropagation()
        // Tab wrapping is handled by `use:trapFocus` on the overlay.
        if (handleModifierShortcuts(e)) return
        switch (e.key) {
            case 'Escape': {
                // The window-capture handler above normally gets here first; this is the
                // same two-step for the paths that reach the dialog's own keydown.
                const outcome = resolveEscape()
                if (outcome === 'defer') break
                e.preventDefault()
                if (outcome === 'close') config.onClose()
                break
            }
            case 'ArrowDown':
            case 'ArrowUp':
                handleArrowNav(e)
                break
            case 'Enter':
                e.preventDefault()
                dispatchEnterAction(config, enterAction, runner.run)
                break
        }
    }

    function openRowMenu(entry: SearchResultEntry): void {
        config.onRowMenu(entry)
    }

    const recentEntries = $derived(config.historyStore.getList())
</script>

<!--
  The dialog chrome (radius, panel edge, shadow, title bar, ×, focus trap, MCP
  registry, focus restore) is `ModalDialog`'s. We opt into:
    - `align="top"`      the Spotlight-style placement this dialog has always had.
    - `fillBody`         fixed-height frame; `.results-well` absorbs the slack.
    - `ownsKeyboard`     `handleKeyDown` owns Enter (the ⏎ ownership swap) and the
                         window-capture Escape that defers to an open popover.
    - `closeOnOverlayClick`  clicking the scrim dismisses, as it always has.
-->
<ModalDialog
    titleId="query-dialog-title"
    dialogId={config.dialogType}
    overlayClass="search-overlay"
    align="top"
    fillBody
    resizable
    ownsKeyboard
    closeOnOverlayClick
    containerStyle="width: min(100%, {config.width}); max-height: 80vh;"
    onkeydown={handleKeyDown}
    onclose={config.onClose}
>
    <!-- The title text keeps its own `<span>`: the badge is a sibling, so consumers'
         tests (and anyone styling the two apart) can address the words alone. -->
    {#snippet title()}
        <span>{config.title}</span>{#if config.badge}<StatusBadge status={config.badge} />{/if}
    {/snippet}

    <div class="query-dialog-body" bind:this={dialogElement}>
        <!-- Zone 1: what to look for and how, as a 2×2 grid. `QueryBar` is
             `display: contents`, so it drops the query field into the left column and the
             Search button into the right one; the mode chips and the Count-only switch sit
             under them in the same two columns. -->
        <div class="query-grid">
            <QueryBar
                bind:inputElement={queryInputElement}
                bind:fieldElement={queryFieldElement}
                {query}
                {mode}
                disabled={config.inputsDisabled}
                aiHighlight={runner.highlightedFields.has('query')}
                {showRunHint}
                runHintCopy={config.runHintCopy ?? tString('queryUi.bar.runHint')}
                runTitleOverride={config.runTitleOverride}
                showEnterHint={enterAction === 'run-search'}
                recentOpen={recent.isOpen}
                onInput={handleQueryInput}
                onRun={runner.runFromButton}
                onToggleRecent={recent.toggle}
                recentTriggerLabel={config.recentItems.triggerAriaLabel ?? tString('queryUi.recent.allButtonAria')}
                recentTriggerTooltip={config.recentItems.triggerTooltip ?? tString('queryUi.recent.trailingTooltip')}
                onCompositionStart={runner.handleCompositionStart}
                onCompositionEnd={runner.handleCompositionEnd}
            />

            <!-- The mode chips fill the left column (`fullWidth`), so the Count-only switch
                 rides beside them rather than inside the group. It belongs in zone 1: it
                 changes what the search RETURNS, it isn't one more way to narrow the matches.
                 Search wires `onToggleCountOnly`; Selection omits it, and then the chips take
                 the whole row rather than leaving a hole in the right column. -->
            <div class="query-grid__modes" class:is-wide={!config.filterChipsExtras.onToggleCountOnly}>
                <ModeChips
                    {mode}
                    aiEnabled={config.aiEnabled}
                    disabled={config.inputsDisabled}
                    onSelect={handleModeChange}
                />
            </div>
            {#if config.filterChipsExtras.onToggleCountOnly}
                <div class="query-grid__count-only" use:tooltip={tString('queryUi.filters.countOnly.tooltip')}>
                    <Switch
                        checked={config.filterChipsExtras.countOnly ?? false}
                        disabled={config.inputsDisabled}
                        onCheckedChange={toggleCountOnly}
                    >
                        {tString('queryUi.filters.countOnly.label')}
                    </Switch>
                </div>
            {/if}
        </div>

        {#if lastAiPrompt}
            <AiPromptStrip aiPrompt={lastAiPrompt} caveat={lastAiCaveat ?? ''} summary={aiSummary} />
        {/if}

        {#if config.noticeBanner}
            <div class="query-dialog__notice" role="note">{config.noticeBanner}</div>
        {/if}

        <!-- Zone 2: the filters. `FilterChips` owns its own band surface + hairlines. -->
        <FilterChips
            filterState={config.state}
            caseSensitive={config.filterChipsExtras.caseSensitive}
            scope={config.filterChipsExtras.scope}
            excludeSystemDirs={config.filterChipsExtras.excludeSystemDirs}
            scopePresets={config.filterChipsExtras.scopePresets}
            defaultScope={config.filterChipsExtras.defaultScope}
            sizeFilter={config.state.getSizeFilter()}
            sizeValue={config.state.getSizeValue()}
            sizeUnit={config.state.getSizeUnit()}
            sizeValueMax={config.state.getSizeValueMax()}
            sizeUnitMax={config.state.getSizeUnitMax()}
            dateFilter={config.state.getDateFilter()}
            dateValue={config.state.getDateValue()}
            dateValueMax={config.state.getDateValueMax()}
            typeFilter={config.state.getTypeFilter()}
            systemDirExcludeTooltip={config.filterChipsExtras.systemDirExcludeTooltip}
            highlightedFields={runner.highlightedFields}
            disabled={config.inputsDisabled}
            {mode}
            {query}
            aiPattern={config.filterChipsExtras.aiPattern}
            scopeChipVisible={config.visibleChips.scope}
            patternChipVisible={config.visibleChips.pattern}
            onInput={inputHandler}
            onToggleCaseSensitive={config.filterChipsExtras.onToggleCaseSensitive}
            onToggleExcludeSystemDirs={config.filterChipsExtras.onToggleExcludeSystemDirs}
            onSetScope={config.filterChipsExtras.onSetScope}
            onClearAiPattern={config.filterChipsExtras.onClearAiPattern}
            scheduleSearch={runner.scheduleSearch}
            onFocusBar={focusInput}
        />

        {#if config.resultsNotice}
            {@render config.resultsNotice()}
        {/if}

        <!-- Zone 3: the results. `.results-container` inside is the only `flex: 1 1 auto`
             child of the body, so it absorbs whatever room the strips leave. -->
        <QueryResults
            bind:this={queryResultsComponent}
            {results}
            {cursorIndex}
            isIndexAvailable={config.isIndexAvailable}
            isIndexReady={config.isIndexReady}
            {isSearching}
            hasSearched={runner.hasSearched}
            {query}
            {sizeFilter}
            {dateFilter}
            scanning={config.scanning}
            entriesScanned={config.entriesScanned}
            {totalCount}
            indexEntryCount={config.indexEntryCount}
            countOnly={config.filterChipsExtras.countOnly ?? false}
            onShowResults={config.filterChipsExtras.onToggleCountOnly ? showResultsFromCount : undefined}
            live={runner.live}
            onStopLive={config.streamingSource ? stopLiveRun : undefined}
            iconCacheVersion={iconVersion}
            aiEnabled={config.aiEnabled}
            showPathColumn={config.showPathColumn}
            onResultClick={(index: number) => {
                activateResultAt(config, index)
            }}
            onHover={handleHover}
            onPickExample={pickExample}
            emptyExamples={config.emptyState.examples}
            onPickPath={config.onPickPath}
            onRowMenu={openRowMenu}
        />

        {#if config.resultsExtra}
            {@render config.resultsExtra()}
        {/if}

        <div class="dialog-footer">
            <div class="footer-right">
                {#if config.secondaryAction || config.primaryAction}
                    <div class="query-dialog__actions" role="group" aria-label={tString('queryUi.dialog.actionsAria')}>
                        {#if config.secondaryAction}
                            <Button
                                variant="secondary"
                                disabled={config.inputsDisabled || results.length === 0}
                                onclick={() => {
                                    activateSecondaryAtCursor(config)
                                }}
                                aria-label={config.secondaryAction.ariaLabel ?? config.secondaryAction.label}
                            >
                                <span class="action-label" use:tooltip={config.secondaryAction.tooltip ?? ''}>
                                    {config.secondaryAction.label}{#if enterAction === 'go-to-file'}<ShortcutChip
                                            key={config.secondaryAction.shortcutHint}
                                            size="sm"
                                        />{/if}
                                </span>
                            </Button>
                        {/if}
                        {#if config.primaryAction}
                            <Button
                                variant="primary"
                                disabled={config.inputsDisabled || results.length === 0}
                                onclick={() => {
                                    activatePrimary(config)
                                }}
                                aria-label={config.primaryAction.ariaLabel ?? config.primaryAction.label}
                            >
                                <span class="action-label" use:tooltip={config.primaryAction.tooltip ?? ''}>
                                    {config.primaryAction.label}<ShortcutChip
                                        key={config.primaryAction.shortcutHint}
                                        size="sm"
                                    />
                                </span>
                            </Button>
                        {/if}
                    </div>
                {/if}
            </div>
        </div>

        <!-- The recent-items dropdown hangs off the query field, so it reads as the field's
             own list rather than a second surface elsewhere in the dialog. -->
        {#if queryFieldElement}
            <RecentItemsPopover
                anchor={queryFieldElement}
                open={recent.isOpen}
                entries={recentEntries}
                adapter={config.recentItems.adapter}
                keyFn={config.recentItems.keyFn}
                onClose={recent.close}
                onPick={recent.pick}
                onRemove={config.onRemoveRecent}
                onExitTop={recent.closeAndFocus}
                filterPlaceholder={config.recentItems.filterPlaceholder}
                emptyMessage={config.recentItems.emptyMessage}
                ariaLabel={config.recentItems.popoverAriaLabel}
                ariaListboxLabel={config.recentItems.listboxAriaLabel}
            />
        {/if}
    </div>
</ModalDialog>

<style>
    /* The body's own column. `ModalDialog`'s `fillBody` gives us a flex-column
       `.modal-body` with a definite height, so this stack can hand all the slack to
       `.results-container` (the only `flex: 1 1 auto` descendant) and keep every strip
       at its intrinsic height. */
    .query-dialog-body {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    /* Zone-1 control grid, a real 2×2: query field + Search button on row 1, mode chips
       + Count-only switch on row 2. ONE `auto` right column, shared by both rows, sizes
       itself to the wider of "Search ⏎" / "Count only" and never wraps either; the left
       column takes the rest at `minmax(0, 1fr)` so the `fullWidth` ToggleGroup can't push
       the grid wider than the dialog. The side inset is `ModalDialog`'s. */
    .query-grid {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-lg) 0;
        flex-shrink: 0;
    }

    .query-grid__modes {
        min-width: 0;
    }

    /* No Count-only switch (Selection): the chips take both columns. Keeping them in
       column 1 would leave a hole and squeeze four mode cells into a narrower row than
       they need at the dialog's minimum width. */
    .query-grid__modes.is-wide {
        grid-column: 1 / -1;
    }

    .query-grid__count-only {
        display: flex;
        align-items: center;
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    /* Optional notice banner row. Selection's snapshot-pane mode uses this to
       surface "Matching what's shown in the list (the full path)"; Search passes
       undefined and the row doesn't render. */
    .query-dialog__notice {
        padding: var(--spacing-xs) 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        flex-shrink: 0;
    }

    /* The footer is the action row now: recent items live in the query field's dropdown. */
    .dialog-footer {
        display: flex;
        align-items: stretch;
        justify-content: flex-end;
        border-top: 1px solid var(--color-border-subtle);
        flex-shrink: 0;
    }

    .footer-right {
        flex: 0 0 auto;
    }

    .query-dialog__actions {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) 0;
    }

    /* The action verb leads; the shortcut hint rides a standard `ShortcutChip` to its right. */
    .action-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
