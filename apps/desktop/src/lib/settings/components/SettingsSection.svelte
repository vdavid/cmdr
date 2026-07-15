<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        title: string
        /** Optional trailing content beside the title (e.g. a feature-status badge). */
        badge?: Snippet
        children: Snippet
    }

    const { title, badge, children }: Props = $props()
</script>

<div class="section">
    <div class="section-header">
        <h2 class="section-title">{title}</h2>
        {#if badge}{@render badge()}{/if}
    </div>
    {@render children()}
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* The larger bottom margin gives the title breathing room from the rows
           below without a horizontal hairline, matching System Settings. */
        margin: 0 0 var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0;
    }
</style>
