<script lang="ts">
    /**
     * The body every once-ever offer toast wears: a title, one or two lines of
     * explanation, and a decline / accept pair.
     *
     * Presentational only. It takes resolved copy rather than message keys, so
     * each offer keeps its own `tString` calls in its own component and the
     * strings still re-render on a live language switch.
     */
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        /** The question, one line. */
        title: string
        /** Why it's being asked, and what saying yes does. */
        body: string
        /** The reassurance line: how to undo this later. */
        note: string
        /** "No, thanks". */
        declineLabel: string
        /** The affirmative, which is also the primary button. */
        acceptLabel: string
        onDecline: () => void
        onAccept: () => void
    }

    const { title, body, note, declineLabel, acceptLabel, onDecline, onAccept }: Props = $props()
</script>

<div class="content">
    <strong class="title">{title}</strong>
    <span class="body">{body}</span>
    <span class="body">{note}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={onDecline}>{declineLabel}</Button>
        <Button size="mini" variant="primary" onclick={onAccept}>{acceptLabel}</Button>
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

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
