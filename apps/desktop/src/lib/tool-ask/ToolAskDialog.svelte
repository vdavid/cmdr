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

    <div id="tool-ask-body" class="tool-ask-text">
        <p class="tool-ask-body" class:setting-name={sentence.values !== undefined}>{sentence.body}</p>
        {#if sentence.detail}
            <p class="tool-ask-detail">{sentence.detail}</p>
        {/if}
        {#if sentence.values}
            <dl class="tool-ask-values">
                {#each sentence.values as row (row.caption)}
                    <dt>{row.caption}</dt>
                    <dd>
                        {#if row.name !== null}<span class="value-name">{row.name}</span>{/if}
                        {#if row.code !== null}<code class="value-code">{row.code}</code>{/if}
                    </dd>
                {/each}
            </dl>
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

    .setting-name {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* Captions in their own column, so both values start at the same edge and a long one wraps
       under itself, never under its caption. */
    .tool-ask-values {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr);
        gap: var(--spacing-xs) var(--spacing-sm);
        margin: var(--spacing-sm) 0 0;
        padding: 0;
        font-size: var(--font-size-md);
    }

    .tool-ask-values dt {
        color: var(--color-text-tertiary);
    }

    .tool-ask-values dd {
        margin: 0;
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
    }

    /* Subtle inline code: a chip in the mono face. The tint is the text color at low strength, so it
       reads on the glass dialog in both themes (a flat background token vanishes on light glass).
       `box-decoration-break` keeps the chip's padding on every line a long id wraps onto. */
    .value-code {
        padding: 0.1em 0.35em;
        font-family: var(--font-mono);
        font-size: 0.85em;
        color: var(--color-text-secondary);
        background: color-mix(in srgb, var(--color-text-primary), transparent 91%);
        border-radius: var(--radius-xs);
        -webkit-box-decoration-break: clone;
        box-decoration-break: clone;
    }
</style>
