<!-- A "couldn't load this data" panel with a "Try again" link that re-requests the same selection. -->
<script lang="ts">
    import type { DashboardSelection } from '$lib/server/types.js'

    let { error, selection }: { error: string; selection: DashboardSelection } = $props()

    const tryAgainHref = $derived(
        selection.range === 'day' && selection.day ? `?day=${selection.day}` : `?range=${selection.range}`,
    )
</script>

<div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
    <p class="text-sm text-text-secondary">Couldn't load this data</p>
    <p class="mt-1 text-xs text-text-tertiary">{error}</p>
    <a href={tryAgainHref} class="mt-2 inline-block text-xs text-accent hover:text-accent-hover"> Try again </a>
</div>
