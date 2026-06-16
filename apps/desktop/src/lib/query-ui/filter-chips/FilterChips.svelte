<script lang="ts">
    /**
     * SearchFilterChips: the chip strip that replaces the old filter row + scope row.
     *
     * Leads with a one-click `Both | Files | Folders` type toggle (a `ToggleGroup`, not a
     * popover — type is a 3-way mutually-exclusive choice where a popover would be friction).
     * Then the Pattern chip, then Size / Modified / Search in chips: clicking a chip opens a
     * popover with the controls. All filters are always visible (so few). See
     * `lib/query-ui/CLAUDE.md` for the rationale.
     *
     * Chip behavior:
     *   - Default state: shows just the label ("Size", "Modified", "Search in").
     *   - Configured: shows a value summary plus an `×` to clear ("Size: > 100 MB ×"). Clicking ×
     *     or pressing Backspace on the focused chip clears the filter back to default.
     *   - Tab cycles through chips; Enter/Space opens the popover; Esc inside the popover closes
     *     it without affecting the dialog.
     *
     * The dense popover bodies live in sibling components (`SizeFilterPopover`, `DateFilterPopover`,
     * `ScopeFilterPopover`). This component owns the chip strip, the open-chip state, and the three
     * keyboard-shortcut routers (the ⌥S / ⌥M / ⌥I openers and the scope-only ⌥C / ⌥V); it threads
     * `anchor` / `open` / `onClose` plus typed callbacks into each popover.
     *
     * Keyboard contract: ⌥C and ⌥V set / clear the scope while the scope popover is open. The
     * popover's footer also exposes them as buttons so mouse users have first-class access.
     */
    import { SvelteSet } from 'svelte/reactivity'
    import Chip from '$lib/ui/Chip.svelte'
    import SizeFilterPopover from './SizeFilterPopover.svelte'
    import DateFilterPopover from './DateFilterPopover.svelte'
    import ScopeFilterPopover from './ScopeFilterPopover.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { deriveSizeChip, deriveDateChip, deriveScopeChip, derivePatternChip } from './filter-chip-state'
    import type { QueryFilterState, SizeFilter, SizeUnit, DateFilter, TypeFilter } from '../query-filter-state.svelte'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'

    type FilterKey = 'size' | 'date' | 'scope'

    interface Props {
        /**
         * The query-filter state instance owning size/date/case/setQuery setters. Passed
         * by the consumer wrapper (Search wires the search instance; Selection wires its
         * own). Named `filterState` (not `state`) to avoid shadowing Svelte's `$state`
         * rune.
         */
        filterState: QueryFilterState
        caseSensitive: boolean
        scope: string
        excludeSystemDirs: boolean
        /**
         * D12: smart "current folder" the Search-in popover's "Use current folder"
         * button acts on. When the focused pane is a search-results snapshot, this
         * walks back to the most recent real folder; when none exists, the button
         * renders disabled with `disabledReason` as its tooltip.
         *
         * Replaces the round-1 `currentFolderPath` prop entirely: the dialog's host
         * already computes the smart fallback (`getFocusedPaneSearchableFolder()`),
         * and there's no reason to also pass the raw path through.
         */
        searchableFolder: {
            path: string | null
            disabled: boolean
            disabledReason: string
        }
        sizeFilter: SizeFilter
        sizeValue: string
        sizeUnit: SizeUnit
        sizeValueMax: string
        sizeUnitMax: SizeUnit
        dateFilter: DateFilter
        dateValue: string
        dateValueMax: string
        /** Current `Both | Files | Folders` type filter (core state, both dialogs show it). */
        typeFilter: TypeFilter
        systemDirExcludeTooltip: string
        highlightedFields: SvelteSet<string>
        disabled: boolean
        /** Active search mode. Drives which input the Pattern chip reads from. */
        mode: 'ai' | 'filename' | 'regex'
        /** The bar's current contents (filename pattern or regex pattern). */
        query: string
        /** The AI-produced pattern (separate from the bar; AI bar holds the prompt). */
        aiPattern: string | null
        /**
         * Whether to render the "Search in" (scope) chip and its popover. Search renders this
         * `true` (scope is core to whole-drive search); Selection passes `false` because
         * selection runs against a single in-memory folder. Default `true` matches Search.
         */
        scopeChipVisible?: boolean
        /**
         * Whether to render the Pattern chip. Both Search and Selection render it (the chip
         * surfaces the AI-translated pattern in AI mode), so the default is `true`. The prop
         * exists for future consumers that don't surface a pattern at all.
         */
        patternChipVisible?: boolean
        onInput: (setter: (v: string) => void, search?: boolean) => (e: Event) => void
        onToggleCaseSensitive: () => void
        onToggleExcludeSystemDirs: () => void
        onSetScope: (path: string) => void
        /**
         * Called when the user clicks the Pattern chip's `×` while in AI mode. Search clears
         * its AI-extras `lastAiPattern`. Selection has no Pattern chip today, but the callback
         * is wired so the same component can be reused if Selection later opts in.
         */
        onClearAiPattern: () => void
        scheduleSearch: () => void
        /**
         * Called when the user activates the Pattern chip (click). The parent focuses
         * the bar so the user can edit the pattern. The bar's contents in AI mode is
         * the natural-language prompt, not the pattern; that's intentional.
         */
        onFocusBar: () => void
    }

    const {
        filterState,
        caseSensitive,
        scope,
        excludeSystemDirs,
        searchableFolder,
        sizeFilter,
        sizeValue,
        sizeUnit,
        sizeValueMax,
        sizeUnitMax,
        dateFilter,
        dateValue,
        dateValueMax,
        typeFilter,
        systemDirExcludeTooltip,
        highlightedFields,
        disabled,
        mode,
        query,
        aiPattern,
        scopeChipVisible = true,
        patternChipVisible = true,
        onInput,
        onToggleCaseSensitive,
        onToggleExcludeSystemDirs,
        onSetScope,
        onClearAiPattern,
        scheduleSearch,
        onFocusBar,
    }: Props = $props()

    // Pull the setters from the injected state instance so we can thread them into the popover
    // children. Arrow-function wrappers (not raw method refs) keep `this` binding intact and
    // silence `@typescript-eslint/unbound-method` since the factory's setters don't read `this`
    // but the type system can't prove it.
    const setSizeFilter: typeof filterState.setSizeFilter = (v) => { filterState.setSizeFilter(v); }
    const setSizeValue: typeof filterState.setSizeValue = (v) => { filterState.setSizeValue(v); }
    const setSizeUnit: typeof filterState.setSizeUnit = (v) => { filterState.setSizeUnit(v); }
    const setSizeValueMax: typeof filterState.setSizeValueMax = (v) => { filterState.setSizeValueMax(v); }
    const setSizeUnitMax: typeof filterState.setSizeUnitMax = (v) => { filterState.setSizeUnitMax(v); }
    const setDateFilter: typeof filterState.setDateFilter = (v) => { filterState.setDateFilter(v); }
    const setDateValue: typeof filterState.setDateValue = (v) => { filterState.setDateValue(v); }
    const setDateValueMax: typeof filterState.setDateValueMax = (v) => { filterState.setDateValueMax(v); }
    const setQueryFromUserInput: typeof filterState.setQueryFromUserInput = (v) => { filterState.setQueryFromUserInput(v); }

    // Type toggle: one-click `Both | Files | Folders`. Lives in the core state (both
    // dialogs show it), leading the chip strip so it reads "show [files] where size > …".
    const TYPE_FILTER_OPTIONS = $derived<ToggleGroupOption[]>([
        { value: 'both', label: tString('queryUi.filters.type.both') },
        { value: 'file', label: tString('queryUi.filters.type.files') },
        { value: 'folder', label: tString('queryUi.filters.type.folders') },
    ])
    function onTypeFilterChange(value: string): void {
        filterState.setTypeFilter(value as TypeFilter)
        scheduleSearch()
    }

    let openChip = $state<FilterKey | null>(null)

    /**
     * Match a plain `Alt+<letter>` key (lowercased), with no other modifiers. Centralized to
     * keep the `svelte:window` keydown handler under the cyclomatic-complexity cap.
     *
     * On macOS, Option+<letter> remaps `event.key` to a typographic glyph (Option+S → "ß",
     * Option+M → "µ"), so we'd miss the shortcut if we only checked `e.key`. `event.code` is
     * the layout-stable physical-key identifier (always `KeyS`, `KeyM`, etc.) and is the
     * right thing to match against for these chords. We still check `e.key` as a fallback so
     * synthesized events from tests (which carry `e.key` but no `e.code`) still work.
     */
    function altLetter(e: KeyboardEvent, letter: string): boolean {
        if (!e.altKey || e.metaKey || e.shiftKey || e.ctrlKey) return false
        const upper = letter.toUpperCase()
        if (e.code === `Key${upper}`) return true
        return e.key === letter || e.key === upper
    }

    /**
     * Dialog-wide ⌥S / ⌥M / ⌥I openers. Returns `true` if the key matched. Bail-out is done
     * by the caller; we just translate keys into popover openings.
     */
    function handleDialogPopoverOpener(e: KeyboardEvent): boolean {
        if (altLetter(e, 's')) {
            e.preventDefault()
            openPopover('size')
            return true
        }
        if (altLetter(e, 'm')) {
            e.preventDefault()
            openPopover('date')
            return true
        }
        if (scopeChipVisible && altLetter(e, 'i')) {
            e.preventDefault()
            openPopover('scope')
            return true
        }
        return false
    }

    /**
     * Scope-popover-only ⌥C / ⌥V. Active only while `openChip === 'scope'`. The caller (the
     * `svelte:window` handler) gates on the open chip; here we only translate keys into
     * actions. Returns `true` if matched.
     */
    function handleScopePopoverShortcut(e: KeyboardEvent): boolean {
        if (altLetter(e, 'c')) {
            e.preventDefault()
            // D12: respect the dialog's smart current-folder fallback so ⌥C does NOT seed an
            // unsearchable `search-results://...` URL into the scope.
            if (!searchableFolder.disabled && searchableFolder.path) {
                onSetScope(searchableFolder.path)
                scheduleSearch()
            }
            return true
        }
        if (altLetter(e, 'v')) {
            e.preventDefault()
            onSetScope('')
            scheduleSearch()
            return true
        }
        return false
    }

    let patternChipEl: HTMLButtonElement | undefined = $state()
    let sizeChipEl: HTMLButtonElement | undefined = $state()
    let dateChipEl: HTMLButtonElement | undefined = $state()
    let scopeChipEl: HTMLButtonElement | undefined = $state()

    // Pipe the user's file-size format through so the chip's KB/kB label matches
    // the popover (`kB` for SI, `KB` for binary).
    const sizeState = $derived(
        deriveSizeChip(sizeFilter, sizeValue, sizeUnit, sizeValueMax, sizeUnitMax, getFileSizeFormat()),
    )
    const dateState = $derived(deriveDateChip(dateFilter, dateValue, dateValueMax))
    const scopeState = $derived(deriveScopeChip(scope, excludeSystemDirs))
    const patternState = $derived(derivePatternChip({ mode, query, aiPattern }))

    /**
     * Clears the active pattern across all modes. Clicking the Pattern chip's
     * `×` clears the pattern but does NOT hide the AI transparency strip (that
     * lives separately on `lastAiPrompt`). In filename / regex mode we clear
     * the bar; in AI mode we clear the AI-produced pattern slot.
     */
    function clearPattern(): void {
        if (mode === 'ai') {
            onClearAiPattern()
        } else {
            setQueryFromUserInput('')
        }
        scheduleSearch()
    }

    /** Chips that should be visible in the strip. All filters are always visible (so few). */
    const visibleChips = $derived.by<FilterKey[]>(() => {
        // Selection (and any consumer with `scopeChipVisible: false`) drops the scope chip.
        const list: FilterKey[] = ['size', 'date']
        if (scopeChipVisible) list.push('scope')
        return list
    })

    function openPopover(key: FilterKey): void {
        if (disabled) return
        openChip = key
    }

    function closePopover(): void {
        openChip = null
    }

    function clearSize(): void {
        setSizeFilter('any')
        setSizeValue('')
        setSizeValueMax('')
        scheduleSearch()
    }

    function clearDate(): void {
        setDateFilter('any')
        setDateValue('')
        setDateValueMax('')
        scheduleSearch()
    }

    function clearScope(): void {
        onSetScope('')
        if (!excludeSystemDirs) onToggleExcludeSystemDirs()
        scheduleSearch()
    }
