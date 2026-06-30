<script lang="ts" module>
    /**
     * One item in a `Select`. `value` is the stable identity (compared and emitted as a string);
     * `label` is the visible text; `description` renders as quieter inline text after the label
     * (used by `SettingSelect`'s option descriptions); `group` is an optional group/optgroup label
     * that, when present, buckets the item under an Ark `ItemGroup` with that heading.
     */
    export interface SelectItem {
        value: string
        label: string
        description?: string
        group?: string
    }
</script>

<script lang="ts">
    /**
     * Presentational, items-driven single-select built on Ark UI's `Select`. The house dropdown:
     * `SettingSelect` and the viewer / transfer / debug native-`<select>` replacements all render
     * through it so the macOS pop-up-button look stays in one place.
     *
     * macOS pop-up-button styling: a borderless trigger (value text + a round chevron stepper that
     * fills the whole trigger on hover/open), and a frosted-glass menu that opens *over* the trigger
     * with the currently-selected row landing on the trigger (see "macOS overlap positioning" below).
     * The checkmark marks the current value on the left; the accent highlight follows the keyboard /
     * pointer cursor.
     *
     * Stable class contract (don't rename without updating consumers + the a11y-contrast checker's
     * `scripts/check-a11y-contrast/dropdown_states.go`, which keys on literal selector strings):
     * `.select-trigger`, `.select-item`, `.select-content`, `.option-description`. `SettingSelect`'s
     * `handleCustomSubmit` focuses `.select-trigger` via `querySelector`. The `ariaLabel` lands on
     * the trigger.
     *
     * The open menu can teleport to `document.body` via the `portal` prop (escapes ancestor
     * `overflow`/`mask`); the overlap measurement finds the content through its `bind:ref`, so it
     * works portaled or not. See `lib/ui/DETAILS.md` § Select.
     */
    import { Select, createListCollection, type SelectValueChangeDetails } from '@ark-ui/svelte/select'
    import { Portal } from '@ark-ui/svelte/portal'
    import Icon from '$lib/ui/Icon.svelte'
    import { computeOverlapShift } from '$lib/ui/select-positioning'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        items: SelectItem[]
        /** Selected item's `value`. Empty string means nothing selected (renders `placeholder`). */
        value: string
        onChange: (value: string) => void
        /** Fires on keyboard/pointer highlight. `SettingSelect` applies immediately on highlight. */
        onHighlightChange?: (highlightedValue: string | null) => void
        disabled?: boolean
        placeholder?: string
        ariaLabel: string
        /** Extra class on the `.select-content` element (for example `custom-highlighted`). */
        contentClass?: string
        /**
         * Teleport the open menu to `document.body` so it escapes any ancestor `overflow`/`mask`/
         * stacking context (for example the settings page's masked, scrolling content wrapper). Leave
         * `false` in the viewer window, whose restricted capability set assumes no portal-to-body.
         */
        portal?: boolean
    }

    const {
        items,
        value,
        onChange,
        onHighlightChange,
        disabled = false,
        placeholder,
        ariaLabel,
        contentClass = '',
        portal = false,
    }: Props = $props()

    const resolvedPlaceholder = $derived(placeholder ?? tString('ui.select.placeholder'))

    const collection = $derived(
        createListCollection({
            items,
            itemToString: (item: SelectItem) => item.label,
            itemToValue: (item: SelectItem) => item.value,
        }),
    )

    // Items render either flat or bucketed under their `group` label. Preserve first-seen group
    // order; items without a `group` bucket under '' (rendered as an unlabelled group).
    const groupedItems = $derived.by((): { label: string; items: SelectItem[] }[] | null => {
        const hasGroups = items.some((item) => item.group)
        if (!hasGroups) return null
        const groups: { label: string; items: SelectItem[] }[] = []
        for (const item of items) {
            const key = item.group ?? ''
            const existing = groups.find((g) => g.label === key)
            if (existing) existing.items.push(item)
            else groups.push({ label: key, items: [item] })
        }
        return groups
    })

    function handleValueChange(details: SelectValueChangeDetails<SelectItem>): void {
        if (details.value.length > 0) onChange(details.value[0])
    }

    function handleHighlightChange(details: { highlightedValue: string | null }): void {
        onHighlightChange?.(details.highlightedValue)
    }

    // --- macOS overlap positioning ---------------------------------------------------------------
    // macOS opens a pop-up menu *over* the trigger, with the current value's row landing on the
    // button (its label aligned to the trigger's value text). Zag positions the *positioner* just
    // below the trigger (`bottom-start`, gutter 0); we then translate the *content* (a child of the
    // positioner, so this never fights zag's own transform) to slide the checked row onto the
    // trigger, clamped to the viewport so it stays on screen. Because the shift is a CSS transform on
    // the content it doesn't trigger a zag reposition, so there's no feedback loop.
    //
    // The reveal is driven by the open state, NOT zag's `onPositioned` (which this zag version never
    // fires). Content stays invisible (`positioned`) until the first measurement lands so it never
    // flashes at the default below-trigger spot; a `setTimeout` fallback guarantees it can never get
    // stuck invisible if rAF is throttled (unfocused window) or the rows aren't found.
    let rootEl: HTMLElement | null = $state(null)
    // The content's own ref, so the measurement finds it even when `portal` teleports it to body
    // (outside `rootEl`'s subtree). The trigger value stays inside `rootEl`.
    let contentEl: HTMLElement | null = $state(null)
    let isOpen = $state(false)
    let shiftX = $state(0)
    let shiftY = $state(0)
    let positioned = $state(false)

    const VIEWPORT_PAD = 8

    const positioning = {
        placement: 'bottom-start' as const,
        gutter: 0,
        flip: false,
        slide: true,
    }

    // Measure once the menu is open and zag has placed the positioner, then translate the content so
    // the checked row lands on the trigger. Returns false (so the caller retries on the next frame)
    // until the trigger value and checked row are both in the DOM.
    function alignToSelected(): boolean {
        if (!rootEl) return false
        const triggerValue = rootEl.querySelector('.select-value')
        const content = contentEl
        if (!triggerValue || !content) return false
        const checkedText =
            content.querySelector('.select-item[data-state="checked"] .select-item-text') ??
            content.querySelector('.select-item .select-item-text')
        if (!checkedText) return false

        const shift = computeOverlapShift({
            trigger: triggerValue.getBoundingClientRect(),
            item: checkedText.getBoundingClientRect(),
            content: content.getBoundingClientRect(),
            shiftX,
            shiftY,
            viewportWidth: window.innerWidth,
            viewportHeight: window.innerHeight,
            pad: VIEWPORT_PAD,
        })
        shiftX = shift.x
        shiftY = shift.y
        positioned = true
        return true
    }

    $effect(() => {
        if (!isOpen) return
        let cancelled = false
        let raf = 0
        // Retry across a few frames: the content mounts and zag places it asynchronously after open.
        // The measurement self-corrects, so an extra refine frame settles any residual gap.
        const attempt = (n: number): void => {
            if (cancelled) return
            const aligned = alignToSelected()
            if (n < 4) raf = requestAnimationFrame(() => { attempt(n + 1); })
            else if (!aligned) positioned = true
        }
        raf = requestAnimationFrame(() => { attempt(0); })
        const fallback = setTimeout(() => {
            if (!cancelled) positioned = true
        }, 60)
        return () => {
            cancelled = true
            cancelAnimationFrame(raf)
            clearTimeout(fallback)
        }
    })

    function handleOpenChange(details: { open: boolean }): void {
        isOpen = details.open
        if (details.open) {
            shiftX = 0
            shiftY = 0
            positioned = false
        }
    }

    // Applied as an inline style (not a CSS rule) because the overlap shift is per-instance dynamic.
    // The content is hidden until the first measurement lands, so it never flashes below the trigger.
    const contentStyle = $derived(
        `transform: translate(${String(shiftX)}px, ${String(shiftY)}px); opacity: ${positioned ? '1' : '0'}`,
    )
