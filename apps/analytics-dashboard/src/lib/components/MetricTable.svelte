<!-- A simple two-column "label / value" table for ranked breakdowns (referrers, pages, channels, etc.). -->
<script lang="ts">
    import { formatNumber } from '$lib/format.js'

    let {
        items,
        colLabel,
        colValue,
    }: { items: Array<{ x: string; y: number }>; colLabel: string; colValue: string } = $props()
</script>

<div class="overflow-x-auto">
    <table class="w-full text-left text-sm">
        <thead>
            <tr class="border-b border-border-subtle text-text-tertiary">
                <th class="pb-2 pr-4 font-medium">{colLabel}</th>
                <th class="pb-2 text-right font-medium">{colValue}</th>
            </tr>
        </thead>
        <tbody>
            {#each items as item}
                <tr class="border-b border-border-subtle/50">
                    <td class="py-1.5 pr-4 text-text-primary">{item.x || '(direct)'}</td>
                    <td class="py-1.5 text-right tabular-nums text-text-secondary">{formatNumber(item.y)}</td>
                </tr>
            {/each}
        </tbody>
    </table>
</div>
