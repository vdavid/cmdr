<script lang="ts">
    interface Props {
        slices: Array<{ label: string; value: number }>
        size?: number
    }

    const { slices, size = 72 }: Props = $props()

    const colors = [
        '#ffc206', '#22c55e', '#3b82f6', '#ef4444',
        '#a855f7', '#f97316', '#06b6d4', '#ec4899',
        '#84cc16', '#6366f1',
    ]

    const total = $derived(slices.reduce((sum, s) => sum + s.value, 0))

    /** SVG path coordinates: `d` is a string attribute, so every number goes in as text. */
    function n(value: number): string {
        return String(value)
    }

    const arcs = $derived.by(() => {
        let angle = -Math.PI / 2
        return slices.map((slice, i) => {
            const frac = total > 0 ? slice.value / total : 0
            const sweep = frac * 2 * Math.PI
            const end = angle + sweep
            const r = 40
            const cx = 50
            const cy = 50

            let path: string
            if (frac >= 0.9999) {
                path = `M ${n(cx)},${n(cy - r)} A ${n(r)},${n(r)} 0 1,1 ${n(cx - 0.01)},${n(cy - r)} Z`
            } else if (frac <= 0.0001) {
                path = ''
            } else {
                const x1 = cx + r * Math.cos(angle)
                const y1 = cy + r * Math.sin(angle)
                const x2 = cx + r * Math.cos(end)
                const y2 = cy + r * Math.sin(end)
                const large = sweep > Math.PI ? 1 : 0
                path = `M ${n(cx)},${n(cy)} L ${n(x1)},${n(y1)} A ${n(r)},${n(r)} 0 ${n(large)},1 ${n(x2)},${n(y2)} Z`
            }

            const result = {
                label: slice.label,
                value: slice.value,
                color: colors[i % colors.length],
                path,
                frac,
            }
            angle = end
            return result
        })
    })
</script>

<div>
    <svg viewBox="0 0 100 100" width={size} height={size} role="img" class="pointer-events-none">
        {#each arcs as arc (arc.label)}
            {#if arc.path}
                <path
                    d={arc.path}
                    fill={arc.color}
                    stroke="var(--color-surface-elevated)"
                    stroke-width="1.5"
                />
            {/if}
        {/each}
    </svg>
    <div class="mt-1 space-y-px">
        {#each arcs as arc (arc.label)}
            <div class="flex items-center gap-1.5 text-xs leading-tight">
                <span style="color: {arc.color}" class="text-[10px]">●</span>
                <span class="text-text-secondary">{arc.label}</span>
                <span class="ml-auto tabular-nums text-text-tertiary">{arc.value}</span>
                <span class="w-9 tabular-nums text-text-tertiary text-right">{(arc.frac * 100).toFixed(0)}%</span>
            </div>
        {/each}
    </div>
</div>
