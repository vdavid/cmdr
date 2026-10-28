<!--
  The note under an assistant turn that reached one of Cmdr's per-answer ceilings: which one,
  its value, that it's fixed (no setting moves it), a way to say so through Send feedback, and,
  on the thread's latest turn, a button that sends "keep going" so the next turn picks up from
  the history this one left. Read from the turn's stats, so it says the same after a reload.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Button from '#lib/ui/Button.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { openFeedbackDialog } from '#lib/feedback/feedback-flow.svelte.js'
    import type { TurnLimit } from '#lib/ipc/bindings.js'
    import { limitNoticeText } from './ask-cmdr-labels'

    interface Props {
        limit: TurnLimit
        /** The turn is the thread's latest and nothing runs, so continuing makes sense. */
        canContinue: boolean
        onContinue: () => void
    }
    const { limit, canContinue, onContinue }: Props = $props()

    const text = $derived(limitNoticeText(limit))
</script>

{#snippet feedback(children: Snippet)}
    <LinkButton onclick={openFeedbackDialog}>{@render children()}</LinkButton>
{/snippet}

<div class="limit-notice" role="status">
    <Icon name="triangle-alert" size={14} aria-hidden="true" />
    <div class="stack">
        <span>{text}</span>
        <span class="fixed"><Trans key="askCmdr.limit.fixed" snippets={{ feedback }} /></span>
        {#if canContinue}
            <div class="actions">
                <Button size="mini" variant="secondary" onclick={onContinue}>{tString('askCmdr.limit.continue')}</Button>
            </div>
        {/if}
    </div>
</div>

<style>
    .limit-notice {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .limit-notice > :global(svg) {
        flex-shrink: 0;
        margin-top: var(--spacing-xxs);
    }

    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .fixed {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .actions {
        display: flex;
        margin-top: var(--spacing-xxs);
    }
</style>
