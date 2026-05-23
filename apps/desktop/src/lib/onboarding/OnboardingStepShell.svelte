<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        children: Snippet
    }

    const { children }: Props = $props()
</script>

<div class="step-shell">
    {@render children()}
</div>

<style>
    .step-shell {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: var(--spacing-2xl);
        font-size: var(--font-size-md);
        line-height: 1.5;
        color: var(--color-text-primary);
    }
</style>
