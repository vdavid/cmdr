<script lang="ts">
    /**
     * One question a tool put to a person. Presentational: `ToolAskHost` holds the question, and the
     * gallery renders this with fixtures. Always under the arming guard, since whoever asked isn't
     * at the keyboard. A long value scrolls inside the body, so it can't push the buttons off-screen.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import type { AskFacts, Initiator } from '#lib/ipc/bindings.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { startedByWhenGuarded } from '#lib/ui/started-elsewhere-guard.js'
    import { askSentence } from './tool-ask-sentence'

    interface Props {
        facts: AskFacts
        startedBy: Initiator
        onAllow: () => void
        /** Also what Escape, the ×, and `dialog close` do. */
        onDecline: () => void
    }

    const { facts, startedBy, onAllow, onDecline }: Props = $props()

    const sentence = $derived(askSentence(facts))
</script>

<ModalDialog
    dialogId="tool-ask"
    titleId="tool-ask-title"
    ariaDescribedby="tool-ask-body"
    startedBy={startedByWhenGuarded(startedBy, true)}
    onclose={onDecline}
    containerStyle="max-width: 480px"
>
    {#snippet title()}{sentence.title}{/snippet}

    <div class="tool-ask-text">
        <p id="tool-ask-body" class="tool-ask-body">{sentence.body}</p>
        {#if sentence.detail}
            <p class="tool-ask-detail">{sentence.detail}</p>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" autoFocus onclick={onDecline}>{tString('ui.toolAsk.decline')}</Button>
        <Button variant="primary" onclick={onAllow}>{tString('ui.toolAsk.allow')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .tool-ask-text {
        max-height: 40vh;
        overflow-y: auto;
    }

    .tool-ask-body,
    .tool-ask-detail {
        margin: 0;
        overflow-wrap: anywhere;
    }

    .tool-ask-body {
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .tool-ask-detail {
        margin-top: var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }
</style>
