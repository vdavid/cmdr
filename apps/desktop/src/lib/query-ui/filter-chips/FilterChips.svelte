<script lang="ts">
    /**
     * SearchFilterChips: the chip strip that replaces the old filter row + scope row.
     *
     * Each filter (Size, Modified, Search in) renders as a single chip. Clicking the chip opens
     * a popover with the controls. A trailing "+ Add filter" chip surfaces filters that are
     * currently in their default state; when all three are configured, the Add filter chip
     * disappears. See `lib/query-ui/CLAUDE.md` § "Filter chips with popovers" for the rationale.
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
    import FilterChip from './FilterChip.svelte'
    import FilterChipPopover from './FilterChipPopover.svelte'
    import SizeFilterPopover from './SizeFilterPopover.svelte'
    import DateFilterPopover from './DateFilterPopover.svelte'
    import ScopeFilterPopover from './ScopeFilterPopover.svelte'
    import { deriveSizeChip, deriveDateChip, deriveScopeChip, derivePatternChip } from './filter-chip-state'
    import type { QueryFilterState, SizeFilter, SizeUnit, DateFilter } from '../query-filter-state.svelte'
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

    let openChip = $state<FilterKey | 'add' | null>(null)

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
    let addChipEl: HTMLButtonElement | undefined = $state()

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

    /** Which filters should appear in the "Add filter" dropdown. Configured filters are absent. */
    const availableToAdd = $derived.by<FilterKey[]>(() => {
        const list: FilterKey[] = []
        if (!sizeState.configured) list.push('size')
        if (!dateState.configured) list.push('date')
        if (scopeChipVisible && !scopeState.configured) list.push('scope')
        return list
    })

    /** Whether to render the trailing Add filter chip. Hidden when nothing's left to add. */
    const showAddFilter = $derived(availableToAdd.length > 0)

    /** Chips that should be visible in the strip (always-on for configured filters). */
    const visibleChips = $derived.by<FilterKey[]>(() => {
        // Default behavior: show all three filters always, since they're so few. The Add filter
        // chip is the discoverability hint, not a gate. This matches §3.2's intent ("the affordance
        // the user reads as 'I can add filters'") while keeping the existing filters one click away.
        // Selection (and any consumer with `scopeChipVisible: false`) drops the scope chip from
        // the always-on set.
        const list: FilterKey[] = ['size', 'date']
        if (scopeChipVisible) list.push('scope')
        return list
    })

    function openPopover(key: FilterKey | 'add'): void {
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

    /** Adds a default filter by opening its popover and seeding a sensible comparator. */
    function addFilter(key: FilterKey): void {
        if (key === 'size') setSizeFilter('gte')
        else if (key === 'date') setDateFilter('after')
        openChip = key
    }

    function addFilterLabel(key: FilterKey): string {
        if (key === 'size') return 'Size'
        if (key === 'date') return 'Modified'
        return 'Search in'
    }
</script>

<!-- Filter chip strip. The Pattern chip is ALWAYS rendered ahead of Size / Modified /
     Search in. Its value comes from the bar in filename / regex mode and from the
     AI-produced pattern in AI mode, so the user sees the actual pattern being applied
     across every mode. See `lib/query-ui/CLAUDE.md` for the rationale. -->
<div class="filter-chip-strip" role="toolbar" aria-label="Search filters">
    {#if patternChipVisible}
        <FilterChip
            bind:chipElement={patternChipEl}
            label="Pattern"
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
            <FilterChip
                bind:chipElement={sizeChipEl}
                label="Size"
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
            <FilterChip
                bind:chipElement={dateChipEl}
                label="Modified"
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
            <FilterChip
                bind:chipElement={scopeChipEl}
                label="Search in"
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

    {#if showAddFilter}
        <button
            bind:this={addChipEl}
            type="button"
            class="add-filter-chip"
            class:is-open={openChip === 'add'}
            aria-haspopup="menu"
            aria-expanded={openChip === 'add'}
            aria-label="Add filter"
            {disabled}
            onclick={() => {
                openPopover('add')
            }}
        >
            <span class="add-glyph" aria-hidden="true">+</span>
            <span>Add filter</span>
        </button>
    {/if}
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

<!-- Add filter dropdown -->
{#if addChipEl}
    <FilterChipPopover
        anchor={addChipEl}
        open={openChip === 'add'}
        onClose={closePopover}
        ariaLabel="Add a filter"
    >
        <div class="add-filter-menu" role="menu" aria-label="Add a filter">
            {#each availableToAdd as key (key)}
                <button
                    type="button"
                    class="add-filter-item"
                    role="menuitem"
                    onclick={() => {
                        addFilter(key)
                    }}
                >
                    {addFilterLabel(key)}
                </button>
            {/each}
        </div>
    </FilterChipPopover>
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

    /* Add filter chip: visually distinct from a configured chip (dashed border, glyph-led label),
       so the eye reads it as a control surface rather than a filled value. */
    .add-filter-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-tertiary);
        background: transparent;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .add-filter-chip:not(:disabled):hover,
    .add-filter-chip.is-open {
        background: var(--color-bg-tertiary);
        border-color: var(--color-border-strong);
        color: var(--color-text-primary);
    }

    .add-filter-chip:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .add-glyph {
        font-size: var(--font-size-md);
        line-height: 1;
    }

    /* ===== Add filter menu ===== */

    .add-filter-menu {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 160px;
    }

    .add-filter-item {
        display: block;
        text-align: left;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 6px 10px;
        font-size: var(--font-size-sm);
        background: transparent;
        border: none;
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        line-height: 1.2;
    }

    .add-filter-item:hover,
    .add-filter-item:focus-visible {
        background: var(--color-accent-subtle);
        outline: none;
    }
</style>