</script>

<!-- Filter chip strip. The Type toggle leads the strip (one-click `Both | Files | Folders`),
     then the Pattern chip, then Size / Modified / Search in. The Pattern chip's value comes
     from the bar in filename / regex mode and from the AI-produced pattern in AI mode, so the
     user sees the actual pattern being applied across every mode. See `lib/query-ui/CLAUDE.md`
     for the rationale. -->
<div class="filter-chip-strip" role="toolbar" aria-label={tString('queryUi.filters.toolbarAria')}>
    <!-- The flash wrapper mirrors the chips' `is-highlighted` treatment for the AI handoff:
         the AI may set the type, so we briefly tint the toggle when it does. ToggleGroup
         has no `highlighted` prop, so the wrapper carries the flash. -->
    <span class="type-toggle-flash" class:is-highlighted={highlightedFields.has('type')}>
        <ToggleGroup
            semantics="toggles"
            value={typeFilter}
            options={TYPE_FILTER_OPTIONS}
            onChange={onTypeFilterChange}
            ariaLabel={tString('queryUi.filters.type.aria')}
            {disabled}
        />
    </span>
    {#if patternChipVisible}
        <Chip
            bind:chipElement={patternChipEl}
            label={tString('queryUi.filters.chip.pattern')}
            value={patternState.summary}
            configured={patternState.configured}
            isOpen={false}
            {disabled}
            highlighted={highlightedFields.has('pattern')}
            onActivate={() => {
                onFocusBar()
            }}
            onClear={clearPattern}
        />
    {/if}
    {#each visibleChips as key (key)}
        {#if key === 'size'}
            <Chip
                bind:chipElement={sizeChipEl}
                label={tString('queryUi.filters.chip.size')}
                value={sizeState.summary}
                configured={sizeState.configured}
                isOpen={openChip === 'size'}
                {disabled}
                highlighted={highlightedFields.has('size')}
                onActivate={() => {
                    openPopover('size')
                }}
                onClear={clearSize}
            />
        {:else if key === 'date'}
            <Chip
                bind:chipElement={dateChipEl}
                label={tString('queryUi.filters.chip.modified')}
                value={dateState.summary}
                configured={dateState.configured}
                isOpen={openChip === 'date'}
                {disabled}
                highlighted={highlightedFields.has('date')}
                onActivate={() => {
                    openPopover('date')
                }}
                onClear={clearDate}
            />
        {:else if key === 'scope'}
            <Chip
                bind:chipElement={scopeChipEl}
                label={tString('queryUi.filters.chip.scope')}
                value={scopeState.summary}
                configured={scopeState.configured}
                isOpen={openChip === 'scope'}
                {disabled}
                highlighted={highlightedFields.has('scope')}
                onActivate={() => {
                    openPopover('scope')
                }}
                onClear={clearScope}
            />
        {/if}
    {/each}
</div>

{#if sizeChipEl}
    <SizeFilterPopover
        anchor={sizeChipEl}
        open={openChip === 'size'}
        onClose={closePopover}
        {sizeFilter}
        {sizeValue}
        {sizeUnit}
        {sizeValueMax}
        {sizeUnitMax}
        {setSizeFilter}
        {setSizeValue}
        {setSizeUnit}
        {setSizeValueMax}
        {setSizeUnitMax}
        {onInput}
        {scheduleSearch}
    />
{/if}

{#if dateChipEl}
    <DateFilterPopover
        anchor={dateChipEl}
        open={openChip === 'date'}
        onClose={closePopover}
        {dateFilter}
        {dateValue}
        {dateValueMax}
        {setDateFilter}
        {setDateValue}
        {setDateValueMax}
        {onInput}
        {scheduleSearch}
    />
{/if}

<!-- D9 + round-2 popover shortcuts. All of the following live on a single
     window listener so we keep the dialog-level keymap close to where the
     popovers it targets are defined.
     - ⌥C / ⌥V (only while the Scope popover is open) — Use current folder /
       All folders. D9 contract.
     - ⌥S / ⌥M / ⌥I (any time, while the dialog is mounted) — open the Size /
       Modified / Search-in popover. D10 / D11 brief calls for these as global
       (in-dialog) shortcuts that focus the first column. -->
<svelte:window onkeydown={(e: KeyboardEvent) => {
    // Bail when the chip strip is disabled (index not ready); shortcuts target controls that
    // aren't usable yet, and we don't want to swallow keys the rest of the dialog wants.
    if (disabled) return
    if (handleDialogPopoverOpener(e)) return
    if (openChip === 'scope') {
        handleScopePopoverShortcut(e)
    }
}} />

{#if scopeChipEl}
    <ScopeFilterPopover
        anchor={scopeChipEl}
        open={openChip === 'scope'}
        onClose={closePopover}
        {scope}
        {excludeSystemDirs}
        {caseSensitive}
        {searchableFolder}
        {systemDirExcludeTooltip}
        {onInput}
        {onSetScope}
        {onToggleCaseSensitive}
        {onToggleExcludeSystemDirs}
        {scheduleSearch}
    />
{/if}

<style>
    .filter-chip-strip {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-subtle);
        border-bottom: 1px solid var(--color-border-subtle);
        flex-wrap: wrap;
    }

    /* AI-handoff flash for the type toggle. Mirrors `FilterChip.is-highlighted`: a brief
       accent tint that fades over 1.5 s, drawing the eye to the type the agent just set. */
    .type-toggle-flash {
        display: inline-flex;
        border-radius: var(--radius-sm);
    }

    .type-toggle-flash.is-highlighted {
        background: var(--color-accent-subtle);
        transition: background 1.5s ease-out;
    }

    /* Match the dialog's one-step-larger font: bump the `Both | Files | Folders` toggle cells
       here only (the shared `ToggleGroup` stays `--font-size-sm` in Settings). `:global` because
       `ToggleGroup` renders its `.tg-*` nodes via `:global` (see ToggleGroup.svelte). */
    .type-toggle-flash :global(.tg-root .tg-item) {
        font-size: var(--font-size-md);
    }
</style>
