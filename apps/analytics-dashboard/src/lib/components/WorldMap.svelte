<!--
  A world map of countries filled on the fixed count color scale (`#lib/count-color-scale.ts`). Hovering,
  focusing, or tapping a country shows the caller's tooltip for it; a second tap, Escape, or a tap elsewhere
  closes a pinned one. Only countries with data are focusable, so Tab walks the data and skips the many
  zeros. The geometry loads lazily and projects once (`#lib/world-map-shapes.ts`).
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import { onMount } from 'svelte'
    import { countColor, countScaleStops } from '#lib/count-color-scale.js'
    import { type MapGeometry, type MapShape, loadMapGeometry } from '#lib/world-map-shapes.js'
    import { formatNumber } from '#lib/format.js'

    interface Props {
        /** The count an alpha-2 code fills with, or `null` when its source couldn't answer ("no data"). */
        valueOf: (country: string) => number | null
        /** Whether a country has a row worth a tooltip; only those are focusable. */
        hasData: (country: string) => boolean
        /** Whether to put "No data" in the legend (when the fill's source is down). */
        showNoData: boolean
        /** The accessible label of a country, like "Sweden (SE): 12 visitors". */
        labelOf: (country: string) => string
        tooltip: Snippet<[string]>
        /** What the fill counts, for the legend ("visitors"). */
        measure: string
    }

    const { valueOf, hasData, showNoData, labelOf, tooltip, measure }: Props = $props()

    /** Fill for a country whose source didn't answer: darker than zero, so "unknown" never passes for "nobody". */
    const noDataColor = '#262522'

    let geometry = $state<MapGeometry | null>(null)
    let loadFailed = $state(false)

    onMount(() => {
        loadMapGeometry().then(
            (g) => (geometry = g),
            () => (loadFailed = true),
        )
    })

    let container = $state<HTMLDivElement>()
    /** The open tooltip: which country, where (container px), and whether a tap pinned it. */
    let active = $state<{ country: string; x: number; y: number; pinned: boolean } | null>(null)

    function fillOf(country: string): string {
        const value = valueOf(country)
        return value === null ? noDataColor : countColor(value)
    }

    function pointAt(e: { clientX: number; clientY: number }): { x: number; y: number } {
        const box = container?.getBoundingClientRect()
        return { x: e.clientX - (box?.left ?? 0), y: e.clientY - (box?.top ?? 0) }
    }

    function centerOf(el: Element): { x: number; y: number } {
        const r = el.getBoundingClientRect()
        return pointAt({ clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 })
    }

    function onPointerMove(country: string, e: PointerEvent) {
        if (e.pointerType !== 'mouse' || active?.pinned) return
        active = { country, ...pointAt(e), pinned: false }
    }

    function onPointerLeave(e: PointerEvent) {
        if (e.pointerType === 'mouse' && !active?.pinned) active = null
    }

    function onClick(country: string, e: MouseEvent) {
        if (active?.pinned && active.country === country) {
            active = null
            return
        }
        active = { country, ...pointAt(e), pinned: true }
    }

    /** An SVG `role="button"` gets no synthetic click from the keyboard, so Enter and Space pin it here. */
    function onShapeKeydown(country: string, e: KeyboardEvent) {
        if (e.key !== 'Enter' && e.key !== ' ') return
        e.preventDefault()
        const pinned = !(active?.pinned && active.country === country)
        active = { country, ...centerOf(e.currentTarget as Element), pinned }
    }

    function onFocus(country: string, e: FocusEvent) {
        if (active?.pinned) return
        active = { country, ...centerOf(e.currentTarget as Element), pinned: false }
    }

    function onBlur() {
        if (!active?.pinned) active = null
    }

    function onDocumentPointerDown(e: PointerEvent) {
        if (active?.pinned && !container?.contains(e.target as Node)) active = null
    }

    function onDocumentKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') active = null
    }

    /** The open country's outline, drawn last so its highlight isn't hidden under a neighbor. */
    const activeShapes = $derived(
        geometry && active ? geometry.shapes.filter((s) => s.country === active?.country) : ([] as MapShape[]),
    )
    /** Keep the tooltip inside the map: flip it left of the point past the middle. */
    const flipLeft = $derived(active !== null && active.x > (container?.clientWidth ?? 0) / 2)
    const stops = countScaleStops()
