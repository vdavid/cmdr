<!--
  A breakdown as a ring (plain SVG): the slices from `#lib/pie-slices.ts` (folded, shared, colored), the
  total in the middle, and a legend listing every slice's count and share, which doubles as the table view.
  Hover, focus, or tap a slice (or a legend row) for its tooltip; a tap or Enter pins it until a second
  tap, Escape, or a tap elsewhere. "Other" lists what folded into it.
-->
<script lang="ts">
    import { donutArc, type PieSlice } from '#lib/pie-slices.js'
    import { formatNumber, formatPercent } from '#lib/format.js'

    interface Props {
        slices: PieSlice[]
        /** The accessible name of the ring ("Downloads by channel"). */
        label: string
        /** What the number in the middle counts ("downloads"). */
        unit: string
    }

    const { slices, label, unit }: Props = $props()

    const size = 144
    const r = 50
    const inner = 32
    const c = 50
    /** How many folded entries the "Other" tooltip names before "and N more". */
    const otherListMax = 8

    const total = $derived(slices.reduce((sum, s) => sum + s.count, 0))
    const arcs = $derived.by(() => {
        let start = 0
        return slices.map((slice) => {
            const end = start + slice.share
            const arc = { slice, start, end, path: donutArc(start, end, r, inner, c, c) }
            start = end
            return arc
        })
    })

    /** The slice the tooltip shows, where it sits (px within the wrapper), and whether a tap pinned it. */
    let active = $state<{ index: number; x: number; y: number } | null>(null)
    let pinned = $state(false)
    let root: HTMLDivElement
    let width = $state(0)

    /** A slice's mid-ring point in px, where the tooltip sits for keyboard focus. */
    function anchor(index: number): { x: number; y: number } {
        const arc = arcs[index]
        const angle = ((arc.start + arc.end) / 2) * 2 * Math.PI - Math.PI / 2
        const mid = ((r + inner) / 2) * (size / 100)
        return { x: size / 2 + mid * Math.cos(angle), y: size / 2 + mid * Math.sin(angle) }
    }

    /** Show slice `index`'s tooltip at the pointer, under a hovered legend row, or at the slice itself. */
    function show(index: number, at?: MouseEvent | HTMLElement) {
        if (pinned && active?.index !== index) pinned = false
        const box = root.getBoundingClientRect()
        if (at instanceof HTMLElement) {
            const row = at.getBoundingClientRect()
            active = { index, x: row.left - box.left - 12, y: row.bottom - box.top - 8 }
        } else if (at) {
            active = { index, x: at.clientX - box.left, y: at.clientY - box.top }
        } else {
            active = { index, ...anchor(index) }
        }
    }

    function hide() {
        if (!pinned) active = null
    }

    function toggle(index: number, e?: MouseEvent) {
        if (pinned && active?.index === index) {
            pinned = false
            active = null
        } else {
            show(index, e)
            pinned = true
        }
    }

    function onKeydown(e: KeyboardEvent, index: number) {
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            toggle(index)
        } else if (e.key === 'Escape') {
            pinned = false
            active = null
        }
    }

    function onDocumentPointerDown(e: PointerEvent) {
        if (pinned && !root.contains(e.target as Node)) {
            pinned = false
            active = null
        }
    }

    function readout(slice: PieSlice): string {
        return `${formatNumber(slice.count)} · ${formatPercent(slice.share)}`
    }

    const activeSlice = $derived(active ? slices.at(active.index) : undefined)
</script>

<svelte:document onpointerdown={onDocumentPointerDown} />

<div bind:this={root} bind:clientWidth={width} class="relative flex flex-wrap items-start gap-x-5 gap-y-3">
    <svg width={size} height={size} viewBox="0 0 100 100" class="shrink-0" role="group" aria-label={label}>
        {#each arcs as arc, i (arc.slice.key)}
            <path
                d={arc.path}
                fill={arc.slice.color}
                stroke="var(--color-surface)"
                stroke-width="2"
                vector-effect="non-scaling-stroke"
                opacity={active && active.index !== i ? 0.45 : 1}
                class="cursor-pointer outline-none"
                role="button"
                tabindex="0"
                aria-label="{arc.slice.label}: {readout(arc.slice)}"
                aria-pressed={pinned && active?.index === i}
                onpointerenter={(e) => { if (e.pointerType === 'mouse') show(i, e) }}
                onpointermove={(e) => { if (e.pointerType === 'mouse' && !pinned) show(i, e) }}
                onpointerleave={(e) => { if (e.pointerType === 'mouse') hide() }}
                onclick={(e) => { toggle(i, e) }}
                onkeydown={(e) => { onKeydown(e, i) }}
                onfocus={() => { show(i) }}
                onblur={hide}
            />
        {/each}
        <text x={c} y={c - 1} text-anchor="middle" class="fill-text-primary text-[13px] font-semibold tabular-nums">
            {formatNumber(total)}
        </text>
        <text x={c} y={c + 10} text-anchor="middle" class="fill-text-tertiary text-[7px]">{unit}</text>
    </svg>

    <ul class="min-w-40 flex-1 space-y-1 text-xs">
        {#each slices as slice, i (slice.key)}
            <li
                role="presentation"
                class="flex items-center gap-2 rounded-sm px-1 {active?.index === i ? 'bg-surface-elevated' : ''}"
                onpointerenter={(e) => { if (e.pointerType === 'mouse') show(i, e.currentTarget) }}
                onpointerleave={(e) => { if (e.pointerType === 'mouse') hide() }}
            >
                <span class="inline-block size-2 shrink-0 rounded-sm" style="background-color: {slice.color};"></span>
                <span class="min-w-0 truncate text-text-secondary" title={slice.label}>{slice.label}</span>
                <span class="ml-auto text-text-secondary tabular-nums">{formatNumber(slice.count)}</span>
                <span class="w-10 text-right text-text-tertiary tabular-nums">{formatPercent(slice.share)}</span>
            </li>
        {/each}
    </ul>

    {#if active && activeSlice}
        {@const onLeft = active.x > width * 0.6}
        <div
            class="pointer-events-none absolute z-20 max-w-64 min-w-40 rounded-lg border border-border bg-surface-elevated p-2.5 text-xs shadow-lg"
            style="top: {active.y + 12}px; {onLeft ? `right: calc(100% - ${String(active.x - 12)}px)` : `left: ${String(active.x + 12)}px`};"
            aria-hidden="true"
        >
            <div class="flex items-center justify-between gap-4">
                <span class="flex min-w-0 items-center gap-1.5 text-text-tertiary">
                    <span class="inline-block size-2 shrink-0 rounded-sm" style="background-color: {activeSlice.color};"></span>
                    <span class="truncate">{activeSlice.label}</span>
                </span>
                <span class="font-semibold whitespace-nowrap text-text-primary tabular-nums">{readout(activeSlice)}</span>
            </div>
            {#if activeSlice.folded}
                <p class="mt-1.5 text-[10px] tracking-wide text-text-tertiary uppercase">Folded in</p>
                {#each activeSlice.folded.slice(0, otherListMax) as entry (entry.key)}
                    <div class="flex items-center justify-between gap-4">
                        <span class="truncate text-text-tertiary">{entry.label}</span>
                        <span class="text-text-secondary tabular-nums">
                            {formatNumber(entry.count)} · {formatPercent(entry.count / total)}
                        </span>
                    </div>
                {/each}
                {#if activeSlice.folded.length > otherListMax}
                    <p class="text-text-tertiary">and {formatNumber(activeSlice.folded.length - otherListMax)} more</p>
                {/if}
            {/if}
        </div>
    {/if}
</div>
