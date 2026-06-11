<script lang="ts" generics="E = unknown">
    /**
     * QueryDialog: the shared orchestrator for filter-and-act-on dialogs.
     *
     * Search is the first consumer; Selection is the second. Everything that diverges
     * per consumer comes in via `QueryDialogConfig`; everything else lives here as the
     * one source of truth for both consumers' polish (keyboard contract, IME guard,
     * auto-apply gates, `deriveEnterAction` ownership swap, `lastDialogEvent` lifecycle,
     * the title bar, the chip strip, the results table, the recent-items footer +
     * popover, the empty state, the notice banner).
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
     * Layout (top → bottom):
     *   1. Title bar (32px, centered, no close button per § Title bar).
     *   2. QueryBar (unified input drives every mode).
     *   3. ModeChips (AI / Filename / Content (disabled) / Regex; consumer-driven).
     *   4. AiPromptStrip (when `state.lastAiPrompt` is non-null).
     *   5. Optional notice banner (Selection uses this on snapshot panes).
     *   6. FilterChips (Pattern / Size / Modified / Search in; visibility per config).
     *   7. QueryResults (column headers + results + states + status bar).
     *   8. Footer: recent-items strip on the left, primary/secondary action buttons on the right.
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { notifyDialogOpened, notifyDialogClosed } from '$lib/tauri-commands'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import { iconCacheVersion } from '$lib/icon-cache'
    import QueryBar from './QueryBar.svelte'
    import ModeChips from './ModeChips.svelte'
    import FilterChips from './filter-chips/FilterChips.svelte'
    import QueryResults from './QueryResults.svelte'
    import AiPromptStrip from './AiPromptStrip.svelte'
    import RecentItemsFooter from './recent-items/RecentItemsFooter.svelte'
    import RecentItemsPopover from './recent-items/RecentItemsPopover.svelte'
    import { deriveEnterAction, SEARCH_AUTO_APPLY_DEBOUNCE_MS, type SearchMode } from './query-filter-state.svelte'
    import type { QueryDialogConfig } from './query-dialog-config'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { trapFocus } from '$lib/ui/focus-trap'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { addToast } from '$lib/ui/toast/toast-store.svelte'
    import { showAiTranslateErrorToast } from '$lib/ai/translate-error-toast'

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
    let footerRef: HTMLDivElement | undefined = $state()
    let recentPopoverOpen = $state(false)
    let debounceTimer: ReturnType<typeof setTimeout> | undefined
    let unlistenAutoApply: (() => void) | undefined
    let highlightedFields: SvelteSet<string> = new SvelteSet<string>()
    let hasSearched = $state(false)
    /**
     * IME composition flag. While true, `scheduleSearch` is a no-op so we don't fire
     * mid-character on Chinese/Japanese/Korean input. On `compositionend` the bar
     * calls `scheduleSearch` once so the user gets exactly one fire post-composition.
     */
    let imeComposing = false

    /**
     * Live mirror of `search.autoApply`. Driven by `onSpecificSettingChange` so
     * toggling the setting in the settings window takes effect without reopening
     * the dialog. Same setting key for every consumer; AI mode never auto-applies
     * regardless (see `scheduleSearch`).
     */
    let autoApplyEnabled = $state<boolean>(getSetting('search.autoApply'))

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

    /**
     * "Press Enter to search" hint visibility:
     *   1. Inputs disabled → hide.
     *   2. Trimmed query is empty → hide.
     *   3. Query unchanged since last run → hide.
     *   4. AI mode (never auto-applies) OR setting off → show.
     */
    const showRunHint = $derived.by(() => {
        if (config.inputsDisabled) return false
        const trimmed = query.trim()
        if (!trimmed) return false
        const lastRun = config.state.getLastRunQuery() ?? ''
        if (trimmed === lastRun.trim()) return false
        return mode === 'ai' || !autoApplyEnabled
    })

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
        hasSearched = false
        const trimmed = config.state.getQuery().trim()
        const hasFilters =
            config.state.getSizeFilter() !== 'any' || config.state.getDateFilter() !== 'any'
        if (trimmed && config.state.getMode() === 'ai' && config.aiEnabled) {
            void runAiSearch(trimmed)
        } else if (config.isIndexReady && (trimmed || hasFilters)) {
            void executeQuery()
        }
        // Otherwise: prefill arrived but nothing to run. The dialog rests on the empty
        // state; the user hits Enter to fire when ready.
    })

    /**
     * Capture-phase Escape handler. Fires before the popover's bubble handler. When
     * a filter-chip popover (or the recent-items popover, which reuses the same
     * primitive) is open, Escape belongs to the popover, not the dialog: we defer
     * and let the popover's keydown close itself on the bubble.
     */
    function handleEscapeCapture(e: KeyboardEvent): void {
        if (e.key !== 'Escape') return
        if (dialogElement?.querySelector('.filter-chip-popover')) {
            return
        }
        e.preventDefault()
        e.stopPropagation()
        config.onClose()
    }

    function focusInput(): void {
        queryInputElement?.focus()
    }

    /** Element that had focus when the dialog opened (the pane container). Restored on close. */
    let previousActiveElement: HTMLElement | null = null

    function openRecentPopover(): void {
        recentPopoverOpen = true
    }

    function closeRecentPopover(): void {
        recentPopoverOpen = false
    }

    onMount(async () => {
        // Capture synchronously, before the awaits below and before focusInput() moves focus.
        previousActiveElement = document.activeElement instanceof HTMLElement ? document.activeElement : null
        notifyDialogOpened(config.dialogType).catch(() => {})
        window.addEventListener('keydown', handleEscapeCapture, true)
        // D8: mark the dialog as freshly opened so ⏎ owns "run-search" by default
        // until the user edits the query/filters or results arrive.
        config.state.setLastDialogEvent('opened')

        // Live-mirror `search.autoApply`. Shared key across consumers (no separate
        // `selection.autoApply` setting; the auto-apply contract is the same one).
        unlistenAutoApply = onSpecificSettingChange('search.autoApply', (_id, value) => {
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
        notifyDialogClosed(config.dialogType).catch(() => {})
        if (config.onDestroy) {
            try {
                config.onDestroy()
            } catch {
                // Same rule as onMount: consumer surfaces its own failures.
            }
        }
        unlistenAutoApply?.()
        window.removeEventListener('keydown', handleEscapeCapture, true)
        if (debounceTimer) clearTimeout(debounceTimer)
        // Restore focus to whatever had it before we opened (the pane container), if it's
        // still in the DOM. Without this, focus falls to <body> after close: arrow keys stop
        // moving the pane cursor and natively scroll the pane instead, until the user clicks
        // back in. Same pattern as CommandPalette and ModalDialog.
        if (previousActiveElement?.isConnected) {
            previousActiveElement.focus()
        }
        // State is intentionally NOT cleared. Close + reopen preserves the user's
        // query/filters/results/cursor. The only reset path is ⌘N inside the dialog.
    })

    /**
     * Schedules a debounced auto-apply. Three early-return gates:
     *   1. AI mode never auto-applies (AI calls cost money; user must opt in).
     *   2. `search.autoApply === false`: user runs every query explicitly.
     *   3. IME composition is in progress.
     */
    function scheduleSearch(): void {
        if (debounceTimer) clearTimeout(debounceTimer)
        if (config.state.getMode() === 'ai') return
        if (!autoApplyEnabled) return
        if (imeComposing) return
        debounceTimer = setTimeout(() => {
            void executeQuery()
        }, SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    }

    function handleCompositionStart(): void {
        imeComposing = true
        if (debounceTimer) clearTimeout(debounceTimer)
    }

    function handleCompositionEnd(): void {
        imeComposing = false
        scheduleSearch()
    }

    /**
     * Runs the consumer's `runQuery` callback and writes the result into state.
     * `fromAiTranslation` is true only when invoked from `runAiSearch` after the
     * translation has populated state; in that branch we keep `lastAiPrompt` /
     * `lastAiCaveat` intact (they were just set). Every other branch clears them
     * so the strip doesn't outlive its AI run.
     */
    async function executeQuery(fromAiTranslation = false): Promise<void> {
        if (debounceTimer) clearTimeout(debounceTimer)
        hasSearched = true
        if (!config.isIndexReady) return

        config.state.setIsSearching(true)
        try {
            const result = await config.runQuery()
            config.state.setResults(result.entries)
            config.state.setTotalCount(result.totalCount)
            config.state.setCursorIndex(0)
            // D8: results just landed. ⏎ now owns "go-to-file" (when results > 0).
            config.state.setLastDialogEvent('results-arrived')
            config.state.setLastRunQuery(config.state.getQuery())
            if (!fromAiTranslation) {
                // Non-AI search completed cleanly. The AI strip belongs to the previous
                // AI run, so drop it. AI runs go through `runAiSearch`, which sets the
                // strip and then calls us with `fromAiTranslation = true`.
                config.state.setLastAiPrompt(null)
                config.state.setLastAiCaveat(null)
            }
        } catch {
            // IPC error: silent. Consumer is responsible for logging.
        } finally {
            config.state.setIsSearching(false)
        }
    }

    /**
     * Runs an AI translation for `prompt`, then executes the query. The consumer's
     * `translateAi` owns applying every AI-returned filter (size / date / scope /
     * AI pattern + label / etc); QueryDialog captures the prompt, flashes any
     * highlighted fields, sets the caveat, and runs the query.
     */
    async function runAiSearch(prompt: string): Promise<void> {
        const trimmed = prompt.trim()
        if (!trimmed) return
        if (!config.translateAi) return

        // Capture the prompt BEFORE calling the IPC so the user sees what they asked
        // even if the IPC fails. The AI bar in AI mode keeps the prompt as the bar's
        // contents (the pattern lives separately via the consumer's extras).
        config.state.setLastAiPrompt(trimmed)
        config.state.setLastAiCaveat(null)

        let result: Awaited<ReturnType<NonNullable<typeof config.translateAi>>>
        try {
            result = await config.translateAi(trimmed)
        } catch (err) {
            // Surface WHY the translation failed (quota, key, timeout, empty answer, …) as a
            // specific toast instead of a silent no-op. Both Search and Selection route here,
            // so the error UX lives in one place. The consumer's `translateAi` lets the typed
            // error throw; we map its `kind` to copy. A non-translation error (shouldn't happen)
            // falls through to a generic toast.
            if (!showAiTranslateErrorToast(err)) {
                addToast("Couldn't run the AI search just now. Try again?", { level: 'warn', dismissal: 'transient' })
            }
            return
        }
        if (!result) return

        // Flash the changed fields for ~1.5 s so the user sees what the AI touched.
        if (result.highlightedFields && result.highlightedFields.length > 0) {
            const next = new SvelteSet<string>(result.highlightedFields)
            highlightedFields = next
            setTimeout(() => {
                highlightedFields = new SvelteSet<string>()
            }, 1500)
        }
        config.state.setLastAiCaveat(result.caveat)

        await executeQuery(true)
        await focusFirstResult()
    }

    async function focusFirstResult(): Promise<void> {
        await tick()
        queryResultsComponent?.scrollCursorIntoView()
    }

    function runFromButton(): void {
        if (config.inputsDisabled) return
        if (config.state.getMode() === 'ai') {
            runAiFromQuery()
        } else {
            void executeQuery()
        }
    }

    function runAiFromQuery(): void {
        if (!config.aiEnabled) return
        const trimmed = config.state.getQuery().trim()
        if (trimmed) void runAiSearch(trimmed)
    }

    /** Empty-state chip pick: load + run, mirroring the recent-search activation path. */
    function pickExample(chip: { mode: SearchMode; query: string }): void {
        config.state.setQuery(chip.query)
        config.state.setMode(chip.mode)
        if (chip.mode === 'ai') {
            if (config.aiEnabled) void runAiSearch(chip.query)
        } else {
            void executeQuery()
        }
        config.onPickExample(chip)
    }

    function handleQueryInput(value: string): void {
        config.state.setQueryFromUserInput(value)
        // D8: query edits hand ⏎ back to the bar's Search button.
        config.state.setLastDialogEvent('query-edited')
        scheduleSearch()
    }

    function inputHandler(setter: (v: string) => void, search = true) {
        return (e: Event) => {
            setter((e.target as HTMLInputElement).value)
            // D8: filter inputs count as filter edits.
            config.state.setLastDialogEvent('filter-edited')
            if (search) scheduleSearch()
        }
    }

    /**
     * Matches a plain modifier-key combo (cmd OR alt, no others, no shift).
     *
     * On macOS, Option+<letter> remaps `event.key` to a typographic glyph (Option+F → "ƒ").
     * For Alt combos we therefore also match on `event.code` (which stays layout-stable as
     * `KeyF` etc.). For named keys (Enter, ArrowLeft, …) and Meta combos the plain `e.key`
     * check remains the contract.
     */
    function matchKey(e: KeyboardEvent, key: string, mod: 'meta' | 'alt'): boolean {
        if (e.shiftKey) return false
        const modMatches = mod === 'meta' ? e.metaKey && !e.altKey : e.altKey && !e.metaKey
        if (!modMatches) return false
        if (e.key === key) return true
        if (mod === 'alt' && key.length === 1 && /[a-zA-Z]/.test(key)) {
            return e.code === `Key${key.toUpperCase()}`
        }
        return false
    }

    /** Returns the chip slot for ⌘1 / ⌘2 / ⌘3, or null. AI when on shifts the numbering. */
    function modeForShortcutNumber(n: number): SearchMode | null {
        if (config.aiEnabled) {
            if (n === 1) return 'ai'
            if (n === 2) return 'filename'
            if (n === 3) return 'regex'
        } else {
            if (n === 1) return 'filename'
            if (n === 2) return 'regex'
        }
        return null
    }

    function handleModeChange(newMode: SearchMode): void {
        if (config.state.getMode() === newMode) return
        config.state.switchMode(newMode)
        // Switching mode preserves the typed query; only re-trigger auto-apply for non-AI modes.
        if (newMode !== 'ai') scheduleSearch()
    }

    function handleModeShortcut(e: KeyboardEvent): boolean {
        if (!e.metaKey || e.altKey || e.shiftKey) return false
        if (e.key < '1' || e.key > '9') return false
        const n = parseInt(e.key, 10)
        const target = modeForShortcutNumber(n)
        if (!target) return false
        e.preventDefault()
        handleModeChange(target)
        focusInput()
        return true
    }

    /**
     * Mode chip shortcuts (⌥A / ⌥F / ⌥R). Wired globally inside the dialog (focus
     * need not be on the chip). The disabled Content chip has no shortcut by design.
     */
    function handleModeChipShortcut(e: KeyboardEvent): boolean {
        if (matchKey(e, 'a', 'alt') && config.aiEnabled) {
            e.preventDefault()
            handleModeChange('ai')
            return true
        }
        if (matchKey(e, 'f', 'alt')) {
            e.preventDefault()
            handleModeChange('filename')
            return true
        }
        if (matchKey(e, 'r', 'alt')) {
            e.preventDefault()
            handleModeChange('regex')
            return true
        }
        return false
    }

    function jumpToCursorParent(): void {
        const idx = config.state.getCursorIndex()
        const r = config.state.getResults()
        if (idx < 0 || idx >= r.length) return
        const target = parentOf(r[idx].path) ?? parentOf(r[idx].parentPath)
        if (!target) return
        config.onPickPath(target)
    }

    function descendFromCursor(): void {
        const idx = config.state.getCursorIndex()
        const r = config.state.getResults()
        if (idx < 0 || idx >= r.length) return
        config.onPickPath(r[idx].path)
    }

    /** Returns the parent directory of a POSIX path, or null for root/empty. */
    function parentOf(path: string): string | null {
        if (!path || path === '/') return null
        const normalized = path.endsWith('/') && path !== '/' ? path.slice(0, -1) : path
        const lastSlash = normalized.lastIndexOf('/')
        if (lastSlash < 0) return null
        if (lastSlash === 0) return '/'
        return normalized.slice(0, lastSlash)
    }

    function handleAltArrowShortcut(e: KeyboardEvent): boolean {
        if (matchKey(e, 'ArrowLeft', 'alt')) {
            e.preventDefault()
            jumpToCursorParent()
            return true
        }
        if (matchKey(e, 'ArrowRight', 'alt')) {
            e.preventDefault()
            descendFromCursor()
            return true
        }
        return false
    }

    /**
     * Routes Enter combinations: ⌥⏎ fires the primary action; ⌘⏎ and ⇧⏎ are
     * explicit no-ops per R4 (bare Enter is the only key that does anything).
     */
    function handleEnterCombinations(e: KeyboardEvent): boolean {
        if (e.key !== 'Enter') return false
        if (e.altKey && !e.metaKey && !e.shiftKey) {
            e.preventDefault()
            const r = config.state.getResults()
            if (r.length > 0 && config.primaryAction) {
                void config.primaryAction.handler(r)
            }
            return true
        }
        if (e.metaKey || e.shiftKey) {
            e.preventDefault()
            return true
        }
        return false
    }

    /**
     * Handles ⌘N, ⌘H, ⌘1-9, ⌥A/F/R, ⌥←/⌥→, ⌥⏎ (primary action), ⌘⏎/⇧⏎ no-op.
     */
    function handleModifierShortcuts(e: KeyboardEvent): boolean {
        if (matchKey(e, 'n', 'meta')) {
            e.preventDefault()
            clearAndRefocus()
            return true
        }
        if (handleModeChipShortcut(e)) return true
        if (handleAltArrowShortcut(e)) return true
        if (handleEnterCombinations(e)) return true
        if (matchKey(e, 'h', 'meta')) {
            e.preventDefault()
            if (recentPopoverOpen) closeRecentPopover()
            else openRecentPopover()
            return true
        }
        if (handleModeShortcut(e)) return true
        return false
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
        hasSearched = false
        void tick().then(() => { focusInput(); })
    }

    /**
     * Up / Down navigation through results. Loops top<->bottom.
     */
    function handleArrowNav(e: KeyboardEvent): void {
        const len = config.state.getResults().length
        if (len === 0) return
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
            case 'Escape':
                e.preventDefault()
                config.onClose()
                break
            case 'ArrowDown':
            case 'ArrowUp':
                handleArrowNav(e)
                break
            case 'Enter':
                e.preventDefault()
                handleEnterKey()
                break
        }
    }

    /**
     * Bare Enter per D8: dispatches on `enterAction`.
     *   - 'go-to-file': fires `secondaryAction.handler(currentEntry)`. If no
     *     secondary action exists (Selection), falls through to the primary action.
     *   - 'run-search': run the active mode's query (AI / filename / regex).
     */
    function handleEnterKey(): void {
        const r = config.state.getResults()
        if (enterAction === 'go-to-file') {
            if (config.secondaryAction) {
                const idx = config.state.getCursorIndex()
                if (idx >= 0 && idx < r.length) {
                    void config.secondaryAction.handler(r[idx])
                }
                return
            }
            // Selection-style: no secondary; fall through to primary on the result set.
            if (config.primaryAction && r.length > 0) {
                void config.primaryAction.handler(r)
            }
            return
        }
        if (config.state.getMode() === 'ai') {
            runAiFromQuery()
        } else {
            void executeQuery()
        }
    }

    function handleResultClick(index: number): void {
        const r = config.state.getResults()
        if (index >= r.length) return
        if (config.secondaryAction) {
            void config.secondaryAction.handler(r[index])
            return
        }
        // No secondary: Selection-style → primary on the whole result set.
        if (config.primaryAction) void config.primaryAction.handler(r)
    }

    function handleOverlayClick(e: MouseEvent): void {
        if (e.target === e.currentTarget) config.onClose()
    }

    function openRowMenu(entry: SearchResultEntry): void {
        config.onRowMenu(entry)
    }

    function activatePrimary(): void {
        const r = config.state.getResults()
        if (config.primaryAction) void config.primaryAction.handler(r)
    }

    function activateSecondary(): void {
        const r = config.state.getResults()
        const idx = config.state.getCursorIndex()
        if (!config.secondaryAction) return
        if (idx < 0 || idx >= r.length) return
        void config.secondaryAction.handler(r[idx])
    }

    const recentEntries = $derived(config.historyStore.getList())
</script>

<div
    class="search-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="query-dialog-title"
    tabindex="-1"
    onclick={handleOverlayClick}
    onkeydown={handleKeyDown}
    use:trapFocus={{ onEscape: config.onClose }}
>
    <div class="search-dialog" bind:this={dialogElement} style="max-width: {config.maxWidth};">
        <!--
          Title bar: a heading-shaped element (not a banner). `<header>` would be a
          landmark and duplicate the app's existing banner; `<h2>` is the right semantic
          choice for "dialog title" and pairs cleanly with the dialog's
          `aria-labelledby`. Visually styled as the title strip per § "Title bar".
        -->
        <h2 class="query-dialog__title" id="query-dialog-title">
            <span>{config.title}</span>
            {#if config.badge}
                <StatusBadge status={config.badge} />
            {/if}
        </h2>

        <QueryBar
            bind:inputElement={queryInputElement}
            {query}
            {mode}
            disabled={config.inputsDisabled}
            aiHighlight={highlightedFields.has('query')}
            {showRunHint}
            showEnterHint={enterAction === 'run-search'}
            onInput={handleQueryInput}
            onRun={runFromButton}
            onCompositionStart={handleCompositionStart}
            onCompositionEnd={handleCompositionEnd}
        />

        <ModeChips {mode} aiEnabled={config.aiEnabled} disabled={config.inputsDisabled} onSelect={handleModeChange} />

        {#if lastAiPrompt}
            <AiPromptStrip aiPrompt={lastAiPrompt} caveat={lastAiCaveat ?? ''} />
        {/if}

        {#if config.noticeBanner}
            <div class="query-dialog__notice" role="note">{config.noticeBanner}</div>
        {/if}

        <FilterChips
            filterState={config.state}
            caseSensitive={config.filterChipsExtras.caseSensitive}
            scope={config.filterChipsExtras.scope}
            excludeSystemDirs={config.filterChipsExtras.excludeSystemDirs}
            searchableFolder={config.filterChipsExtras.searchableFolder}
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
            {highlightedFields}
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
            {scheduleSearch}
            onFocusBar={focusInput}
        />

        <QueryResults
            bind:this={queryResultsComponent}
            {results}
            {cursorIndex}
            isIndexAvailable={config.isIndexAvailable}
            isIndexReady={config.isIndexReady}
            {isSearching}
            {hasSearched}
            {query}
            {sizeFilter}
            {dateFilter}
            scanning={config.scanning}
            entriesScanned={config.entriesScanned}
            {totalCount}
            indexEntryCount={config.indexEntryCount}
            iconCacheVersion={iconVersion}
            aiEnabled={config.aiEnabled}
            showPathColumn={config.showPathColumn}
            onResultClick={handleResultClick}
            onHover={handleHover}
            onPickExample={pickExample}
            emptyExamples={config.emptyState.examples}
            onPickPath={config.onPickPath}
            onRowMenu={openRowMenu}
        />

        <div class="dialog-footer" bind:this={footerRef}>
            <div class="footer-left">
                <RecentItemsFooter
                    entries={recentEntries}
                    adapter={config.recentItems.adapter}
                    keyFn={config.recentItems.keyFn}
                    disabled={config.inputsDisabled}
                    onPick={config.onActivateRecent}
                    onRemove={config.onRemoveRecent}
                    onOpenAll={openRecentPopover}
                    leadingLabel={config.recentItems.leadingLabel}
                    trailingLabel={config.recentItems.trailingLabel}
                    trailingTooltipText={config.recentItems.trailingTooltipText}
                    trailingShortcut={config.recentItems.trailingShortcut}
                    ariaRegionLabel={config.recentItems.ariaRegionLabel}
                    ariaAllButtonLabel={config.recentItems.ariaAllButtonLabel}
                />
            </div>
            <div class="footer-right">
                {#if config.secondaryAction || config.primaryAction}
                    <div class="query-dialog__actions" role="group" aria-label="Dialog actions">
                        {#if config.secondaryAction}
                            <button
                                type="button"
                                class="btn btn-secondary btn-mini"
                                disabled={config.inputsDisabled || results.length === 0}
                                onclick={activateSecondary}
                                aria-label={config.secondaryAction.ariaLabel ?? config.secondaryAction.label}
                                title={config.secondaryAction.tooltip ?? ''}
                            >
                                {config.secondaryAction.label}{#if enterAction === 'go-to-file'}<span
                                        class="shortcut-hint"
                                        aria-hidden="true">{config.secondaryAction.shortcutHint}</span
                                    >{/if}
                            </button>
                        {/if}
                        {#if config.primaryAction}
                            <button
                                type="button"
                                class="btn btn-primary btn-mini"
                                disabled={config.inputsDisabled || results.length === 0}
                                onclick={activatePrimary}
                                aria-label={config.primaryAction.ariaLabel ?? config.primaryAction.label}
                                title={config.primaryAction.tooltip ?? ''}
                            >
                                {config.primaryAction.label}<span
                                    class="shortcut-hint shortcut-on-primary"
                                    aria-hidden="true">{config.primaryAction.shortcutHint}</span
                                >
                            </button>
                        {/if}
                    </div>
                {/if}
            </div>
        </div>

        {#if footerRef}
            <RecentItemsPopover
                anchor={footerRef}
                open={recentPopoverOpen}
                entries={recentEntries}
                adapter={config.recentItems.adapter}
                keyFn={config.recentItems.keyFn}
                onClose={closeRecentPopover}
                onPick={config.onActivateRecent}
                onRemove={config.onRemoveRecent}
                filterPlaceholder={config.recentItems.filterPlaceholder}
                emptyMessage={config.recentItems.emptyMessage}
                ariaLabel={config.recentItems.popoverAriaLabel}
                ariaListboxLabel={config.recentItems.listboxAriaLabel}
            />
        {/if}
    </div>
</div>

<style>
    .search-overlay {
        position: fixed;
        /* Start below the title bar so the scrim never covers the OS window-drag
           region: the user can still drag the window while a dialog is open.
           `--titlebar-height` is per-window (see app.css § Window chrome). */
        inset: var(--titlebar-height) 0 0 0;
        background: var(--color-overlay-light);
        display: flex;
        justify-content: center;
        align-items: flex-start;
        padding-top: 10vh;
        z-index: var(--z-modal);
    }

    /* Dialog dimensions: width is consumer-driven via `config.maxWidth` (inline style
       on .search-dialog). The height never exceeds 80vh; the results region absorbs
       whatever room is left after the title + bar + chips + filters + footer. */
    .search-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-lg);
        width: 100%;
        max-height: 80vh;
        display: flex;
        flex-direction: column;
        box-shadow: var(--shadow-lg);
        overflow: hidden;
    }

    /* Title bar: 32px tall, centered. No close button (Escape is the only close path).
       Not in the Tab order — text only. Rendered as <h2> for semantics; we reset the
       default heading typography. */
    .query-dialog__title {
        margin: 0;
        height: 32px;
        gap: var(--spacing-xs);
        padding: 0 var(--spacing-lg);
        border-bottom: 1px solid var(--color-border-subtle);
        font-size: var(--font-size-md);
        font-weight: 500;
        color: var(--color-text-secondary);
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
    }

    /* Optional notice banner row. Selection's snapshot-pane mode uses this to
       surface "Matching what's shown in the list (the full path)"; Search passes
       undefined and the row doesn't render. */
    .query-dialog__notice {
        padding: var(--spacing-xs) var(--spacing-lg);
        background: var(--color-bg-primary);
        border-bottom: 1px solid var(--color-border-subtle);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        flex-shrink: 0;
    }

    .dialog-footer {
        display: flex;
        align-items: stretch;
        justify-content: space-between;
        gap: var(--spacing-sm);
        background: var(--color-bg-primary);
        border-top: 1px solid var(--color-border-subtle);
        flex-shrink: 0;
    }

    .footer-left {
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
    }

    .footer-right {
        flex: 0 0 auto;
    }

    .query-dialog__actions {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-lg);
    }

    .shortcut-hint {
        margin-left: var(--spacing-xs);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        opacity: 0.8;
    }

    .shortcut-hint.shortcut-on-primary {
        color: var(--color-accent-fg);
        opacity: 0.8;
    }
</style>
