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
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }

    .section :global(.section-action-btn) {
        padding: 3px 12px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        cursor: default;
        transition: all var(--transition-base);
    }

    .section :global(.section-action-btn:hover:not(:disabled)) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .section :global(.section-action-btn:disabled) {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .section :global(.section-action-btn:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: 0 0 0 4px rgba(0, 0, 0, 0.1);
    }

    @media (prefers-color-scheme: dark) {
        .section :global(.section-action-btn:focus-visible) {
            box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.08);
        }
    }
</style>
