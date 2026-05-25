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
     * Keyboard contract: ⌥F and ⌥D set / clear the scope and are wired at the dialog level
     * (`SearchDialog.svelte` → `handleModifierShortcuts`). They work regardless of whether the
     * scope popover is open. The popover's footer also exposes them as buttons so mouse users
     * have first-class access.
     */
    import { SvelteSet } from 'svelte/reactivity'
    import { tooltip } from '$lib/tooltip/tooltip'
    import FilterChip from './FilterChip.svelte'
    import FilterChipPopover from './FilterChipPopover.svelte'
    import { deriveSizeChip, deriveDateChip, deriveScopeChip, derivePatternChip } from './filter-chip-state'
    import type { QueryFilterState, SizeFilter, SizeUnit, DateFilter } from './query-filter-state.svelte'
    import {
        SIZE_PRESETS,
        byteUnitLabel,
        kiloByteLabel,
        isSizeRangeDisabled,
        showsUpperBound,
        isDateRangeDisabled,
        showsDateUpperBound,
        buildDatePresets,
        type DynamicDatePreset,
    } from './filter-popover-helpers'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'

    type FilterKey = 'size' | 'date' | 'scope'

    interface Props {
        /**
         * The query-filter state instance owning size/date/case/setQuery setters. Passed by
         * the consumer wrapper (Search wires the search instance; Selection wires its own
         * Selection instance in M7). Replaces the M2-era module-singleton setter imports.
         * Named `filterState` (not `state`) to avoid shadowing Svelte's `$state` rune.
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
         * `true` (scope is core to whole-drive search); Selection (M7+) passes `false`
         * because selection runs against a single in-memory folder. Default `true` keeps
         * Search's existing behavior.
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
         * its AI-extras `lastAiPattern`. Selection's M7 wrapper will clear its own AI-pattern
         * slot (Selection has no Pattern chip per the M3 plan, but the callback is wired so
         * the same component can be reused if Selection later opts in).
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

    // Pull the setters from the injected state instance so the template reads
    // `setSizeFilter(...)` like before. Arrow-function wrappers (not raw method refs) keep
    // `this` binding intact and silence `@typescript-eslint/unbound-method` since the
    // factory's setters don't read `this` but the type system can't prove it.
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
     * Round 2 D10 grid: the comparator column (col 1) of the Size popover. The
     * `>=` and `<=` use HTML entities so the rendered cell reads as the math
     * glyph rather than the ASCII soup.
     */
    const SIZE_COMPARATORS: ReadonlyArray<{ value: SizeFilter; label: string }> = [
        { value: 'any', label: 'any' },
        { value: 'gte', label: '≥' },
        { value: 'lte', label: '≤' },
        { value: 'between', label: 'between' },
    ]

    /**
     * Round 2 D11 grid: the comparator column of the Modified popover.
     */
    const DATE_COMPARATORS: ReadonlyArray<{ value: DateFilter; label: string }> = [
        { value: 'any', label: 'any' },
        { value: 'after', label: 'after' },
        { value: 'before', label: 'before' },
        { value: 'between', label: 'between' },
    ]

    /**
     * Whether the user has flipped the value column into "Custom…" mode for the
     * lower / upper bound. The free-form `<input>` only appears while these are
     * `true`; the inline `setSizeValue('')` zaps the existing preset selection
     * so the radio-set goes back to "no preset highlighted".
     */
    let sizeIsCustomLower = $state(false)
    let sizeIsCustomUpper = $state(false)
    let dateIsCustomLower = $state(false)
    let dateIsCustomUpper = $state(false)

    /**
     * R3 B5: rebuild the Modified preset list each time the popover renders.
     * Labels are date-relative ("1st of May 0:00"), so a stale cached list
     * from yesterday would mislead the user. The dynamic list is cheap to
     * compute (one Date plus a few format calls); the only reason to memoize
     * would be benchmarks, which haven't surfaced as a concern.
     */
    const datePresets = $derived<DynamicDatePreset[]>(buildDatePresets())
    /** Set of ISO date strings that match a preset. Used for the custom-isolation rule. */
    const datePresetSet = $derived(new Set<string>(datePresets.map((p) => p.resolved)))

    /** Re-sync the "custom" flag against an externally-set value (AI mode, MCP prefill). */
    $effect(() => {
        // If the dialog or AI lands a value that exactly matches a preset, drop out of custom mode
        // so the preset row lights up; otherwise stay in custom (the input drives the value).
        if (sizeValue && !SIZE_PRESETS.includes(sizeValue)) {
            sizeIsCustomLower = true
        } else if (sizeValue) {
            sizeIsCustomLower = false
        }
    })
    $effect(() => {
        if (sizeValueMax && !SIZE_PRESETS.includes(sizeValueMax)) {
            sizeIsCustomUpper = true
        } else if (sizeValueMax) {
            sizeIsCustomUpper = false
        }
    })

    /**
     * R3 B5: mirror the size effect for the Modified popover. The bug was
     * that selecting any preset wrote the resolved ISO date into `dateValue`,
     * which the popover then displayed as both the preset's `is-selected`
     * cell AND the Custom cell (because `dateIsCustomLower` was set true at
     * some earlier point and never reset). Selection model now:
     *   - dateValue matches a preset → dateIsCustomLower = false.
     *   - dateValue is non-empty and does NOT match any preset → user picked
     *     Custom (or the AI / history set a custom value) → keep
     *     dateIsCustomLower = true.
     *   - dateValue empty → keep whatever the flag was (don't reset on clear).
     */
    $effect(() => {
        if (dateValue && !datePresetSet.has(dateValue)) {
            dateIsCustomLower = true
        } else if (dateValue) {
            dateIsCustomLower = false
        }
    })
    $effect(() => {
        if (dateValueMax && !datePresetSet.has(dateValueMax)) {
            dateIsCustomUpper = true
        } else if (dateValueMax) {
            dateIsCustomUpper = false
        }
    })

    /**
     * When the user picks a new comparator we clear the value if it's currently empty AND the
     * comparator demands one. Without this nudge the popover would stay disabled-looking with
     * `value === ''` until the user clicks a preset. Schedules a search at the end so the chip
     * label updates immediately.
     */
    function onSizeComparatorEdit(): void {
        scheduleSearch()
    }

    function onDateComparatorEdit(): void {
        scheduleSearch()
    }

    /**
     * R3 U5: helpers for the "click on disabled cell auto-promotes the
     * comparator" behaviour. When the user clicks a value in the value
     * column (or a unit in the unit column) while the comparator is `any`,
     * promote the comparator (Size → `gte`, Modified → `after`) AND apply
     * the clicked value. The user can still tweak the comparator before
     * hitting Enter; we don't fire the search until they explicitly do.
     *
     * These helpers are wrappers around the existing setters so the cell
     * click handlers can stay compact in the template.
     */
    function pickSizeValue(value: string): void {
        if (sizeFilter === 'any') setSizeFilter('gte')
        setSizeValue(value)
        scheduleSearch()
    }
    function pickSizeUnit(unit: SizeUnit): void {
        if (sizeFilter === 'any') setSizeFilter('gte')
        setSizeUnit(unit)
        scheduleSearch()
    }
    function pickSizeCustomLower(): void {
        if (sizeFilter === 'any') setSizeFilter('gte')
        sizeIsCustomLower = true
        setSizeValue('')
        scheduleSearch()
    }
    function pickDateValue(resolved: string): void {
        if (dateFilter === 'any') setDateFilter('after')
        setDateValue(resolved)
        scheduleSearch()
    }
    function pickDateValueMax(resolved: string): void {
        if (dateFilter === 'any') setDateFilter('after')
        setDateValueMax(resolved)
        scheduleSearch()
    }
    function pickDateCustomLower(): void {
        if (dateFilter === 'any') setDateFilter('after')
        dateIsCustomLower = true
        setDateValue('')
        scheduleSearch()
    }

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

    // R3 B3: pipe the user's file-size format through so the chip's KB/kB label
    // matches the popover. Today the popover renders `kB` for SI but the chip
    // bypassed the setting and always printed `KB`.
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

<!-- Size popover. Round 2 D10: replaces the old `<select>` triplet with a
     multi-column list-style grid. Col 1 = comparator (`any` / `>=` / `<=` /
     `between`). Col 2 = numeric preset (`0` / `1` / `5` / ... / `Custom...`).
     Col 3 = unit (`bytes` / `KB` / `MB` / `GB`). When col 1 = `between`, cols
     4 + 5 mirror cols 2 + 3 for the upper bound. Cols 2-5 render disabled when
     col 1 = `any` (no range to apply). -->
{#if sizeChipEl}
    <FilterChipPopover
        anchor={sizeChipEl}
        open={openChip === 'size'}
        onClose={closePopover}
        ariaLabel="Size filter options"
    >
        <div class="popover-section size-grid-section">
            <span class="popover-label">Size</span>
            <div
                class="list-grid"
                class:has-upper={showsUpperBound(sizeFilter)}
                role="group"
                aria-label="Size filter options"
            >
                <!-- Col 1: comparator -->
                <div class="list-col" role="radiogroup" aria-label="Comparator">
                    {#each SIZE_COMPARATORS as opt (opt.value)}
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeFilter === opt.value}
                            role="radio"
                            aria-checked={sizeFilter === opt.value}
                            onclick={() => {
                                setSizeFilter(opt.value)
                                onSizeComparatorEdit()
                            }}
                        >
                            {opt.label}
                        </button>
                    {/each}
                </div>

                <!-- Col 2: lower-bound value. R3 U5: cells stay clickable when
                     the comparator is `any`. Clicking promotes the comparator
                     to `gte` and applies the chosen value. R3 U3: the Custom
                     <input> renders INSIDE the Custom cell so one click both
                     selects it AND focuses the input. -->
                <div class="list-col" role="radiogroup" aria-label="Minimum size value">
                    {#each SIZE_PRESETS as preset (preset)}
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeValue === preset && !sizeIsCustomLower}
                            class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                            role="radio"
                            aria-checked={sizeValue === preset && !sizeIsCustomLower}
                            onclick={() => {
                                pickSizeValue(preset)
                            }}
                        >
                            {preset}
                        </button>
                    {/each}
                    <!-- R3 U3 + B5: Custom cell holds the inline number input.
                         Selected only when the user explicitly clicked Custom
                         (or the AI / history set a custom value via the
                         `dateIsCustomLower`-style effect above). -->
                    <button
                        type="button"
                        class="list-cell list-cell-custom"
                        class:is-selected={sizeIsCustomLower}
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        onclick={(e) => {
                            // Don't re-pick if the click is on the inner input
                            // (it bubbles up through the button).
                            if ((e.target as HTMLElement).tagName === 'INPUT') return
                            pickSizeCustomLower()
                            // Focus the inner input on the next tick so the
                            // user can type immediately.
                            void Promise.resolve().then(() => {
                                ;(
                                    e.currentTarget as HTMLElement | null
                                )?.querySelector<HTMLInputElement>('input')?.focus()
                            })
                        }}
                    >
                        {#if sizeIsCustomLower}
                            <input
                                type="number"
                                class="popover-input size-input custom-input-inline"
                                value={sizeValue}
                                oninput={onInput(setSizeValue)}
                                onclick={(e) => {
                                    e.stopPropagation()
                                }}
                                aria-label="Custom minimum size value"
                                min="0"
                                step="any"
                                placeholder="custom"
                            />
                        {:else}
                            custom…
                        {/if}
                    </button>
                </div>

                <!-- Col 3: lower-bound unit -->
                <div class="list-col" role="radiogroup" aria-label="Minimum size unit">
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeUnit === 'B'}
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        role="radio"
                        aria-checked={sizeUnit === 'B'}
                        onclick={() => {
                            pickSizeUnit('B')
                        }}
                    >
                        {byteUnitLabel(sizeValue)}
                    </button>
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={sizeUnit === 'KB'}
                        class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                        role="radio"
                        aria-checked={sizeUnit === 'KB'}
                        onclick={() => {
                            pickSizeUnit('KB')
                        }}
                    >
                        {kiloByteLabel(getFileSizeFormat())}
                    </button>
                    {#each ['MB', 'GB'] as larger (larger)}
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeUnit === larger}
                            class:is-disabled-look={isSizeRangeDisabled(sizeFilter)}
                            role="radio"
                            aria-checked={sizeUnit === larger}
                            onclick={() => {
                                pickSizeUnit(larger as SizeUnit)
                            }}
                        >
                            {larger}
                        </button>
                    {/each}
                </div>

                {#if showsUpperBound(sizeFilter)}
                    <!-- Col 4: upper-bound value. R3 U3: Custom input inline. -->
                    <div class="list-col" role="radiogroup" aria-label="Maximum size value">
                        {#each SIZE_PRESETS as preset (preset)}
                            <button
                                type="button"
                                class="list-cell"
                                class:is-selected={sizeValueMax === preset && !sizeIsCustomUpper}
                                role="radio"
                                aria-checked={sizeValueMax === preset && !sizeIsCustomUpper}
                                onclick={() => {
                                    setSizeValueMax(preset)
                                    scheduleSearch()
                                }}
                            >
                                {preset}
                            </button>
                        {/each}
                        <button
                            type="button"
                            class="list-cell list-cell-custom"
                            class:is-selected={sizeIsCustomUpper}
                            onclick={(e) => {
                                if ((e.target as HTMLElement).tagName === 'INPUT') return
                                sizeIsCustomUpper = true
                                setSizeValueMax('')
                                scheduleSearch()
                                void Promise.resolve().then(() => {
                                    ;(
                                        e.currentTarget as HTMLElement | null
                                    )?.querySelector<HTMLInputElement>('input')?.focus()
                                })
                            }}
                        >
                            {#if sizeIsCustomUpper}
                                <input
                                    type="number"
                                    class="popover-input size-input custom-input-inline"
                                    value={sizeValueMax}
                                    oninput={onInput(setSizeValueMax)}
                                    onclick={(e) => {
                                        e.stopPropagation()
                                    }}
                                    aria-label="Custom maximum size value"
                                    min="0"
                                    step="any"
                                    placeholder="custom"
                                />
                            {:else}
                                custom…
                            {/if}
                        </button>
                    </div>

                    <!-- Col 5: upper-bound unit -->
                    <div class="list-col" role="radiogroup" aria-label="Maximum size unit">
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeUnitMax === 'B'}
                            role="radio"
                            aria-checked={sizeUnitMax === 'B'}
                            onclick={() => {
                                setSizeUnitMax('B')
                                scheduleSearch()
                            }}
                        >
                            {byteUnitLabel(sizeValueMax)}
                        </button>
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={sizeUnitMax === 'KB'}
                            role="radio"
                            aria-checked={sizeUnitMax === 'KB'}
                            onclick={() => {
                                setSizeUnitMax('KB')
                                scheduleSearch()
                            }}
                        >
                            {kiloByteLabel(getFileSizeFormat())}
                        </button>
                        {#each ['MB', 'GB'] as larger (larger)}
                            <button
                                type="button"
                                class="list-cell"
                                class:is-selected={sizeUnitMax === larger}
                                role="radio"
                                aria-checked={sizeUnitMax === larger}
                                onclick={() => {
                                    setSizeUnitMax(larger as SizeUnit)
                                    scheduleSearch()
                                }}
                            >
                                {larger}
                            </button>
                        {/each}
                    </div>
                {/if}
            </div>
        </div>
    </FilterChipPopover>
{/if}

<!-- Modified popover. Round 2 D11: list-style grid mirroring the Size popover.
     Col 1 = comparator (`any` / `after` / `before` / `between`). Col 2 = preset
     dates (`today`, `yesterday`, `this week`, ..., `Custom…`). Cols 3 + 4
     appear when col 1 = `between` for the upper bound. No unit column. -->
{#if dateChipEl}
    <FilterChipPopover
        anchor={dateChipEl}
        open={openChip === 'date'}
        onClose={closePopover}
        ariaLabel="Modified filter options"
    >
        <div class="popover-section size-grid-section">
            <span class="popover-label">Modified</span>
            <div
                class="list-grid date-grid"
                class:has-upper={showsDateUpperBound(dateFilter)}
                role="group"
                aria-label="Modified filter options"
            >
                <!-- Col 1: comparator -->
                <div class="list-col" role="radiogroup" aria-label="Comparator">
                    {#each DATE_COMPARATORS as opt (opt.value)}
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={dateFilter === opt.value}
                            role="radio"
                            aria-checked={dateFilter === opt.value}
                            onclick={() => {
                                setDateFilter(opt.value)
                                onDateComparatorEdit()
                            }}
                        >
                            {opt.label}
                        </button>
                    {/each}
                </div>

                <!-- Col 2: lower-bound preset. R3 U4: dynamic labels
                     ("today 0:00", "1st of May 0:00", "1st of April, 2026,
                     0:00", ...) computed at popover render. R3 U5: cells
                     remain clickable while comparator = `any`; clicking
                     promotes the comparator to `after`. R3 B5: Custom is
                     selected only when the user explicitly clicked it (or
                     the AI / history loaded a custom value); the `$effect`
                     above keeps `dateIsCustomLower` in sync. -->
                <div class="list-col" role="radiogroup" aria-label="Date value">
                    {#each datePresets as preset (preset.key)}
                        <button
                            type="button"
                            class="list-cell"
                            class:is-selected={dateValue === preset.resolved && !dateIsCustomLower}
                            class:is-disabled-look={isDateRangeDisabled(dateFilter)}
                            role="radio"
                            aria-checked={dateValue === preset.resolved && !dateIsCustomLower}
                            onclick={() => {
                                pickDateValue(preset.resolved)
                            }}
                        >
                            {preset.label}
                        </button>
                    {/each}
                    <button
                        type="button"
                        class="list-cell list-cell-custom"
                        class:is-selected={dateIsCustomLower}
                        class:is-disabled-look={isDateRangeDisabled(dateFilter)}
                        onclick={(e) => {
                            if ((e.target as HTMLElement).tagName === 'INPUT') return
                            pickDateCustomLower()
                            void Promise.resolve().then(() => {
                                ;(
                                    e.currentTarget as HTMLElement | null
                                )?.querySelector<HTMLInputElement>('input')?.focus()
                            })
                        }}
                    >
                        {#if dateIsCustomLower}
                            <input
                                type="date"
                                class="popover-input date-input custom-input-inline"
                                value={dateValue}
                                oninput={onInput(setDateValue)}
                                onclick={(e) => {
                                    e.stopPropagation()
                                }}
                                aria-label="Custom date value"
                            />
                        {:else}
                            custom…
                        {/if}
                    </button>
                </div>

                {#if showsDateUpperBound(dateFilter)}
                    <!-- Col 3: upper-bound preset (same shape as col 2). -->
                    <div class="list-col" role="radiogroup" aria-label="Maximum date value">
                        {#each datePresets as preset (preset.key)}
                            <button
                                type="button"
                                class="list-cell"
                                class:is-selected={dateValueMax === preset.resolved && !dateIsCustomUpper}
                                role="radio"
                                aria-checked={dateValueMax === preset.resolved && !dateIsCustomUpper}
                                onclick={() => {
                                    pickDateValueMax(preset.resolved)
                                }}
                            >
                                {preset.label}
                            </button>
                        {/each}
                        <button
                            type="button"
                            class="list-cell list-cell-custom"
                            class:is-selected={dateIsCustomUpper}
                            onclick={(e) => {
                                if ((e.target as HTMLElement).tagName === 'INPUT') return
                                dateIsCustomUpper = true
                                setDateValueMax('')
                                scheduleSearch()
                                void Promise.resolve().then(() => {
                                    ;(
                                        e.currentTarget as HTMLElement | null
                                    )?.querySelector<HTMLInputElement>('input')?.focus()
                                })
                            }}
                        >
                            {#if dateIsCustomUpper}
                                <input
                                    type="date"
                                    class="popover-input date-input custom-input-inline"
                                    value={dateValueMax}
                                    oninput={onInput(setDateValueMax)}
                                    onclick={(e) => {
                                        e.stopPropagation()
                                    }}
                                    aria-label="Custom maximum date value"
                                />
                            {:else}
                                custom…
                            {/if}
                        </button>
                    </div>
                {/if}
            </div>
        </div>
    </FilterChipPopover>
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
                oninput={onInput(onSetScope)}
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
                        aria-label="Hide boring folders"
                    />
                    <!-- R3 U6: copy renamed "Hide system folders" -> "Hide boring folders".
                         Tooltip lists EVERY exclude (built by the parent from the
                         `get_system_dir_excludes` IPC); no "+30 more" truncation. -->
                    <span use:tooltip={{ html: systemDirExcludeTooltip }}>Hide boring folders</span>
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
            <!-- D9: scope shortcuts moved inside the popover. ⌥C "Use current
                 folder", ⌥V "All folders". Only active while the popover is open
                 (matching the round-2 resolved shortcut allocation: the global
                 ⌥F now drives the Filename mode chip instead). -->
            <div class="popover-footer">
                <!-- D12: "Use current folder" renders disabled when the focused
                     pane is a search-results snapshot AND no real-folder history
                     entry is reachable. The button still shows so the user sees
                     the option exists; the tooltip explains why it's off. -->
                <button
                    type="button"
                    class="footer-button"
                    disabled={searchableFolder.disabled}
                    use:tooltip={searchableFolder.disabled ? searchableFolder.disabledReason : ''}
                    onclick={() => {
                        if (searchableFolder.disabled || !searchableFolder.path) return
                        onSetScope(searchableFolder.path)
                        scheduleSearch()
                    }}
                >
                    Use current folder
                    <kbd class="footer-kbd">⌥C</kbd>
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
                    <kbd class="footer-kbd">⌥V</kbd>
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

    /* ===== Round 2 D10 / D11: list-style grid popover ===== */

    .size-grid-section {
        min-width: 320px;
    }

    /* Auto-sized columns so each `Custom…` text width drives the col width and
       the upper bound (rendered conditionally) inherits the same widths via
       the parent grid. Gap pulls the columns visually apart so the eye reads
       them as separate axes. */
    .list-grid {
        display: grid;
        grid-template-columns: repeat(3, auto);
        gap: var(--spacing-sm);
    }

    .list-grid.has-upper {
        grid-template-columns: repeat(5, auto);
    }

    .date-grid {
        grid-template-columns: repeat(2, auto);
    }

    .date-grid.has-upper {
        grid-template-columns: repeat(3, auto);
    }

    .list-col {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .list-cell {
        display: block;
        text-align: left;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 4px 10px;
        font-size: var(--font-size-sm);
        line-height: 1.3;
        background: transparent;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        white-space: nowrap;
        text-decoration: none;
    }

    .list-cell:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .list-cell.is-selected {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .list-cell:not(:disabled):focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -1px;
    }

    .list-cell:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    /* R3 U5: cells in the value / unit columns stay clickable while the
       comparator is `any`. The dimmed look mirrors `:disabled` but the cell
       is still a real button: clicking promotes the comparator and applies
       the value. */
    .list-cell.is-disabled-look {
        opacity: 0.5;
    }

    .list-cell.is-disabled-look:not(.is-selected):hover {
        background: var(--color-bg-tertiary);
        opacity: 0.7;
    }

    /* R3 U3: the Custom cell holds the inline input. The cell itself stays
       sized for the longest preset label so the column doesn't reflow when
       Custom expands; the input fills the remaining cell width. */
    .list-cell.list-cell-custom {
        /* Reserve enough vertical room for the inner input. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        min-height: 22px;
        display: flex;
        align-items: center;
    }

    .list-cell.list-cell-custom .custom-input-inline {
        font-size: var(--font-size-sm);
        background: transparent;
        border: 0;
        outline: none;
        color: var(--color-text-primary);
        width: 100%;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 0;
        line-height: 1.3;
    }

    /* R3 U3: dropped the round-2 `.custom-input` rule (the input lived as a
       sibling under the Custom cell). The new layout puts the input INSIDE
       the cell, styled via `.custom-input-inline` above. */

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

    /* Free-form number / date input used under each "Custom…" cell in the
       Size + Modified popovers. The list-style grid is the primary surface;
       these are the escape hatches. */
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

    .popover-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .size-input {
        width: 80px;
    }

    .date-input {
        width: 130px;
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

    .footer-button:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .footer-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
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
