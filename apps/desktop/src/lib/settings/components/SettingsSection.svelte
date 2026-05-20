<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        title: string
        children: Snippet
    }

    const { title, children }: Props = $props()
</script>

<div class="section">
    <h2 class="section-title">{title}</h2>
    {@render children()}
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        /* Border / padding removed — the larger bottom margin gives the title
           breathing room from the rows below without a horizontal hairline,
           matching System Settings. */
        margin: 0 0 var(--spacing-lg);
    }
</style>
