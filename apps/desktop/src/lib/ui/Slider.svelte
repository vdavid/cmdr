<script lang="ts">
    /**
     * Single-thumb slider built on Ark UI's `Slider`: the house slider. Ark owns the
     * `role="slider"` ARIA, pointer dragging, and keyboard stepping; we style the track, range,
     * thumb, and ticks, and add the three things every caller wanted separately:
     *
     * - `valueLabel`: a live readout, next to the track or over it. Always `aria-hidden`, since
     *   the slider already announces its own value (steer that with `ariaValueText`).
     * - `snapTargets`: magnetic snapping to notable values while dragging.
     * - `endLabels`: quiet captions under the two ends ("Faster" / "Smaller").
     *
     * We never render `Slider.HiddenInput`. Nothing here posts a form, and nesting a focusable
     * input inside the thumb trips axe's nested-interactive and unlabeled-input rules. Test
     * hooks ride `thumbProps` (a `data-test` attribute) instead.
     */
    import { Slider, type SliderValueChangeDetails } from '@ark-ui/svelte/slider'

    interface Props {
        value: number
        onChange: (value: number) => void
        min: number
        max: number
        step?: number
        disabled?: boolean
        /** Accessible name for the slider. */
        ariaLabel: string
        /** Screen-reader text for the current value, when the raw number isn't meaningful. */
        ariaValueText?: (value: number) => string
        /** Positions (in value space) to draw a tick at. A tick lights up on an exact match. */
        ticks?: number[]
        /** Values the thumb snaps to when dragged within two steps of them. */
        snapTargets?: number[]
        /** Quiet captions under the track's two ends, `[start, end]`. */
        endLabels?: [string, string]
        /** Live readout of the current value, already formatted ("125%", "4"). */
        valueLabel?: string
        /** `trailing` sits right of the track; `above` sits over its left end. */
        valueLabelPlacement?: 'trailing' | 'above'
        /** Double-clicking the thumb calls this (settings rows reset to the default). */
        onThumbDoubleClick?: () => void
        /** Extra attributes for the thumb, for example a `data-test` hook. */
        thumbProps?: Record<string, string>
    }

    const {
        value,
        onChange,
        min,
        max,
        step = 1,
        disabled = false,
        ariaLabel,
        ariaValueText,
        ticks = [],
        snapTargets = [],
        endLabels,
        valueLabel,
        valueLabelPlacement = 'trailing',
        onThumbDoubleClick,
        thumbProps,
    }: Props = $props()

    /** Percent along the track, for absolutely-positioned tick marks. */
    function positionOf(target: number): number {
        if (max === min) return 0
        return ((target - min) / (max - min)) * 100
    }

    function handleValueChange(details: SliderValueChangeDetails): void {
        const raw = details.value[0]
        onChange(snap(raw))
    }

    /** Magnetic snapping: within two steps of a notable value, land on it exactly. */
    function snap(raw: number): number {
        if (snapTargets.length === 0) return raw
        const closest = snapTargets.reduce((prev, curr) =>
            Math.abs(curr - raw) < Math.abs(prev - raw) ? curr : prev,
        )
        return Math.abs(closest - raw) < step * 2 ? closest : raw
    }
</script>

<div class="sl-wrapper">
    <Slider.Root
        class="sl-root"
        value={[value]}
        onValueChange={handleValueChange}
        {min}
        {max}
        {step}
        {disabled}
        aria-label={[ariaLabel]}
        getAriaValueText={ariaValueText ? (details) => ariaValueText(details.value) : undefined}
    >
        {#if valueLabel && valueLabelPlacement === 'above'}
            <div class="sl-value-above" aria-hidden="true">{valueLabel}</div>
        {/if}

        <Slider.Control class="sl-control">
            <Slider.Track class="sl-track">
                <Slider.Range class="sl-range" />
            </Slider.Track>
            <Slider.Thumb index={0} class="sl-thumb" ondblclick={onThumbDoubleClick} {...thumbProps} />
            {#if ticks.length > 0}
                <div class="sl-ticks" aria-hidden="true">
                    {#each ticks as tick (tick)}
                        <span
                            class="sl-tick"
                            class:is-active={value === tick}
                            style="left: {positionOf(tick)}%"
                        ></span>
                    {/each}
                </div>
            {/if}
        </Slider.Control>

        {#if endLabels}
            <div class="sl-ends" aria-hidden="true">
                <span>{endLabels[0]}</span>
                <span>{endLabels[1]}</span>
            </div>
        {/if}
    </Slider.Root>

    {#if valueLabel && valueLabelPlacement === 'trailing'}
        <span class="sl-value" aria-hidden="true">{valueLabel}</span>
    {/if}
</div>

<style>
    /* Selectors handed to an Ark part are `:global(...)`: Svelte 5 doesn't propagate this
       component's scoping hash through a `class` prop forwarded into a third-party component,
       so a scoped selector would whiff against the Ark-rendered DOM. The `sl-` prefix is this
       component's alone, which is what keeps the unscoping safe. */
    .sl-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        min-width: 0;
        width: 100%;
    }

    /* Ark's slider needs an explicitly sized root. `min-width` lets the track shrink in a
       narrow row while the thumb stays draggable; below that Ark handles the cramped layout. */
    :global(.sl-root) {
        flex: 1;
        min-width: 60px;
    }

    :global(.sl-control) {
        position: relative;
        display: flex;
        align-items: center;
        height: 20px;
        width: 100%;
    }

    :global(.sl-track) {
        flex: 1;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        position: relative;
    }

    :global(.sl-range) {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width var(--transition-base);
    }

    :global(.sl-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-full);
        cursor: default;
        box-shadow: var(--shadow-sm);
        /* Above the tick marks. */
        z-index: 2;
        position: relative;
    }

    :global(.sl-thumb:hover) {
        border-color: var(--color-accent-hover);
    }

    :global(.sl-thumb[data-disabled]) {
        cursor: not-allowed;
    }

    :global(.sl-thumb:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }

    .sl-ticks {
        position: absolute;
        left: 0;
        right: 0;
        top: 50%;
        transform: translateY(-50%);
        height: 4px;
        pointer-events: none;
        /* Below the thumb. */
        z-index: 0;
    }

    .sl-tick {
        position: absolute;
        width: 2px;
        height: 8px;
        background: var(--color-border);
        transform: translate(-50%, -50%);
        top: 50%;
    }

    .sl-tick.is-active {
        background: var(--color-accent);
    }

    .sl-ends {
        display: flex;
        justify-content: space-between;
        margin-top: var(--spacing-xs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .sl-value-above {
        margin-bottom: var(--spacing-xs);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* `min-width` keeps the track from twitching as the readout's width changes
       ("75%" → "100%"), and the text stays left-aligned so it reads as a label. */
    .sl-value {
        min-width: 4ch;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-variant-numeric: tabular-nums;
    }

    /* Honor reduced-motion: drop the range-fill transition (WKWebView reflects this media
       query for prefers-reduced-motion, unlike prefers-reduced-transparency). */
    @media (prefers-reduced-motion: reduce) {
        :global(.sl-range) {
            transition: none;
        }
    }
</style>
