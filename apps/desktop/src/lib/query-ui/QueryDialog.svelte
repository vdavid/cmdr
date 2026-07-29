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
     * into `align="top"`, `fillBody`, `padded={false}`, `ownsKeyboard`, and
     * `closeOnOverlayClick`; see DETAILS.md § Chrome.
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
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import type { SearchResultEntry } from '$lib/tauri-commands'
    import { iconCacheVersion } from '$lib/icon-cache'
    import QueryBar from './QueryBar.svelte'
    import ModeChips from './ModeChips.svelte'
    import FilterChips from './filter-chips/FilterChips.svelte'
    import QueryResults from './QueryResults.svelte'
    import AiPromptStrip from './AiPromptStrip.svelte'
    import { buildAiSummary } from './ai-summary'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import RecentItemsPopover from './recent-items/RecentItemsPopover.svelte'
    import { deriveEnterAction, SEARCH_AUTO_APPLY_DEBOUNCE_MS, type SearchMode } from './query-filter-state.svelte'
    import type { QueryDialogConfig } from './query-dialog-config'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { addToast } from '$lib/ui/toast/toast-store.svelte'
    import { tString } from '$lib/intl/messages.svelte'
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
    /** The query field's pill frame; the recent-items dropdown anchors to it. */
    let queryFieldElement: HTMLElement | undefined = $state()
    let recentPopoverOpen = $state(false)
    let debounceTimer: ReturnType<typeof setTimeout> | undefined
    let unlistenAutoApply: (() => void) | undefined
    let highlightedFields: SvelteSet<string> = new SvelteSet<string>()
    /**
     * Whether a run's results are the current content. Seeded from the surviving state so a
     * close + reopen shows the SAME results immediately, not the empty state. `lastRunQuery`
     * is non-null exactly when a run has landed and hasn't been cleared by `⌘N` (`clearCore`
     * nulls it), so it's the precise "a prior run exists" signal. On a first-ever open it's
     * null, so we still start on the empty state. For non-AI restored sessions we additionally
     * re-run on mount (see `onMount`) so Select re-derives against the current folder; AI
     * restored sessions render these persisted results WITHOUT re-calling the cloud.
     */
    let hasSearched = $state(config.state.getLastRunQuery() !== null)
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
     * The AI-produced pattern for the transparency strip, with its kind. Search exposes the
     * precise pattern + kind via `filterChipsExtras.aiPattern` / `aiPatternKind` (its dedicated
     * Pattern-chip slot). Selection has no Pattern chip and passes `null` there, but its
     * `translateAi` clears the other-kind `handTyped` buffer, so reading those buffers (regex
     * first, matching the matcher's precedence) is kind-correct as the fallback. The strip is the
     * human-readable mirror; the live chips stay the editable source of truth.
     */
    const aiPattern = $derived.by((): { pattern: string | null; kind: 'glob' | 'regex' | null } => {
        const extrasPattern = config.filterChipsExtras.aiPattern
        if (extrasPattern && extrasPattern.trim()) {
            return { pattern: extrasPattern, kind: config.filterChipsExtras.aiPatternKind }
        }
        const regexBuf = config.state.getHandTypedBuffer('regex')
        if (regexBuf && regexBuf.trim()) return { pattern: regexBuf, kind: 'regex' }
        const globBuf = config.state.getHandTypedBuffer('filename')
        if (globBuf && globBuf.trim()) return { pattern: globBuf, kind: 'glob' }
        return { pattern: null, kind: null }
    })

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
     * Whether the current state has anything runnable: a non-empty query OR an active filter
     * (size ≠ any, date ≠ any, or type ≠ both). The single source of truth for "is there a
     * session worth running?", shared by the `runOnMount` effect and the reopen re-run gate in
     * `onMount`. Type counts: a "Folders"-only Selection run is a valid filter-only query.
     */
    function hasRestorableQuery(): boolean {
        return (
            config.state.getQuery().trim() !== '' ||
            config.state.getSizeFilter() !== 'any' ||
            config.state.getDateFilter() !== 'any' ||
            config.state.getTypeFilter() !== 'both'
        )
    }

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
        if (trimmed && config.state.getMode() === 'ai' && config.aiEnabled) {
            void runAiSearch(trimmed)
        } else if (config.isIndexReady && hasRestorableQuery()) {
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
        if (dialogElement?.querySelector('.ui-popover')) {
            return
        }
        e.preventDefault()
        e.stopPropagation()
        config.onClose()
    }

    function focusInput(): void {
        queryInputElement?.focus()
    }

    function openRecentPopover(): void {
        recentPopoverOpen = true
    }

    /**
     * Closes the dropdown and makes sure focus lands back in the query field rather than
     * on the anchor or the body.
     *
     * `Popover`'s Escape path calls `onClose()` and then `anchor.focus()`; the anchor is the
     * pill frame (a `<div>`), which isn't focusable, so without this the focus would fall to
     * the document. Click-outside must NOT be stolen though, so the refocus is deferred one
     * frame and only fires when nothing else has claimed focus by then.
     */
    function closeRecentPopover(): void {
        recentPopoverOpen = false
        requestAnimationFrame(() => {
            const active = document.activeElement
            if (active === null || active === document.body || active === queryFieldElement) focusInput()
        })
    }

    /**
     * Closes the dropdown and puts the caret straight back in the field (keyboard paths).
     * The focus call waits a tick: while the popover is still mounted its own focus trap
     * would pull focus straight back.
     */
    function closeRecentPopoverAndFocus(): void {
        recentPopoverOpen = false
        void tick().then(() => {
            focusInput()
        })
    }

    function toggleRecentPopover(): void {
        if (recentPopoverOpen) closeRecentPopoverAndFocus()
        else openRecentPopover()
    }

    /**
     * A recent entry was picked: the consumer loads it into state, and we hand ⏎ back to
     * "run-search" so the very next Enter runs it. Picking never runs the query itself —
     * a recent search is a starting point to edit, and for AI mode a silent run would spend
     * the user's money on a keystroke they meant as navigation.
     */
    function pickRecent(entry: E): void {
        config.onActivateRecent(entry)
        config.state.setLastDialogEvent('query-edited')
        closeRecentPopoverAndFocus()
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
        if (
            config.state.getLastRunQuery() !== null &&
            config.state.getMode() !== 'ai' &&
            hasRestorableQuery()
        ) {
            config.state.setRunOnMount(true)
        }

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
        if (!config.isIndexReady) {
            // Bail before running, but clear any spinner `runAiSearch` turned on for the translate
            // round-trip (it sets `isSearching` before calling us). Without this the spinner sticks.
            config.state.setIsSearching(false)
            return
        }

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
        } catch (err) {
            // Surface WHY nothing came back. The backend refuses some runs with an
            // actionable message ("Query too broad. Add a filename pattern, size, date,
            // or type filter"); swallowing it left the user staring at an empty list that
            // reads as "nothing matched". Same one-place-for-both-consumers rule as the AI
            // path above. No typed variant crosses this IPC boundary, so we pass the
            // message through verbatim instead of classifying it by its text
            // (`.claude/rules/no-string-matching.md`).
            addToast(tString('queryUi.dialog.runQueryToast', { message: describeRunFailure(err) }), {
                level: 'warn',
                dismissal: 'transient',
            })
        } finally {
            config.state.setIsSearching(false)
        }
    }

    /** The backend's own message when there is one; a generic fallback otherwise. */
    function describeRunFailure(err: unknown): string {
        const message = err instanceof Error ? err.message : typeof err === 'string' ? err : ''
        return message.trim() || tString('queryUi.dialog.runQueryUnknownReason')
    }

    /**
     * Runs an AI translation for `prompt`, then executes the query. The consumer's
     * `translateAi` owns applying every AI-returned filter (size / date / scope /
     * AI pattern + label / etc); QueryDialog captures the prompt, flashes any
     * highlighted fields, sets the caveat, and runs the query.
     *
     * The spinner covers the WHOLE round-trip: we flip `isSearching` on before the
     * cloud translate (the slow part: seconds) and leave it on through `executeQuery`,
     * which clears it in its own `finally`. The early-return paths (empty prompt,
     * translate error, empty result) reset it themselves so it can't stick on.
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
        // Show the spinner for the slow cloud translate, not just the post-translate query.
        hasSearched = true
        config.state.setIsSearching(true)

        let result: Awaited<ReturnType<NonNullable<typeof config.translateAi>>>
        try {
            result = await config.translateAi(trimmed)
        } catch (err) {
            // Surface WHY the translation failed (quota, key, timeout, empty answer, …) as a
            // specific toast instead of a silent no-op. Both Search and Selection route here,
            // so the error UX lives in one place. The consumer's `translateAi` lets the typed
            // error throw; we map its `kind` to copy. A non-translation error (shouldn't happen)
            // falls through to a generic toast.
            config.state.setIsSearching(false)
            if (!showAiTranslateErrorToast(err)) {
                addToast(tString('queryUi.dialog.aiTranslateFailedToast'), { level: 'warn', dismissal: 'transient' })
            }
            return
        }
        if (!result) {
            config.state.setIsSearching(false)
            return
        }

        // Flash the changed fields for ~1.5 s so the user sees what the AI touched.
        if (result.highlightedFields && result.highlightedFields.length > 0) {
            const next = new SvelteSet<string>(result.highlightedFields)
            highlightedFields = next
            setTimeout(() => {
                highlightedFields = new SvelteSet<string>()
            }, 1500)
        }
        config.state.setLastAiCaveat(result.caveat)

        // `executeQuery` sets `isSearching` true again (idempotent) and clears it in `finally`.
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

    /**
     * Count-only switch (zone 1, beside the mode chips). Flipping it changes what the
     * backend returns, so re-run: `scheduleSearch` keeps AI mode's explicit-trigger
     * contract intact (it no-ops there, and the AI run costs money).
     */
    function toggleCountOnly(): void {
        if (config.inputsDisabled) return
        config.filterChipsExtras.onToggleCountOnly?.()
        scheduleSearch()
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
        runFromButton()
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
        if (e.shiftKey || e.ctrlKey) return false
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
        if (!e.metaKey || e.altKey || e.shiftKey || e.ctrlKey) return false
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
     * Handles ⌘N, ⌘H, ⌘1-9, ⌥A/F/R, ⌥⏎ (primary action), ⌘⏎/⇧⏎ no-op.
     *
     * ⌥← / ⌥→ are deliberately NOT handled: they're macOS's native move-by-word in
     * the focused query input, so the dialog leaves them alone (path pills are
     * mouse-only). See DETAILS.md § Path pills.
     */
    function handleModifierShortcuts(e: KeyboardEvent): boolean {
        if (matchKey(e, 'n', 'meta')) {
            e.preventDefault()
            clearAndRefocus()
            return true
        }
        if (handleModeChipShortcut(e)) return true
        if (handleEnterCombinations(e)) return true
        if (matchKey(e, 'h', 'meta')) {
            e.preventDefault()
            toggleRecentPopover()
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
     *
     * With no result list to walk, ArrowDown opens the recent-items dropdown instead — the
     * combobox gesture, landing on a key that was otherwise dead. Results win when they
     * exist: walking them is the more valuable use of ↓, and the chevron + `⌘H` open the
     * dropdown unconditionally.
     */
    function handleArrowNav(e: KeyboardEvent): void {
        const len = config.state.getResults().length
        if (len === 0) {
            if (e.key === 'ArrowDown' && !recentPopoverOpen && recentEntries.length > 0) {
                e.preventDefault()
                openRecentPopover()
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

<!--
  The dialog chrome (radius, panel edge, shadow, title bar, ×, focus trap, MCP
  registry, focus restore) is `ModalDialog`'s. We opt into:
    - `align="top"`      the Spotlight-style placement this dialog has always had.
    - `fillBody`         fixed-height frame; `.results-container` absorbs the slack.
    - `padded={false}`   every strip is full-bleed and pads itself.
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
    padded={false}
    ownsKeyboard
    closeOnOverlayClick
    containerStyle="width: 100%; max-width: {config.maxWidth}; max-height: 80vh;"
    onkeydown={handleKeyDown}
    onclose={config.onClose}
>
    <!-- The title text keeps its own `<span>`: the badge is a sibling, so consumers'
         tests (and anyone styling the two apart) can address the words alone. -->
    {#snippet title()}
        <span>{config.title}</span>{#if config.badge}<StatusBadge status={config.badge} />{/if}
    {/snippet}

    <div class="query-dialog-body" bind:this={dialogElement}>
        <!-- Zone 1: what to look for and how. -->
        <QueryBar
            bind:inputElement={queryInputElement}
            bind:fieldElement={queryFieldElement}
            {query}
            {mode}
            disabled={config.inputsDisabled}
            aiHighlight={highlightedFields.has('query')}
            {showRunHint}
            showEnterHint={enterAction === 'run-search'}
            recentOpen={recentPopoverOpen}
            onInput={handleQueryInput}
            onRun={runFromButton}
            onToggleRecent={toggleRecentPopover}
            recentTriggerLabel={config.recentItems.triggerAriaLabel ?? tString('queryUi.recent.allButtonAria')}
            recentTriggerTooltip={config.recentItems.triggerTooltip ?? tString('queryUi.recent.trailingTooltip')}
            onCompositionStart={handleCompositionStart}
            onCompositionEnd={handleCompositionEnd}
        />

        <!-- The mode chips span the dialog (`fullWidth`), so the Count-only switch rides
             beside them rather than inside the group. It belongs in zone 1: it changes
             what the search RETURNS, it isn't one more way to narrow the matches.
             Search wires `onToggleCountOnly`; Selection omits it and the switch is absent. -->
        <div class="mode-row">
            <div class="mode-row__chips">
                <ModeChips
                    {mode}
                    aiEnabled={config.aiEnabled}
                    disabled={config.inputsDisabled}
                    onSelect={handleModeChange}
                />
            </div>
            {#if config.filterChipsExtras.onToggleCountOnly}
                <div class="mode-row__count-only" use:tooltip={tString('queryUi.filters.countOnly.tooltip')}>
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

        <!-- Zone 3: the results. `.results-container` inside is the only `flex: 1 1 auto`
             child of the body, so it absorbs whatever room the strips leave. -->
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
            countOnly={config.filterChipsExtras.countOnly ?? false}
            onShowResults={config.filterChipsExtras.onToggleCountOnly ? showResultsFromCount : undefined}
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
                                onclick={activateSecondary}
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
                                onclick={activatePrimary}
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
                open={recentPopoverOpen}
                entries={recentEntries}
                adapter={config.recentItems.adapter}
                keyFn={config.recentItems.keyFn}
                onClose={closeRecentPopover}
                onPick={pickRecent}
                onRemove={config.onRemoveRecent}
                onExitTop={closeRecentPopoverAndFocus}
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

    /* Zone-1 mode row: the `fullWidth` mode chips take the room, the Count-only switch
       rides at the trailing end. `ModeChips` brings its own `--spacing-lg` inset, so the
       switch only pays for the right one. */
    .mode-row {
        display: flex;
        align-items: center;
        background: var(--color-bg-primary);
        flex-shrink: 0;
    }

    .mode-row__chips {
        flex: 1 1 auto;
        min-width: 0;
    }

    .mode-row__count-only {
        flex: 0 0 auto;
        display: flex;
        align-items: center;
        padding-right: var(--spacing-dialog);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    /* Optional notice banner row. Selection's snapshot-pane mode uses this to
       surface "Matching what's shown in the list (the full path)"; Search passes
       undefined and the row doesn't render. */
    .query-dialog__notice {
        padding: var(--spacing-xs) var(--spacing-dialog);
        background: var(--color-bg-primary);
        border-bottom: 1px solid var(--color-border-subtle);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        flex-shrink: 0;
    }

    /* The footer is the action row now: recent items live in the query field's dropdown. */
    .dialog-footer {
        display: flex;
        align-items: stretch;
        justify-content: flex-end;
        background: var(--color-bg-primary);
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
        padding: var(--spacing-sm) var(--spacing-dialog);
    }

    /* The action verb leads; the shortcut hint rides a standard `ShortcutChip` to its right. */
    .action-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
