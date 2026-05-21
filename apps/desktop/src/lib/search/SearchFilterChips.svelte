<script lang="ts">
    /**
     * SearchFilterChips: the chip strip that replaces the old filter row + scope row.
     *
     * Each filter (Size, Modified, Search in) renders as a single chip. Clicking the chip opens
     * a popover with the controls. A trailing "+ Add filter" chip surfaces filters that are
     * currently in their default state; when all three are configured, the Add filter chip
     * disappears. See `docs/specs/search-redesign-plan.md` §3.2 for the full spec.
     *
     * Chip behavior:
     *   - Default state: shows just the label ("Size", "Modified", "Search in").
     *   - Configured: shows a value summary plus an `×` to clear ("Size: > 100 MB ×"). Clicking ×
     *     or pressing Backspace on the focused chip clears the filter back to default.
     *   - Tab cycles through chips; Enter/Space opens the popover; Esc inside the popover closes
     *     it without affecting the dialog.
     *
     * Keyboard contract: ⌥F and ⌥D set / clear the scope and are wired at the dialog level
     * (`SearchDialog.svelte` → `handleModifierShortcuts`). They work regardless of whether the
     * scope popover is open. The popover's footer also exposes them as buttons so mouse users
     * have first-class access.
     */
    import { SvelteSet } from 'svelte/reactivity'
    import { tooltip } from '$lib/tooltip/tooltip'
    import FilterChip from './FilterChip.svelte'
    import FilterChipPopover from './FilterChipPopover.svelte'
    import { deriveSizeChip, deriveDateChip, deriveScopeChip } from './filter-chip-state'
    import {
        setSizeFilter,
        setSizeValue,
        setSizeUnit,
        setSizeValueMax,
        setSizeUnitMax,
        setDateFilter,
        setDateValue,
        setDateValueMax,
        setScope,
    } from './search-state.svelte'
    import type { SizeFilter, SizeUnit, DateFilter } from './search-state.svelte'

    type FilterKey = 'size' | 'date' | 'scope'

    interface Props {
        caseSensitive: boolean
        scope: string
        excludeSystemDirs: boolean
        currentFolderPath: string
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
        onInput: (setter: (v: string) => void, search?: boolean) => (e: Event) => void
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- T constrains the setter's param type to match the cast
        onSelect: <T extends string>(setter: (v: T) => void, search?: boolean) => (e: Event) => void
        onToggleCaseSensitive: () => void
        onToggleExcludeSystemDirs: () => void
        onSetScope: (path: string) => void
        scheduleSearch: () => void
    }

    const {
        caseSensitive,
        scope,
        excludeSystemDirs,
        currentFolderPath,
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
        onInput,
        onSelect,
        onToggleCaseSensitive,
        onToggleExcludeSystemDirs,
        onSetScope,
        scheduleSearch,
    }: Props = $props()

    let openChip = $state<FilterKey | 'add' | null>(null)

    let sizeChipEl: HTMLButtonElement | undefined = $state()
    let dateChipEl: HTMLButtonElement | undefined = $state()
    let scopeChipEl: HTMLButtonElement | undefined = $state()
    let addChipEl: HTMLButtonElement | undefined = $state()

    const sizeState = $derived(deriveSizeChip(sizeFilter, sizeValue, sizeUnit, sizeValueMax, sizeUnitMax))
    const dateState = $derived(deriveDateChip(dateFilter, dateValue, dateValueMax))
    const scopeState = $derived(deriveScopeChip(scope, excludeSystemDirs))

    /** Which filters should appear in the "Add filter" dropdown. Configured filters are absent. */
    const availableToAdd = $derived.by<FilterKey[]>(() => {
        const list: FilterKey[] = []
        if (!sizeState.configured) list.push('size')
        if (!dateState.configured) list.push('date')
        if (!scopeState.configured) list.push('scope')
        return list
    })

    /** Whether to render the trailing Add filter chip. Hidden when nothing's left to add. */
    const showAddFilter = $derived(availableToAdd.length > 0)

    /** Chips that should be visible in the strip (always-on for configured filters). */
    const visibleChips = $derived.by<FilterKey[]>(() => {
        // Default behavior: show all three filters always, since they're so few. The Add filter
        // chip is the discoverability hint, not a gate. This matches §3.2's intent ("the affordance
        // the user reads as 'I can add filters'") while keeping the existing filters one click away.
        return ['size', 'date', 'scope']
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
        setScope('')
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

<!-- Filter chip strip. Replaces the old `.filter-row` and `.input-row` (scope) sections. -->
<div class="filter-chip-strip" role="toolbar" aria-label="Search filters">
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

<!-- Size popover -->
{#if sizeChipEl}
    <FilterChipPopover
        anchor={sizeChipEl}
        open={openChip === 'size'}
        onClose={closePopover}
        ariaLabel="Size filter options"
    >
        <div class="popover-section">
            <label class="popover-label" for="popover-size-filter">Size</label>
            <div class="popover-row">
                <select
                    id="popover-size-filter"
                    class="popover-select"
                    value={sizeFilter}
                    onchange={onSelect<SizeFilter>(setSizeFilter)}
                    aria-label="Size comparator"
                >
                    <option value="any">any</option>
                    <option value="gte">&ge;</option>
                    <option value="lte">&le;</option>
                    <option value="between">between</option>
                </select>
                {#if sizeFilter !== 'any'}
                    <input
                        type="number"
                        class="popover-input size-input"
                        value={sizeValue}
                        oninput={onInput(setSizeValue)}
                        aria-label="Minimum size value"
                        min="0"
                        step="any"
                    />
                    <select
                        class="popover-select unit-select"
                        value={sizeUnit}
                        onchange={onSelect<SizeUnit>(setSizeUnit)}
                        aria-label="Size unit"
                    >
                        <option value="KB">KB</option>
                        <option value="MB">MB</option>
                        <option value="GB">GB</option>
                    </select>
                {/if}
            </div>
            {#if sizeFilter === 'between'}
                <div class="popover-row">
                    <span class="popover-separator">to</span>
                    <input
                        type="number"
                        class="popover-input size-input"
                        value={sizeValueMax}
                        oninput={onInput(setSizeValueMax)}
                        aria-label="Maximum size value"
                        min="0"
                        step="any"
                    />
                    <select
                        class="popover-select unit-select"
                        value={sizeUnitMax}
                        onchange={onSelect<SizeUnit>(setSizeUnitMax)}
                        aria-label="Maximum size unit"
                    >
                        <option value="KB">KB</option>
                        <option value="MB">MB</option>
                        <option value="GB">GB</option>
                    </select>
                </div>
            {/if}
        </div>
    </FilterChipPopover>
{/if}

<!-- Modified popover -->
{#if dateChipEl}
    <FilterChipPopover
        anchor={dateChipEl}
        open={openChip === 'date'}
        onClose={closePopover}
        ariaLabel="Modified filter options"
    >
        <div class="popover-section">
            <label class="popover-label" for="popover-date-filter">Modified</label>
            <div class="popover-row">
                <select
                    id="popover-date-filter"
                    class="popover-select"
                    value={dateFilter}
                    onchange={onSelect<DateFilter>(setDateFilter)}
                    aria-label="Date comparator"
                >
                    <option value="any">any</option>
                    <option value="after">after</option>
                    <option value="before">before</option>
                    <option value="between">between</option>
                </select>
                {#if dateFilter !== 'any'}
                    <input
                        type="date"
                        class="popover-input date-input"
                        value={dateValue}
                        oninput={onInput(setDateValue)}
                        aria-label="Date value"
                    />
                {/if}
            </div>
            {#if dateFilter === 'between'}
                <div class="popover-row">
                    <span class="popover-separator">to</span>
                    <input
                        type="date"
                        class="popover-input date-input"
                        value={dateValueMax}
                        oninput={onInput(setDateValueMax)}
                        aria-label="Maximum date value"
                    />
                </div>
            {/if}
        </div>
    </FilterChipPopover>
{/if}

<!-- Scope ("Search in") popover -->
{#if scopeChipEl}
    <FilterChipPopover
        anchor={scopeChipEl}
        open={openChip === 'scope'}
        onClose={closePopover}
        ariaLabel="Search in folders"
    >
        <div class="popover-section scope-popover">
            <label class="popover-label" for="popover-scope">Search in</label>
            <textarea
                id="popover-scope"
                class="popover-textarea"
                placeholder="All folders"
                value={scope}
                oninput={onInput(setScope)}
                aria-label="Scope folders"
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                rows="3"
            ></textarea>
            <div class="scope-hint">
                Comma-separated paths. Prefix with <code>!</code> to exclude. Wildcards
                <code>*</code> and <code>?</code> work.
            </div>
            <div class="popover-row scope-toggles">
                <label class="popover-checkbox">
                    <input
                        type="checkbox"
                        checked={excludeSystemDirs}
                        onchange={() => {
                            onToggleExcludeSystemDirs()
                        }}
                        aria-label="Hide system folders"
                    />
                    <span use:tooltip={{ html: systemDirExcludeTooltip }}>Hide system folders</span>
                </label>
                <label class="popover-checkbox">
                    <input
                        type="checkbox"
                        checked={caseSensitive}
                        onchange={() => {
                            onToggleCaseSensitive()
                        }}
                        aria-label="Case-sensitive matching"
                    />
                    <span>Case-sensitive</span>
                </label>
            </div>
            <div class="popover-footer">
                <button
                    type="button"
                    class="footer-button"
                    onclick={() => {
                        onSetScope(currentFolderPath)
                        scheduleSearch()
                    }}
                >
                    Use current folder
                    <kbd class="footer-kbd">⌥F</kbd>
                </button>
                <button
                    type="button"
                    class="footer-button"
                    onclick={() => {
                        onSetScope('')
                        scheduleSearch()
                    }}
                >
                    All folders
                    <kbd class="footer-kbd">⌥D</kbd>
                </button>
            </div>
        </div>
    </FilterChipPopover>
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

    /* ===== Popover contents ===== */

    .popover-section {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .popover-label {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        text-transform: uppercase;
        letter-spacing: 0.04em;
        font-weight: 600;
    }

    .popover-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .popover-select,
    .popover-input {
        font-size: var(--font-size-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 2px 6px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .popover-select:focus,
    .popover-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .size-input {
        width: 70px;
    }

    .date-input {
        width: 130px;
    }

    .unit-select {
        width: auto;
    }

    .popover-separator {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    /* ===== Scope popover ===== */

    .scope-popover {
        min-width: 320px;
    }

    .popover-textarea {
        width: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 6px 8px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
        resize: vertical;
        line-height: 1.4;
    }

    .popover-textarea:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .scope-hint {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.4;
    }

    .scope-hint code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 3px;
        border-radius: var(--radius-xs);
    }

    .scope-toggles {
        flex-wrap: wrap;
        gap: var(--spacing-md);
    }

    .popover-checkbox {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .popover-footer {
        display: flex;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xs);
        border-top: 1px solid var(--color-border-subtle);
        margin-top: var(--spacing-xs);
    }

    .footer-button {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        line-height: 1;
    }

    .footer-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .footer-kbd {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        font-weight: 500;
        color: var(--color-accent-text);
        background: var(--color-accent-subtle);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border-radius: var(--radius-sm);
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
