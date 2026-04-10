<script lang="ts">
    /**
     * SearchInputArea — Pattern row + scope row + filter row.
     *
     * All query configuration inputs. The parent orchestrator owns search execution;
     * this component renders the inputs and fires callbacks on changes.
     */
    import { SvelteSet } from 'svelte/reactivity'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { SizeFilter, SizeUnit, DateFilter, PatternType } from './search-state.svelte'
    import {
        setNamePattern,
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

    interface Props {
        patternInputElement: HTMLInputElement | undefined
        namePattern: string
        patternType: PatternType
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
        onSearch: () => void
        onTogglePatternType: () => void
        onToggleCaseSensitive: () => void
        onToggleExcludeSystemDirs: () => void
        onSetScope: (path: string) => void
        scheduleSearch: () => void
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        patternInputElement = $bindable(),
        namePattern,
        patternType,
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
        onSearch,
        onTogglePatternType,
        onToggleCaseSensitive,
        onToggleExcludeSystemDirs,
        onSetScope,
        scheduleSearch,
    }: Props = $props()
    /* eslint-enable prefer-const */
</script>

<!-- Pattern / search row (always visible) -->
<div class="input-row">
    <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
        <circle cx="6.5" cy="6.5" r="5" stroke="currentColor" stroke-width="1.5" />
        <line x1="10.5" y1="10.5" x2="14.5" y2="14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
    <input
        bind:this={patternInputElement}
        type="text"
        class="name-input"
        class:ai-highlight={highlightedFields.has('name')}
        placeholder={patternType === 'regex'
            ? 'Regular expression pattern'
            : 'Filename pattern (use * and ? as wildcards)'}
        value={namePattern}
        oninput={onInput(setNamePattern)}
        {disabled}
        aria-label="Filename pattern"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
    />
    <button
        class="pattern-type-toggle"
        class:active={caseSensitive}
        class:ai-highlight={highlightedFields.has('caseSensitive')}
        onclick={onToggleCaseSensitive}
        {disabled}
        title={caseSensitive ? 'Case-sensitive' : 'Case-insensitive'}
        aria-label={caseSensitive ? 'Case-sensitive' : 'Case-insensitive'}
    >
        Aa
    </button>
    <button
        class="pattern-type-toggle"
        class:ai-highlight={highlightedFields.has('patternType')}
        onclick={onTogglePatternType}
        {disabled}
        title="Toggle between glob and regex matching"
        aria-label="Pattern type: {patternType === 'regex' ? 'Regex' : 'Glob'}"
    >
        {patternType === 'regex' ? 'Regex' : 'Glob'}
    </button>
    <button class="action-button" onclick={onSearch} {disabled} title="Search (Enter)"> Search </button>
</div>

<!-- Scope row -->
<div class="input-row">
    <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
        <path
            d="M2 4.5V12.5C2 13.05 2.45 13.5 3 13.5H13C13.55 13.5 14 13.05 14 12.5V6.5C14 5.95 13.55 5.5 13 5.5H8L6.5 3.5H3C2.45 3.5 2 3.95 2 4.5Z"
            stroke="currentColor"
            stroke-width="1.3"
            fill="none"
        />
    </svg>
    <input
        type="text"
        class="name-input"
        class:ai-highlight={highlightedFields.has('scope')}
        placeholder="All folders"
        value={scope}
        oninput={onInput(setScope)}
        {disabled}
        aria-label="Search scope"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
    />
    <div class="scope-info-wrapper">
        <button
            class="scope-info-button"
            use:tooltip={{
                html:
                    '<div style="max-width:380px">' +
                    '<div style="font-weight:600;margin-bottom:4px">Search scope — which folders to search in</div>' +
                    '<div style="color:var(--color-text-secondary);margin-bottom:8px">Comma-separated paths. Use ! to exclude.</div>' +
                    '<table style="border-spacing:0;margin-bottom:8px;width:100%">' +
                    '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects</code></td><td style="color:var(--color-text-secondary)">Search in one folder</td></tr>' +
                    '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects, ~/Documents</code></td><td style="color:var(--color-text-secondary)">Search in multiple folders</td></tr>' +
                    '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>!node_modules, !.git</code></td><td style="color:var(--color-text-secondary)">Exclude folders by name</td></tr>' +
                    '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>~/projects, !node_modules</code></td><td style="color:var(--color-text-secondary)">Combine include and exclude</td></tr>' +
                    '<tr><td style="padding:2px 12px 2px 0;white-space:nowrap"><code>!.*</code></td><td style="color:var(--color-text-secondary)">Exclude hidden folders</td></tr>' +
                    '</table>' +
                    '<div style="color:var(--color-text-secondary)">Wildcards * and ? work in folder names.<br>Use quotes or backslash to escape commas.</div>' +
                    '</div>',
            }}
            {disabled}
            aria-label="Scope syntax help"
        >
            i
        </button>
    </div>
    <button
        class="pattern-type-toggle"
        class:active={excludeSystemDirs}
        onclick={onToggleExcludeSystemDirs}
        {disabled}
        use:tooltip={{ html: systemDirExcludeTooltip }}
        aria-label={excludeSystemDirs ? 'System folders excluded' : 'System folders included'}
    >
        Filter
    </button>
    <button
        class="pattern-type-toggle"
        onclick={() => {
            onSetScope(currentFolderPath)
            scheduleSearch()
        }}
        {disabled}
        title="Scope to current folder (⌥F)"
        aria-label="Scope to current folder"
    >
        ⌥F
    </button>
    <button
        class="pattern-type-toggle"
        onclick={() => {
            onSetScope('')
            scheduleSearch()
        }}
        {disabled}
        title="Search entire drive (⌥D)"
        aria-label="Search entire drive"
    >
        ⌥D
    </button>
</div>

<!-- Filter row -->
<div class="filter-row">
    <div class="filter-group" class:ai-highlight={highlightedFields.has('size')}>
        <label class="filter-label" for="size-filter">Size</label>
        <select
            id="size-filter"
            class="filter-select"
            value={sizeFilter}
            onchange={onSelect<SizeFilter>(setSizeFilter)}
            {disabled}
            aria-label="Size filter"
        >
            <option value="any">any</option>
            <option value="gte">&ge;</option>
            <option value="lte">&le;</option>
            <option value="between">between</option>
        </select>
        {#if sizeFilter !== 'any'}
            <input
                type="number"
                class="filter-input size-input"
                value={sizeValue}
                oninput={onInput(setSizeValue)}
                {disabled}
                aria-label="Minimum size value"
                min="0"
                step="any"
            />
            <select
                class="filter-select unit-select"
                value={sizeUnit}
                onchange={onSelect<SizeUnit>(setSizeUnit)}
                {disabled}
                aria-label="Size unit"
            >
                <option value="KB">KB</option>
                <option value="MB">MB</option>
                <option value="GB">GB</option>
            </select>
        {/if}
        {#if sizeFilter === 'between'}
            <span class="filter-separator">–</span>
            <input
                type="number"
                class="filter-input size-input"
                value={sizeValueMax}
                oninput={onInput(setSizeValueMax)}
                {disabled}
                aria-label="Maximum size value"
                min="0"
                step="any"
            />
            <select
                class="filter-select unit-select"
                value={sizeUnitMax}
                onchange={onSelect<SizeUnit>(setSizeUnitMax)}
                {disabled}
                aria-label="Maximum size unit"
            >
                <option value="KB">KB</option>
                <option value="MB">MB</option>
                <option value="GB">GB</option>
            </select>
        {/if}
    </div>

    <div class="filter-group" class:ai-highlight={highlightedFields.has('date')}>
        <label class="filter-label" for="date-filter">Modified</label>
        <select
            id="date-filter"
            class="filter-select"
            value={dateFilter}
            onchange={onSelect<DateFilter>(setDateFilter)}
            {disabled}
            aria-label="Date filter"
        >
            <option value="any">any</option>
            <option value="after">after</option>
            <option value="before">before</option>
            <option value="between">between</option>
        </select>
        {#if dateFilter !== 'any'}
            <input
                type="date"
                class="filter-input date-input"
                value={dateValue}
                oninput={onInput(setDateValue)}
                {disabled}
                aria-label="Date value"
            />
        {/if}
        {#if dateFilter === 'between'}
            <span class="filter-separator">–</span>
            <input
                type="date"
                class="filter-input date-input"
                value={dateValueMax}
                oninput={onInput(setDateValueMax)}
                {disabled}
                aria-label="Maximum date value"
            />
        {/if}
    </div>
</div>

<style>
    .input-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-sm) var(--spacing-md);
        border-bottom: 1px solid var(--color-border-strong);
        background: var(--color-bg-primary);
        gap: var(--spacing-sm);
    }

    .search-icon {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .name-input {
        flex: 1;
        font-size: var(--font-size-md);
        border: 1px solid transparent;
        background: transparent;
        color: var(--color-text-primary);
        outline: none;
        min-width: 0;
    }

    .name-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .name-input::placeholder {
        color: var(--color-text-tertiary);
        opacity: 1; /* Override browser default dimming for a11y contrast */
    }

    .name-input.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    /* Shared button style for Search */
    .action-button {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .action-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .action-button:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .pattern-type-toggle {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        font-size: var(--font-size-xs);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        font-family: var(--font-mono);
        min-width: 40px;
        text-align: center;
    }

    .pattern-type-toggle:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .pattern-type-toggle:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    .pattern-type-toggle.active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .pattern-type-toggle.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    /* Scope info button and tooltip */
    .scope-info-wrapper {
        position: relative;
        flex-shrink: 0;
    }

    .scope-info-button {
        width: 18px;
        height: 18px;
        border-radius: var(--radius-full);
        border: 1px solid var(--color-border);
        font-size: var(--font-size-xs);
        font-style: italic;
        font-family: var(--font-system);
        color: var(--color-text-tertiary);
        display: flex;
        align-items: center;
        justify-content: center;
        line-height: 1;
    }

    .scope-info-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .scope-info-button:not(:disabled):hover {
        border-color: var(--color-border-strong);
        color: var(--color-text-secondary);
    }

    /* Filter row */
    .filter-row {
        display: flex;
        align-items: center;
        padding: var(--spacing-xs) var(--spacing-md);
        gap: var(--spacing-lg);
        border-bottom: 1px solid var(--color-border-strong);
        flex-wrap: wrap;
    }

    .filter-group {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .filter-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .filter-select {
        font-size: var(--font-size-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .filter-select:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .filter-input {
        font-size: var(--font-size-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 4px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .filter-input:focus {
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

    .filter-separator {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .filter-group.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        padding: var(--spacing-xxs) var(--spacing-xs);
        transition: background 1.5s ease-out;
    }
</style>
