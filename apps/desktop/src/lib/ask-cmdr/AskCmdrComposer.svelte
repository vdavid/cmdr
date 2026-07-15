<!--
  The Ask Cmdr message composer: staged attachment chips, a growing textarea, an "ask
  about selection" button, and a send/stop button. Enter sends, Shift+Enter inserts a
  newline, Escape returns focus to the active pane. Disabled while a turn streams
  (single-flight per thread); the send button flips to Stop then.

  A file or folder dragged from a pane (or Finder) onto the composer attaches by
  reference — see `ask-cmdr-drop.ts` for why this is a native webview drag, not HTML5.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, onSpecificSettingChange, type AiProvider } from '$lib/settings'
    import { askCmdrSelectionAttachments } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import AskCmdrAttachmentChip from './AskCmdrAttachmentChip.svelte'
    import { installComposerDrop } from './ask-cmdr-drop'
    import {
        addAttachments,
        askCmdrState,
        markRailFocused,
        removeAttachment,
        returnFocusToPane,
        sendMessage,
        stopStreaming,
    } from './ask-cmdr-trigger.svelte'

    const log = getAppLogger('askCmdr')

    let text = $state('')
    let textarea = $state<HTMLTextAreaElement | null>(null)
    let composerEl = $state<HTMLDivElement | null>(null)
    let composing = $state(false)
    let dragOver = $state(false)

    // Focus the composer when it mounts (the rail mounts on open, so opening focuses here).
    $effect(() => {
        textarea?.focus()
    })

    // Subscribe the composer as a native drag-drop target (no-op outside a Tauri webview).
    onMount(() => {
        let unlisten: (() => void) | null = null
        void installComposerDrop(
            () => composerEl?.getBoundingClientRect() ?? null,
            (active) => (dragOver = active),
            (refs) => { addAttachments(refs); },
        ).then((u) => (unlisten = u))
        return () => unlisten?.()
    })

    // Which AI provider Ask Cmdr uses, read live so flipping it in settings gates Send
    // immediately (no restart). Provider off ⇒ can't start a new turn; an in-flight turn
    // is unaffected (Send is already disabled while streaming).
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    $effect(() => onSpecificSettingChange('ai.provider', (_id, v) => { provider = v }))
    const providerOff = $derived(provider === 'off')

    const streaming = $derived(askCmdrState.streaming)
    const canSend = $derived(text.trim().length > 0 && !providerOff)
    const attachments = $derived(askCmdrState.attachments)

    function submit(): void {
        if (askCmdrState.streaming || !canSend) return
        sendMessage(text)
        text = ''
    }

    async function attachSelection(): Promise<void> {
        try {
            addAttachments(await askCmdrSelectionAttachments())
        } catch (e) {
            log.warn('attaching the selection failed: {error}', { error: String(e) })
        }
        textarea?.focus()
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

<div class="composer" class:drag-over={dragOver} bind:this={composerEl}>
    {#if attachments.length > 0}
        <div class="attachments">
            {#each attachments as attachment (attachment.path)}
                <AskCmdrAttachmentChip {attachment} onRemove={removeAttachment} />
            {/each}
        </div>
    {/if}
    <div class="composer-row">
        <button
            type="button"
            class="composer-button ghost"
            onclick={attachSelection}
            aria-label={tString('askCmdr.composer.attach')}
            title={tString('askCmdr.composer.attach')}
        >
            <Icon name="paperclip" size={16} aria-hidden="true" />
        </button>
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
    {#if providerOff}
        <p class="provider-off-hint">{tString('askCmdr.composer.providerOff')}</p>
    {/if}
    {#if dragOver}
        <div class="drop-hint" aria-hidden="true">
            <Icon name="paperclip" size={16} aria-hidden="true" />
            <span>{tString('askCmdr.composer.dropHint')}</span>
        </div>
    {/if}
</div>

<style>
    .composer {
        position: relative;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .composer.drag-over {
        outline: 2px dashed var(--color-accent);
        outline-offset: -2px;
    }

    .attachments {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xxs);
    }

    .composer-row {
        display: flex;
        align-items: flex-end;
        gap: var(--spacing-xs);
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

    .composer-button.ghost {
        color: var(--color-text-secondary);
        background: none;
    }

    .composer-button.ghost:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .provider-off-hint {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.4;
    }

    .drop-hint {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        background: var(--color-bg-glass);
        border-radius: var(--radius-md);
        pointer-events: none;
    }
</style>
