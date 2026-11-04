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
        /* A reserved gutter, given back from the right padding: without it a classic
           scrollbar leaves the content 17px too wide (#889). `#lib/ui/DETAILS.md` §
           "Scrollbar gutters". */
        scrollbar-gutter: stable;
        padding-right: calc(var(--spacing-2xl) - var(--spacing-scrollbar-width));
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }
</style>
