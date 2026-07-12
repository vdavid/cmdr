<!--
  The Ask Cmdr message composer: a growing textarea plus a send/stop button. Enter sends,
  Shift+Enter inserts a newline, Escape returns focus to the active pane. Disabled while a
  turn streams (single-flight per thread); the button flips to Stop then.
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { askCmdrState, markRailFocused, returnFocusToPane, sendMessage, stopStreaming } from './ask-cmdr-trigger.svelte'

    let text = $state('')
    let textarea = $state<HTMLTextAreaElement | null>(null)
    let composing = $state(false)

    // Focus the composer when it mounts (the rail mounts on open, so opening focuses here).
    $effect(() => {
        textarea?.focus()
    })

    const streaming = $derived(askCmdrState.streaming)
    const canSend = $derived(text.trim().length > 0)

    function submit(): void {
        if (askCmdrState.streaming || !canSend) return
        sendMessage(text)
        text = ''
    }

    function onKeydown(event: KeyboardEvent): void {
        if (event.key === 'Enter' && !event.shiftKey && !composing) {
            event.preventDefault()
            submit()
        } else if (event.key === 'Escape') {
            event.preventDefault()
            returnFocusToPane()
        }
    }
</script>

<div class="composer">
    <textarea
        bind:this={textarea}
        bind:value={text}
        class="composer-input"
        rows="2"
        placeholder={tString('askCmdr.composer.placeholder')}
        aria-label={tString('askCmdr.composer.placeholder')}
        onkeydown={onKeydown}
        onfocus={markRailFocused}
        oncompositionstart={() => (composing = true)}
        oncompositionend={() => (composing = false)}
    ></textarea>
    {#if streaming}
        <button type="button" class="composer-button stop" onclick={stopStreaming} aria-label={tString('askCmdr.composer.stop')}>
            <Icon name="square" size={16} aria-hidden="true" />
        </button>
    {:else}
        <button
            type="button"
            class="composer-button"
            onclick={submit}
            disabled={!canSend}
            aria-label={tString('askCmdr.composer.send')}
        >
            <Icon name="corner-down-left" size={16} aria-hidden="true" />
        </button>
    {/if}
</div>

<style>
    .composer {
        display: flex;
        align-items: flex-end;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .composer-input {
        flex: 1;
        min-width: 0;
        resize: none;
        max-height: 8lh;
        padding: var(--spacing-xs) var(--spacing-sm);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
    }

    .composer-input:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -1px;
    }

    .composer-button {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        flex: none;
        border: none;
        border-radius: var(--radius-md);
        color: var(--color-accent-fg);
        background: var(--color-accent);
    }

    .composer-button:hover {
        background: var(--color-accent-hover);
    }

    .composer-button:disabled {
        opacity: 0.5;
        cursor: default;
    }

    .composer-button.stop {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }
</style>
