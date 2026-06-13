<!-- A horizontal row of headline metrics, each with an optional colored dot and percent delta. -->
<script lang="ts">
    interface Metric {
        label: string
        value: string
        delta?: { text: string; positive: boolean }
        color?: string
    }

    let { metrics }: { metrics: Metric[] } = $props()
</script>

<div class="flex flex-wrap gap-6">
    {#each metrics as metric}
        <div>
            <p class="flex items-center gap-1.5 text-xs text-text-tertiary">
                {#if metric.color}
                    <span class="inline-block h-2 w-2 rounded-full" style="background: {metric.color}"></span>
                {/if}
                {metric.label}
            </p>
            <div class="flex items-baseline gap-2">
                <p class="text-2xl font-bold tabular-nums text-text-primary">{metric.value}</p>
                {#if metric.delta}
                    <span class="text-sm tabular-nums {metric.delta.positive ? 'text-success' : 'text-danger'}">
                        {metric.delta.text}
                    </span>
                {/if}
            </div>
        </div>
    {/each}
</div>
