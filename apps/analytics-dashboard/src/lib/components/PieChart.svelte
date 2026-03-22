<script lang="ts">
    interface Props {
        slices: Array<{ label: string; value: number }>
        size?: number
    }

    let { slices, size = 72 }: Props = $props()

    const colors = [
        '#ffc206', '#22c55e', '#3b82f6', '#ef4444',
        '#a855f7', '#f97316', '#06b6d4', '#ec4899',
        '#84cc16', '#6366f1',
    ]

    const total = $derived(slices.reduce((sum, s) => sum + s.value, 0))

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
                path = `M ${cx},${cy - r} A ${r},${r} 0 1,1 ${cx - 0.01},${cy - r} Z`
            } else if (frac <= 0.0001) {
                path = ''
            } else {
                const x1 = cx + r * Math.cos(angle)
                const y1 = cy + r * Math.sin(angle)
                const x2 = cx + r * Math.cos(end)
                const y2 = cy + r * Math.sin(end)
                const large = sweep > Math.PI ? 1 : 0
                path = `M ${cx},${cy} L ${x1},${y1} A ${r},${r} 0 ${large},1 ${x2},${y2} Z`
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
        {#each arcs as arc}
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
        {#each arcs as arc}
            <div class="flex items-center gap-1.5 text-xs leading-tight">
                <span style="color: {arc.color}" class="text-[10px]">●</span>
                <span class="text-text-secondary">{arc.label}</span>
                <span class="ml-auto tabular-nums text-text-tertiary">{arc.value}</span>
                <span class="w-9 tabular-nums text-text-tertiary text-right">{(arc.frac * 100).toFixed(0)}%</span>
            </div>
        {/each}
    </div>
</div>