</script>

<svelte:document onpointerdown={onDocumentPointerDown} onkeydown={onDocumentKeydown} />

<div>
    <div bind:this={container} class="relative">
        {#if geometry}
            <svg
                viewBox="0 0 {geometry.width} {geometry.height}"
                class="block h-auto w-full"
                role="group"
                aria-label="World map of {measure} by country"
            >
                {#each geometry.shapes as shape (shape.id)}
                    {#if hasData(shape.country)}
                        <path
                            d={shape.d}
                            fill={fillOf(shape.country)}
                            class="map-shape cursor-pointer"
                            role="button"
                            tabindex="0"
                            aria-label={labelOf(shape.country)}
                            onpointermove={(e) => {
                                onPointerMove(shape.country, e)
                            }}
                            onpointerleave={onPointerLeave}
                            onclick={(e) => {
                                onClick(shape.country, e)
                            }}
                            onkeydown={(e) => {
                                onShapeKeydown(shape.country, e)
                            }}
                            onfocus={(e) => {
                                onFocus(shape.country, e)
                            }}
                            onblur={onBlur}
                        />
                    {:else}
                        <path
                            d={shape.d}
                            fill={fillOf(shape.country)}
                            class="map-shape"
                            role="presentation"
                            onpointermove={(e) => {
                                onPointerMove(shape.country, e)
                            }}
                            onpointerleave={onPointerLeave}
                        />
                    {/if}
                {/each}
                {#each activeShapes as shape (shape.id)}
                    <path d={shape.d} class="map-highlight" />
                {/each}
            </svg>
            {#if active}
                <div
                    role="tooltip"
                    class="pointer-events-none absolute z-30 w-64 max-w-[calc(100vw-3rem)] rounded-lg border border-border bg-surface-elevated p-3 text-xs text-text-secondary shadow-lg"
                    style:left="{flipLeft ? active.x - 12 : active.x + 12}px"
                    style:top="{active.y + 12}px"
                    style:transform={flipLeft ? 'translateX(-100%)' : undefined}
                >
                    {@render tooltip(active.country)}
                </div>
            {/if}
        {:else if loadFailed}
            <p class="py-8 text-center text-sm text-text-tertiary">Couldn't draw the map. Reload the page to try again.</p>
        {:else}
            <div class="aspect-2/1 w-full animate-pulse rounded-lg bg-surface-elevated/50"></div>
        {/if}
    </div>

    <!-- Legend: the fixed scale, zero to the purple end. -->
    <div class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-tertiary">
        <span class="capitalize">{measure}</span>
        <ol class="flex flex-wrap items-center gap-x-2.5 gap-y-1" aria-label="Color scale for {measure}">
            {#each stops as stop, i (stop.value)}
                <li class="inline-flex items-center gap-1">
                    <span class="inline-block size-3 rounded-sm" style:background-color={stop.color}></span>
                    <span class="tabular-nums">{formatNumber(stop.value)}{i === stops.length - 1 ? '+' : ''}</span>
                </li>
            {/each}
            {#if showNoData}
                <li class="inline-flex items-center gap-1">
                    <span class="inline-block size-3 rounded-sm border border-border" style:background-color={noDataColor}
                    ></span>
                    <span>No data</span>
                </li>
            {/if}
        </ol>
    </div>
</div>

<style>
    .map-shape {
        stroke: var(--color-surface);
        stroke-width: 0.6;
        stroke-linejoin: round;
        outline: none;
    }

    .map-highlight {
        fill: none;
        stroke: var(--color-text-primary);
        stroke-width: 1.5;
        stroke-linejoin: round;
        pointer-events: none;
    }
</style>
