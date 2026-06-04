<script lang="ts">
    /**
     * Modified-date popover body: a list-style grid mirroring the Size popover. Col 1 = comparator
     * (`any` / `after` / `before` / `between`). Col 2 = preset dates (`today`, `yesterday`,
     * `this week`, ..., `Custom…`). Col 3 appears when col 1 = `between` for the upper bound. No
     * unit column. Preset cells stay clickable while the comparator is `any`; a click auto-promotes
     * the comparator to `after`.
     *
     * Extracted from `FilterChips.svelte`. The parent owns the chip strip, the `openChip` state,
     * and the ⌥M opener; this component owns the popover surface, the dynamic preset list, and its
     * local custom-input flags.
     */
    import FilterChipPopover from './FilterChipPopover.svelte'
    import type { DateFilter } from '../query-filter-state.svelte'
    import { isDateRangeDisabled, showsDateUpperBound, buildDatePresets, type DynamicDatePreset } from './filter-popover-helpers'
    import './filter-popover.css'

    interface Props {
        /** The Modified chip element, used by the popover shell for positioning + focus return. */
        anchor: HTMLElement
        /** Whether the popover is shown (owned by the parent's `openChip` state). */
        open: boolean
        /** Fired when the popover wants to close (Esc / click outside). */
        onClose: () => void
        dateFilter: DateFilter
        dateValue: string
        dateValueMax: string
        setDateFilter: (v: DateFilter) => void
        setDateValue: (v: string) => void
        setDateValueMax: (v: string) => void
        onInput: (setter: (v: string) => void, search?: boolean) => (e: Event) => void
        scheduleSearch: () => void
    }

    const {
        anchor,
        open,
        onClose,
        dateFilter,
        dateValue,
        dateValueMax,
        setDateFilter,
        setDateValue,
        setDateValueMax,
        onInput,
        scheduleSearch,
    }: Props = $props()

    /**
     * The comparator column of the Modified popover.
     */
    const DATE_COMPARATORS: ReadonlyArray<{ value: DateFilter; label: string }> = [
        { value: 'any', label: 'any' },
        { value: 'after', label: 'after' },
        { value: 'before', label: 'before' },
        { value: 'between', label: 'between' },
    ]

    let dateIsCustomLower = $state(false)
    let dateIsCustomUpper = $state(false)

    /**
     * Rebuild the Modified preset list each time the popover renders. Labels are
     * date-relative ("1st of May 0:00"), so a stale cached list from yesterday
     * would mislead the user. The dynamic list is cheap to compute (one Date plus
     * a few format calls); the only reason to memoize would be benchmarks, which
     * haven't surfaced as a concern.
     */
    const datePresets = $derived<DynamicDatePreset[]>(buildDatePresets())
    /** Set of ISO date strings that match a preset. Used for the custom-isolation rule. */
    const datePresetSet = $derived(new Set<string>(datePresets.map((p) => p.resolved)))

    /**
     * The key of the FIRST preset whose `resolved` ISO date equals the current
     * lower / upper bound. Two presets can resolve to the same date (on a
     * Sunday with a Sunday-first locale, "today" and "this Sunday" both land on
     * today; on the 1st of a month, "today" and "1st of <month>" collide). The
     * cell renders selected only when its own key matches this one, so exactly
     * one preset cell ever lights up instead of every preset sharing that date.
     */
    const selectedDateLowerKey = $derived(datePresets.find((p) => p.resolved === dateValue)?.key)
    const selectedDateUpperKey = $derived(datePresets.find((p) => p.resolved === dateValueMax)?.key)

    /**
     * Mirrors the size effect for the Modified popover. Selection model:
     *   - dateValue matches a preset → dateIsCustomLower = false.
     *   - dateValue is non-empty and does NOT match any preset → user picked
     *     Custom (or the AI / history set a custom value) → keep
     *     dateIsCustomLower = true.
     *   - dateValue empty → keep whatever the flag was (don't reset on clear).
     * Without this rule, selecting a preset would write the resolved ISO date into
     * `dateValue`, and the popover would highlight both the preset cell AND the
     * Custom cell (if `dateIsCustomLower` was set earlier and never reset).
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

    function onDateComparatorEdit(): void {
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
</script>

<!-- Modified popover: list-style grid mirroring the Size popover. Col 1 =
     comparator (`any` / `after` / `before` / `between`). Col 2 = preset dates
     (`today`, `yesterday`, `this week`, ..., `Custom…`). Cols 3 + 4 appear when
     col 1 = `between` for the upper bound. No unit column. -->
<FilterChipPopover {anchor} {open} {onClose} ariaLabel="Modified filter options">
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

            <!-- Col 2: lower-bound preset. Labels are dynamic ("today 0:00",
                 "1st of May 0:00", "1st of April, 2026, 0:00", ...) computed
                 at popover render. Cells remain clickable while comparator =
                 `any`; clicking promotes the comparator to `after`. Custom is
                 selected only when the user explicitly clicked it (or the AI
                 / history loaded a custom value); the `$effect` above keeps
                 `dateIsCustomLower` in sync. -->
            <div class="list-col" role="radiogroup" aria-label="Date value">
                {#each datePresets as preset (preset.key)}
                    <button
                        type="button"
                        class="list-cell"
                        class:is-selected={selectedDateLowerKey === preset.key && !dateIsCustomLower}
                        class:is-disabled-look={isDateRangeDisabled(dateFilter)}
                        role="radio"
                        aria-checked={selectedDateLowerKey === preset.key && !dateIsCustomLower}
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
                            class:is-selected={selectedDateUpperKey === preset.key && !dateIsCustomUpper}
                            role="radio"
                            aria-checked={selectedDateUpperKey === preset.key && !dateIsCustomUpper}
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

<style>
    .date-input {
        width: 130px;
    }
</style>
