<script lang="ts">
    /**
     * The shared body of the two "a report went out" toasts: an optional title, one
     * sentence ending in the report id badge, and a right-aligned pair of mini buttons.
     * `AutoSendToastContent` (Flow B) and `ErrorReportToastContent` (Flow A, sent or
     * amended) differ only in their words and their buttons.
     */
    import type { Snippet } from 'svelte'

    interface Props {
        /** Bold lead line above the sentence; omitted when the sentence carries the news itself. */
        title?: string
        /** The sentence before the id badge. */
        message: string
        /** The report id, rendered as a monospace badge. */
        reportId: string
        /** The buttons, right-aligned. */
        actions: Snippet
    }

    const { title, message, reportId, actions }: Props = $props()
</script>

<div class="content">
    {#if title}
        <div class="title">{title}</div>
    {/if}
    <div class="body">
        {message}
        <span class="id-badge">{reportId}</span>
    </div>
    <div class="actions">
        {@render actions()}
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .id-badge {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