</script>

<div class="select-root" bind:this={rootEl}>
    <Select.Root
        {collection}
        value={value ? [value] : []}
        onValueChange={handleValueChange}
        onHighlightChange={handleHighlightChange}
        onOpenChange={handleOpenChange}
        {positioning}
        {disabled}
    >
        <Select.Control>
            <Select.Trigger class="select-trigger" aria-label={ariaLabel}>
                <Select.ValueText class="select-value" placeholder={resolvedPlaceholder} />
                <span class="select-indicator"><Icon name="chevrons-up-down" size={14} aria-hidden="true" /></span>
            </Select.Trigger>
        </Select.Control>
        <!-- Always wrap the menu in `Portal`, disabled (rendered inline) unless `portal` is set; when
             enabled it teleports to body so the open menu escapes ancestor `overflow`/`mask`. Ark's
             Portal forwards the Select context, and the content's `bind:ref` works either way. -->
        <Portal disabled={!portal}>
            <Select.Positioner>
                <Select.Content
                    bind:ref={contentEl}
                    class={`select-content${contentClass ? ` ${contentClass}` : ''}`}
                    style={contentStyle}
                    onkeydown={(e: KeyboardEvent) => {
                        // Keep Escape scoped to the dropdown so a host dialog's capture-phase Escape
                        // doesn't also fire and close the whole dialog.
                        if (e.key === 'Escape') e.stopPropagation()
                    }}
                >
                    {#if groupedItems}
                        {#each groupedItems as group (group.label)}
                            <Select.ItemGroup>
                                {#if group.label}
                                    <Select.ItemGroupLabel class="select-group-label">
                                        {group.label}
                                    </Select.ItemGroupLabel>
                                {/if}
                                {#each group.items as item (item.value)}
                                    <Select.Item {item} class="select-item">
                                        <Select.ItemIndicator class="item-indicator"
                                            ><Icon name="check" size={13} aria-hidden="true" /></Select.ItemIndicator
                                        >
                                        <Select.ItemText class="select-item-text">
                                            {item.label}
                                            {#if item.description}
                                                <span class="option-description"> — {item.description}</span>
                                            {/if}
                                        </Select.ItemText>
                                    </Select.Item>
                                {/each}
                            </Select.ItemGroup>
                        {/each}
                    {:else}
                        {#each items as item (item.value)}
                            <Select.Item {item} class="select-item">
                                <Select.ItemIndicator class="item-indicator"
                                    ><Icon name="check" size={13} aria-hidden="true" /></Select.ItemIndicator
                                >
                                <Select.ItemText class="select-item-text">
                                    {item.label}
                                    {#if item.description}
                                        <span class="option-description"> — {item.description}</span>
                                    {/if}
                                </Select.ItemText>
                            </Select.Item>
                        {/each}
                    {/if}
                </Select.Content>
            </Select.Positioner>
        </Portal>
        <Select.HiddenSelect />
    </Select.Root>
</div>

<style>
    .select-root {
        display: contents;
    }

    /* macOS borderless pop-up button: value text + a round chevron stepper, hugging its content. */
    :global(.select-trigger) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 3px 3px var(--spacing-sm);
        border: none;
        border-radius: var(--radius-md);
        background: transparent;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
    }

    /* On hover (and while open) the whole trigger fills with the chevron stepper's color, so the
       control reads as one uniform pill — the macOS pop-up-button behavior. */
    :global(.select-trigger:hover:not([data-disabled])),
    :global(.select-trigger[data-state='open']) {
        background: var(--color-bg-tertiary);
    }

    :global(.select-value) {
        white-space: nowrap;
    }

    :global(.select-trigger[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.select-trigger:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    /* The macOS-style chevron stepper: a circle holding the up/down chevrons. Its fill matches the
       trigger's hover/open fill, so it blends into the pill in those states (one uniform color) and
       reads as a distinct circle only at rest. */
    :global(.select-indicator) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 18px;
        height: 18px;
        border-radius: var(--radius-full);
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* Frosted-glass menu. Shared tokens with tooltips / filter-chip popovers so every glass surface
       reads as one material; the blur is dropped under reduced transparency (token flips opaque). */
    :global(.select-content) {
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-lg);
        box-shadow: var(--shadow-lg);
        padding: var(--spacing-xs);
        z-index: var(--z-dropdown);
        max-height: 300px;
        overflow-y: auto;
        /* Consistent width regardless of content. */
        min-width: 180px;
        width: max-content;
        outline: none;
        /* The macOS overlap shift (transform) and the until-measured hide (opacity) are applied
           inline per instance; see `contentStyle`. */
    }

    :global(html.reduce-transparency .select-content) {
        -webkit-backdrop-filter: none;
        backdrop-filter: none;
    }

    :global(.select-content:focus),
    :global(.select-content:focus-visible) {
        outline: none;
    }

    :global(.select-group-label) {
        display: block;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
        color: var(--color-text-tertiary);
    }

    :global(.select-item) {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px var(--spacing-sm);
        border-radius: var(--radius-sm);
        cursor: default;
        font-size: var(--font-size-sm);
        outline: none;
    }

    /* The label cell: takes the remaining width after the left checkmark. */
    :global(.select-item-text) {
        flex: 1;
        min-width: 0;
    }

    /* No hover visual indication on its own: only the highlighted (cursor) state paints. */
    :global(.select-item:hover) {
        background: transparent;
    }

    /* Highlighted item (keyboard / pointer cursor): accent fill, like macOS. */
    :global(.select-item[data-highlighted]) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.select-item[data-highlighted]:hover) {
        background: var(--color-accent-hover);
    }

    :global(.select-item:focus),
    :global(.select-item:focus-visible) {
        outline: none;
    }

    /* Checkmark marks the current value, on the left, macOS-style. Always reserve its space so
       labels stay aligned whether or not a row is checked. */
    :global(.item-indicator) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        min-width: 1em;
        color: var(--color-accent-text);
        visibility: hidden;
    }

    :global(.select-item[data-state='checked'] .item-indicator) {
        visibility: visible;
    }

    /* On the highlighted row the checkmark sits on the accent fill, so it flips to the accent fg. */
    :global(.select-item[data-highlighted] .item-indicator) {
        color: var(--color-accent-fg);
    }

    /* When a content-level class (`custom-highlighted`) is set, hide the checkmark on the still-checked
       standard row so the highlighted "Custom…" row is the only one reading as selected. */
    :global(.custom-highlighted .select-item[data-state='checked']:not([data-highlighted]) .item-indicator) {
        visibility: hidden;
    }

    .option-description {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    /* When the parent option is highlighted the bg flips to `--color-accent`. The description's
       resting `--color-text-tertiary` (#666 / #a0a0a0) drops to ~1–2.4:1 contrast on every system
       accent. Switch it to `--color-accent-fg` (auto-picked black/white via `readableFgOn`) so it
       matches the label's color and stays readable. The secondary visual weight is lost in this
       state, but on a saturated bg there's no opacity that stays both secondary and AA-compliant for
       Apple Purple (the worst dark-fg case). The contrast checker's `dropdown_states.go` matrix
       validates this against every accent variant. */
    :global(.select-item[data-highlighted]) .option-description {
        color: var(--color-accent-fg);
    }
</style>
