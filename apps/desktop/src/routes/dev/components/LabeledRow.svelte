<script lang="ts">
    /** One sample row in a Components catalog section: a fixed-width caption, then the samples. */
    import type { Snippet } from 'svelte'

    interface Props {
        label: string
        children: Snippet
    }

    const { label, children }: Props = $props()
</script>

<div class="row">
    <span class="row-label">{label}</span>
    {@render children()}
</div>

<style>
    .row {
        display: flex;
        gap: var(--spacing-lg);
        align-items: center;
        flex-wrap: wrap;
    }

    .row-label {
        min-width: 140px;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
